//! The macOS tap: a `CGEventTap` on its own thread and run loop, like `InputTap.swift`.

use super::{Action, InputError, InputEvent, Kind, Modifier, MouseButton, Shared};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::mach_port::{CFMachPortCreateRunLoopSource, CFMachPortInvalidate, CFMachPortRef};
use core_foundation::runloop::{
    CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRunInMode, CFRunLoopSourceRef, CFRunLoopStop,
    kCFRunLoopCommonModes, kCFRunLoopDefaultMode,
};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

type CGEventRef = *mut c_void;
type CGEventTapCallBack =
    extern "C" fn(proxy: *mut c_void, kind: u32, event: CGEventRef, refcon: *mut c_void) -> CGEventRef;

use core_graphics::geometry::CGPoint;

// CGEventType
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const MOUSE_MOVED: u32 = 5;
const LEFT_DRAGGED: u32 = 6;
const RIGHT_DRAGGED: u32 = 7;
const KEY_DOWN: u32 = 10;
const FLAGS_CHANGED: u32 = 12;
const OTHER_DOWN: u32 = 25;
const OTHER_UP: u32 = 26;
const OTHER_DRAGGED: u32 = 27;
const DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

// CGEventFlags
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_ALT: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;

const SESSION_TAP: u32 = 1; // kCGSessionEventTap
const HEAD_INSERT: u32 = 0; // kCGHeadInsertEventTap
const DEFAULT_TAP: u32 = 0; // kCGEventTapOptionDefault
const HID_SYSTEM_STATE: i32 = 1; // kCGEventSourceStateHIDSystemState
const BUTTON_NUMBER_FIELD: u32 = 3; // kCGMouseEventButtonNumber
const CLOCK_UPTIME_RAW: u32 = 8;
const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventSetType(event: CGEventRef, kind: u32);
    fn CGEventGetTimestamp(event: CGEventRef) -> u64;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventSourceCounterForEventType(state: i32, kind: u32) -> u32;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

unsafe extern "C" {
    fn clock_gettime_nsec_np(clock: u32) -> u64;
    fn pthread_set_qos_class_self_np(qos: u32, relative_priority: i32) -> i32;
}

fn flag(m: Modifier) -> u64 {
    match m {
        Modifier::Alt => FLAG_ALT,
        Modifier::Command => FLAG_COMMAND,
        Modifier::Control => FLAG_CONTROL,
    }
}

fn bit(kind: u32) -> u64 {
    1 << kind
}

/// What the callback sees through `refcon`. Lives until the tap thread has ended.
struct Ctx {
    shared: Arc<Shared>,
    port: AtomicPtr<c_void>,
}

pub(crate) struct Tap {
    shared: Arc<Shared>,
    ctx: *mut Ctx,
    port: CFMachPortRef,
    source: CFRunLoopSourceRef,
    run_loop: usize,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

// The CF objects are only touched through thread-safe CF calls (enable, stop, release).
unsafe impl Send for Tap {}
unsafe impl Sync for Tap {}

impl Tap {
    pub fn start(on_event: Box<dyn Fn(InputEvent) + Send>) -> Result<Tap, InputError> {
        let shared = Arc::new(Shared::new(on_event));
        let ctx = Box::into_raw(Box::new(Ctx { shared: shared.clone(), port: AtomicPtr::new(std::ptr::null_mut()) }));
        let mask = [
            LEFT_DOWN, LEFT_UP, RIGHT_DOWN, RIGHT_UP, OTHER_DOWN, OTHER_UP, LEFT_DRAGGED, RIGHT_DRAGGED,
            OTHER_DRAGGED, FLAGS_CHANGED,
        ]
        .into_iter()
        .fold(0, |m, k| m | bit(k));

        // SAFETY: `ctx` stays valid until after the tap thread is joined in `drop`.
        let port = unsafe { CGEventTapCreate(SESSION_TAP, HEAD_INSERT, DEFAULT_TAP, mask, callback, ctx as *mut c_void) };
        if port.is_null() {
            // SAFETY: nothing else has the pointer.
            drop(unsafe { Box::from_raw(ctx) });
            return Err(InputError::NotTrusted);
        }
        // SAFETY: `port` is a live event tap.
        let source = unsafe {
            CGEventTapEnable(port, false);
            (*ctx).port.store(port as *mut c_void, Ordering::Release);
            CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0)
        };
        if source.is_null() {
            // SAFETY: as above; the port is ours to release.
            unsafe {
                CFMachPortInvalidate(port);
                CFRelease(port as CFTypeRef);
                drop(Box::from_raw(ctx));
            }
            return Err(InputError::Failed("CFMachPortCreateRunLoopSource".into()));
        }

        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<usize>();
        let source_addr = source as usize;
        let stop_flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("TinyHand event tap".into())
            .spawn(move || unsafe {
                // SAFETY: plain CF/pthread calls on this thread's own run loop; the source
                // outlives the thread (released after join).
                pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
                let rl = CFRunLoopGetCurrent();
                CFRunLoopAddSource(rl, source_addr as CFRunLoopSourceRef, kCFRunLoopCommonModes);
                let _ = ready_tx.send(rl as usize);
                // Not CFRunLoopRun: a stop that lands before the loop starts would be lost.
                while !stop_flag.load(Ordering::Acquire) {
                    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0, 0);
                }
            })
            .map_err(|e| InputError::Failed(e.to_string()))?;
        let run_loop = ready_rx.recv().map_err(|_| InputError::Failed("event tap thread died".into()))?;

        Ok(Tap { shared, ctx, port, source, run_loop, stop, thread: Some(thread) })
    }

    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    pub fn enabled_changed(&self, on: bool) {
        // SAFETY: the port lives as long as `self`.
        unsafe { CGEventTapEnable(self.port, on) };
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        // SAFETY: the run loop and port are alive until released here; `ctx` is freed only
        // after the thread that calls back with it has ended.
        unsafe {
            CGEventTapEnable(self.port, false);
            self.stop.store(true, Ordering::Release);
            CFRunLoopStop(self.run_loop as _);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
            CFMachPortInvalidate(self.port);
            CFRelease(self.source as CFTypeRef);
            CFRelease(self.port as CFTypeRef);
            drop(Box::from_raw(self.ctx));
        }
    }
}

extern "C" fn callback(_proxy: *mut c_void, kind: u32, event: CGEventRef, refcon: *mut c_void) -> CGEventRef {
    if refcon.is_null() || event.is_null() {
        return event;
    }
    // SAFETY: `refcon` is the `Ctx` from `start`, alive until the thread is joined.
    let ctx = unsafe { &*(refcon as *const Ctx) };
    // A panic must not unwind into CoreGraphics; let the event through instead.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle(ctx, kind, event))).unwrap_or(event)
}

fn handle(ctx: &Ctx, kind: u32, event: CGEventRef) -> CGEventRef {
    let shared = &ctx.shared;
    // SAFETY (whole function): `event` is the live event CoreGraphics handed us.
    let stamp = unsafe { CGEventGetTimestamp(event) };
    if stamp > 0 {
        let now = unsafe { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) };
        shared.note_wait(now.wrapping_sub(stamp) as f64 / 1_000_000.0);
    }

    if kind == DISABLED_BY_TIMEOUT || kind == DISABLED_BY_USER_INPUT {
        if kind == DISABLED_BY_TIMEOUT {
            eprintln!("event tap was disabled by a timeout; re-enabled");
        }
        let port = ctx.port.load(Ordering::Acquire);
        if shared.enabled() && !port.is_null() {
            unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
        }
        return event;
    }

    if kind == FLAGS_CHANGED {
        let f = unsafe { CGEventGetFlags(event) };
        shared.emit(InputEvent::Modifiers {
            shift: f & FLAG_SHIFT != 0,
            control: f & FLAG_CONTROL != 0,
            alt: f & FLAG_ALT != 0,
            command: f & FLAG_COMMAND != 0,
        });
        return event;
    }

    let other = || MouseButton::Other(unsafe { CGEventGetIntegerValueField(event, BUTTON_NUMBER_FIELD) } as u32);
    let k = match kind {
        LEFT_DOWN => Kind::Down(MouseButton::Left),
        RIGHT_DOWN => Kind::Down(MouseButton::Right),
        OTHER_DOWN => Kind::Down(other()),
        LEFT_UP => Kind::Up(MouseButton::Left),
        RIGHT_UP => Kind::Up(MouseButton::Right),
        OTHER_UP => Kind::Up(other()),
        LEFT_DRAGGED => Kind::Drag { left: true },
        RIGHT_DRAGGED | OTHER_DRAGGED => Kind::Drag { left: false },
        _ => return event,
    };
    let p = unsafe { CGEventGetLocation(event) };
    let key_held = |m: Modifier| {
        let f = flag(m);
        unsafe { CGEventGetFlags(event) & f != 0 || CGEventSourceFlagsState(HID_SYSTEM_STATE) & f != 0 }
    };
    match shared.decide(k, p.x, p.y, key_held, screen_locked) {
        Action::Pass | Action::PassStripping(None) => event,
        Action::PassStripping(Some(m)) => {
            unsafe { CGEventSetFlags(event, CGEventGetFlags(event) & !flag(m)) };
            event
        }
        Action::Swallow => std::ptr::null_mut(),
        Action::Move => {
            unsafe { CGEventSetType(event, MOUSE_MOVED) };
            event
        }
    }
}

/// Backstop for lock paths that don't post `com.apple.screenIsLocked`.
pub fn screen_locked() -> bool {
    // SAFETY: a Copy function; null when there is no session.
    let dict = unsafe { CGSessionCopyCurrentDictionary() };
    if dict.is_null() {
        return false;
    }
    // SAFETY: we own the +1 reference from the Copy call.
    let dict: CFDictionary<CFString, core_foundation::base::CFType> = unsafe { CFDictionary::wrap_under_create_rule(dict) };
    dict.find(CFString::from_static_string("CGSSessionScreenIsLocked"))
        .and_then(|v| v.downcast::<CFBoolean>())
        .is_some_and(bool::from)
}

pub fn is_trusted() -> bool {
    // SAFETY: no arguments.
    unsafe { AXIsProcessTrusted() != 0 }
}

pub fn request_trust() {
    // SAFETY: the key is a constant CFString from HIServices.
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
    }
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

extern "C" fn noop(_: *mut c_void, _: u32, event: CGEventRef, _: *mut c_void) -> CGEventRef {
    event
}

/// Creates a tap for null events only (so it intercepts nothing) and throws it away.
pub fn can_tap() -> bool {
    // SAFETY: the probe tap is disabled at once, never added to a run loop, and released.
    unsafe {
        let port = CGEventTapCreate(SESSION_TAP, HEAD_INSERT, DEFAULT_TAP, bit(0), noop, std::ptr::null_mut());
        if port.is_null() {
            return false;
        }
        CGEventTapEnable(port, false);
        CFMachPortInvalidate(port);
        CFRelease(port as CFTypeRef);
        true
    }
}

pub fn input_counter() -> u64 {
    [KEY_DOWN, LEFT_DOWN, RIGHT_DOWN, OTHER_DOWN]
        .into_iter()
        // SAFETY: plain query.
        .map(|k| unsafe { CGEventSourceCounterForEventType(HID_SYSTEM_STATE, k) } as u64)
        .sum()
}

pub fn left_button_down() -> bool {
    // SAFETY: plain query; button 0 is the left button.
    unsafe { CGEventSourceButtonState(HID_SYSTEM_STATE, 0) }
}
