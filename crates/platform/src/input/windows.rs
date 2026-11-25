//! The Windows tap: `WH_MOUSE_LL` and `WH_KEYBOARD_LL` hooks on a thread of their own.
//!
//! A low-level hook can let an event through or drop it (return 1), not change it. So a
//! swallowed press is dropped and the moves while it is held pass unchanged: apps only ever
//! see plain moves, which is what rewriting drags to moves does on macOS. A press that passes
//! for the interact key keeps the modifier.
//!
//! Windows doesn't update `GetAsyncKeyState` for dropped button events, so the hooks track
//! the buttons themselves; `left_button_down` uses that while a tap is running.

use super::{Action, InputError, InputEvent, Kind, Modifier, MouseButton, Shared};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

/// The running tap's state, for the hook procedures (which get no user pointer).
static CURRENT: AtomicPtr<Ctx> = AtomicPtr::new(std::ptr::null_mut());
/// Key and button presses seen by the hooks.
static COUNTER: AtomicU64 = AtomicU64::new(0);
/// Bits per side (low nibble left keys, high nibble right keys) of SHIFT..COMMAND.
static MODIFIERS: AtomicU8 = AtomicU8::new(0);
/// LEFT / OTHERS buttons held, as the hooks saw them.
static BUTTONS: AtomicU8 = AtomicU8::new(0);
/// Whether a tap's hooks are installed.
static RUNNING: AtomicBool = AtomicBool::new(false);

const SHIFT: u8 = 1;
const CONTROL: u8 = 2;
const ALT: u8 = 4;
const COMMAND: u8 = 8;
const LEFT: u8 = 1;
const OTHERS: u8 = 2;

struct Ctx {
    shared: Arc<Shared>,
}

fn modifiers() -> u8 {
    let m = MODIFIERS.load(Ordering::Relaxed);
    (m | (m >> 4)) & 0xF
}

pub(crate) struct Tap {
    shared: Arc<Shared>,
    ctx: *mut Ctx,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

// `ctx` is only freed in `drop`, after the hook thread has ended.
unsafe impl Send for Tap {}
unsafe impl Sync for Tap {}

impl Tap {
    pub fn start(on_event: Box<dyn Fn(InputEvent) + Send>) -> Result<Tap, InputError> {
        let shared = Arc::new(Shared::new(on_event));
        let ctx = Box::into_raw(Box::new(Ctx { shared: shared.clone() }));
        if CURRENT.compare_exchange(std::ptr::null_mut(), ctx, Ordering::AcqRel, Ordering::Acquire).is_err() {
            // SAFETY: nobody else saw `ctx`.
            drop(unsafe { Box::from_raw(ctx) });
            return Err(InputError::AlreadyRunning);
        }
        MODIFIERS.store(0, Ordering::Relaxed);
        BUTTONS.store(0, Ordering::Relaxed);
        let fail = |why: String| {
            CURRENT.store(std::ptr::null_mut(), Ordering::Release);
            // SAFETY: the hooks never ran or have been removed.
            drop(unsafe { Box::from_raw(ctx) });
            Err(InputError::Failed(why))
        };

        let (tx, rx) = mpsc::channel::<Result<u32, String>>();
        let spawned = std::thread::Builder::new().name("TinyHand input hooks".into()).spawn(move || unsafe {
            // SAFETY: plain Win32 calls on this thread; the hooks are removed before it ends.
            let module = GetModuleHandleW(std::ptr::null());
            let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
            let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0);
            if mouse.is_null() || keyboard.is_null() {
                for h in [mouse, keyboard] {
                    if !h.is_null() {
                        UnhookWindowsHookEx(h);
                    }
                }
                let _ = tx.send(Err("SetWindowsHookExW".into()));
                return;
            }
            // Make the thread's message queue before anyone posts WM_QUIT to it.
            let mut msg: MSG = std::mem::zeroed();
            PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
            RUNNING.store(true, Ordering::Release);
            let _ = tx.send(Ok(GetCurrentThreadId()));
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
            UnhookWindowsHookEx(mouse);
            UnhookWindowsHookEx(keyboard);
            RUNNING.store(false, Ordering::Release);
        });
        let thread = match spawned {
            Ok(t) => t,
            Err(e) => return fail(e.to_string()),
        };
        match rx.recv() {
            Ok(Ok(thread_id)) => Ok(Tap { shared, ctx, thread_id, thread: Some(thread) }),
            Ok(Err(why)) => {
                let _ = thread.join();
                fail(why)
            }
            Err(_) => {
                let _ = thread.join();
                fail("hook thread died".into())
            }
        }
    }

    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    pub fn enabled_changed(&self, _on: bool) {}
}

impl Drop for Tap {
    fn drop(&mut self) {
        // SAFETY: the thread's queue exists (made before its id was sent back).
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        CURRENT.store(std::ptr::null_mut(), Ordering::Release);
        // SAFETY: the hooks are gone, so nothing reads `ctx` any more.
        drop(unsafe { Box::from_raw(self.ctx) });
    }
}

fn ctx() -> Option<&'static Ctx> {
    // SAFETY: CURRENT is only non-null while the hook thread (the only caller) runs.
    unsafe { CURRENT.load(Ordering::Acquire).as_ref() }
}

fn key_down(vk: u16) -> bool {
    // SAFETY: plain query.
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 && let Some(ctx) = ctx() {
        // SAFETY: for WH_MOUSE_LL, lparam points at an MSLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let swallow = std::panic::catch_unwind(|| on_mouse(ctx, wparam as u32, info)).unwrap_or(false);
        if swallow {
            return 1;
        }
    }
    // SAFETY: passing the event on, as every hook must.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn on_mouse(ctx: &Ctx, msg: u32, info: &MSLLHOOKSTRUCT) -> bool {
    let xbutton = || MouseButton::Other(2 + (info.mouseData >> 16)); // XBUTTON1 = 3, XBUTTON2 = 4
    let (kind, bit) = match msg {
        WM_LBUTTONDOWN => (Kind::Down(MouseButton::Left), LEFT),
        WM_RBUTTONDOWN => (Kind::Down(MouseButton::Right), OTHERS),
        WM_MBUTTONDOWN => (Kind::Down(MouseButton::Other(2)), OTHERS),
        WM_XBUTTONDOWN => (Kind::Down(xbutton()), OTHERS),
        WM_LBUTTONUP => (Kind::Up(MouseButton::Left), LEFT),
        WM_RBUTTONUP => (Kind::Up(MouseButton::Right), OTHERS),
        WM_MBUTTONUP => (Kind::Up(MouseButton::Other(2)), OTHERS),
        WM_XBUTTONUP => (Kind::Up(xbutton()), OTHERS),
        WM_MOUSEMOVE => {
            let held = BUTTONS.load(Ordering::Relaxed);
            if held == 0 {
                return false;
            }
            (Kind::Drag { left: held & LEFT != 0 }, 0)
        }
        _ => return false,
    };
    match kind {
        Kind::Down(_) => {
            COUNTER.fetch_add(1, Ordering::Relaxed);
            BUTTONS.fetch_or(bit, Ordering::Relaxed);
        }
        Kind::Up(_) => {
            BUTTONS.fetch_and(!bit, Ordering::Relaxed);
        }
        Kind::Drag { .. } => {}
    }
    let mods = modifiers();
    let key_held = |m: Modifier| match m {
        Modifier::Alt => mods & ALT != 0 || key_down(VK_MENU),
        Modifier::Control => mods & CONTROL != 0 || key_down(VK_CONTROL),
        Modifier::Command => mods & COMMAND != 0 || key_down(VK_LWIN) || key_down(VK_RWIN),
    };
    let action = ctx.shared.decide(kind, info.pt.x as f64, info.pt.y as f64, key_held, || false);
    action == Action::Swallow
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 && let Some(ctx) = ctx() {
        // SAFETY: for WH_KEYBOARD_LL, lparam points at a KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let _ = std::panic::catch_unwind(|| on_key(ctx, wparam as u32, info.vkCode));
    }
    // SAFETY: as in `mouse_proc`.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn on_key(ctx: &Ctx, msg: u32, vk: u32) {
    let down = match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => true,
        WM_KEYUP | WM_SYSKEYUP => false,
        _ => return,
    };
    let bit = match vk as u16 {
        VK_LSHIFT => SHIFT,
        VK_LCONTROL => CONTROL,
        VK_LMENU => ALT,
        VK_LWIN => COMMAND,
        VK_RSHIFT => SHIFT << 4,
        VK_RCONTROL => CONTROL << 4,
        VK_RMENU => ALT << 4,
        VK_RWIN => COMMAND << 4,
        _ => {
            if down {
                COUNTER.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }
    };
    let before = modifiers();
    if down {
        MODIFIERS.fetch_or(bit, Ordering::Relaxed);
    } else {
        MODIFIERS.fetch_and(!bit, Ordering::Relaxed);
    }
    let now = modifiers();
    if now != before && ctx.shared.enabled() {
        ctx.shared.emit(InputEvent::Modifiers {
            shift: now & SHIFT != 0,
            control: now & CONTROL != 0,
            alt: now & ALT != 0,
            command: now & COMMAND != 0,
        });
    }
}

pub fn is_trusted() -> bool {
    true
}

pub fn request_trust() {}

pub fn can_tap() -> bool {
    true
}

pub fn input_counter() -> u64 {
    COUNTER.load(Ordering::Relaxed)
}

pub fn left_button_down() -> bool {
    if RUNNING.load(Ordering::Acquire) {
        BUTTONS.load(Ordering::Relaxed) & LEFT != 0
    } else {
        key_down(VK_LBUTTON)
    }
}
