//! `Shell_NotifyIconW` with a hidden window of our own that receives the icon's mouse
//! messages. The window belongs to the thread that made the `Tray` (GPUI's main thread),
//! whose message loop delivers them.
//!
//! The window is a hidden top-level tool window rather than a message-only one: only
//! top-level windows get the `TaskbarCreated` broadcast (sent when Explorer restarts, when
//! the icon must be added again), and `TrackPopupMenu` needs a window that can be made
//! foreground so the menu closes when the user clicks elsewhere.

use super::{ATTENTION, MenuEntry, Rect, TrayEvent, TrayIcons, tinted};
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, GetDC, ReleaseDC,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CALLBACK_MESSAGE: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
const CLASS_NAME: &str = "TinyHandTray";

struct State {
    on_event: Box<dyn Fn(TrayEvent) + Send>,
    entries: RefCell<Vec<MenuEntry>>,
    taskbar_created: u32,
    /// The icon as last given to the shell, re-added after Explorer restarts.
    data: RefCell<NOTIFYICONDATAW>,
}

pub struct Tray {
    state: Box<State>,
    hwnd: HWND,
    /// on, off, tinted on, tinted off.
    icons: [HICON; 4],
    active: bool,
    attention: bool,
}

impl Tray {
    pub fn new(icons: TrayIcons, on_event: Box<dyn Fn(TrayEvent) + Send>) -> Result<Tray, String> {
        unsafe {
            let (w, h) = (icons.width, icons.height);
            let hicons = [
                hicon(&icons.on_rgba, w, h),
                hicon(&icons.off_rgba, w, h),
                hicon(&tinted(&icons.on_rgba, ATTENTION), w, h),
                hicon(&tinted(&icons.off_rgba, ATTENTION), w, h),
            ];
            if hicons.iter().any(|i| i.is_null()) {
                for i in hicons.into_iter().filter(|i| !i.is_null()) {
                    DestroyIcon(i);
                }
                return Err(format!("couldn't make the tray icons: {}", std::io::Error::last_os_error()));
            }

            let instance = GetModuleHandleW(null());
            let class = wide(CLASS_NAME);
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            // Fails harmlessly if a previous Tray registered it.
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if hwnd.is_null() {
                for i in hicons {
                    DestroyIcon(i);
                }
                return Err(format!("couldn't make the tray window: {}", std::io::Error::last_os_error()));
            }

            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd;
            data.uID = ICON_ID;
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            data.uCallbackMessage = CALLBACK_MESSAGE;
            data.hIcon = hicons[1];
            copy_tip(&mut data, "TinyHand");

            let state = Box::new(State {
                on_event,
                entries: RefCell::new(Vec::new()),
                taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
                data: RefCell::new(data),
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*state as *const State as isize);

            if Shell_NotifyIconW(NIM_ADD, &*state.data.borrow()) == 0 {
                DestroyWindow(hwnd);
                for i in hicons {
                    DestroyIcon(i);
                }
                return Err("the shell refused the tray icon".into());
            }
            Ok(Tray { state, hwnd, icons: hicons, active: false, attention: false })
        }
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        self.update_icon();
    }

    pub fn set_attention(&mut self, attention: bool) {
        self.attention = attention;
        self.update_icon();
    }

    fn update_icon(&self) {
        let index = usize::from(!self.active) + if self.attention { 2 } else { 0 };
        let mut data = self.state.data.borrow_mut();
        data.hIcon = self.icons[index];
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &*data) };
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        let mut data = self.state.data.borrow_mut();
        copy_tip(&mut data, tooltip);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &*data) };
    }

    pub fn set_menu(&mut self, entries: Vec<MenuEntry>) {
        *self.state.entries.borrow_mut() = entries;
    }

    pub fn frame(&self) -> Option<Rect> {
        None
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &*self.state.data.borrow());
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
            for i in self.icons {
                DestroyIcon(i);
            }
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let state = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref();
        let Some(state) = state else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        if msg == CALLBACK_MESSAGE {
            // Without NIM_SETVERSION, lparam is the mouse message itself.
            match lparam as u32 {
                WM_LBUTTONUP => (state.on_event)(TrayEvent::Clicked),
                WM_RBUTTONUP => show_menu(hwnd, state),
                _ => {}
            }
            return 0;
        }
        if msg == state.taskbar_created && state.taskbar_created != 0 {
            Shell_NotifyIconW(NIM_ADD, &*state.data.borrow());
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Builds the menu from the current entries and runs it at the cursor. The entries are
/// copied first: TrackPopupMenu runs a modal loop that dispatches the thread's other
/// messages, which may call `set_menu`.
unsafe fn show_menu(hwnd: HWND, state: &State) {
    unsafe {
        let entries = state.entries.borrow().clone();
        if entries.is_empty() {
            return;
        }
        let menu = CreatePopupMenu();
        let mut ids = Vec::new();
        for entry in &entries {
            match entry {
                MenuEntry::Separator => {
                    AppendMenuW(menu, MF_SEPARATOR, 0, null());
                }
                MenuEntry::Item(m) if m.visible => {
                    ids.push(m.id.clone());
                    let mut flags = MF_STRING;
                    if m.checked {
                        flags |= MF_CHECKED;
                    }
                    if !m.enabled {
                        flags |= MF_GRAYED;
                    }
                    // A tab right-aligns the hint, where Windows menus put shortcuts.
                    AppendMenuW(menu, flags, ids.len(), wide(&m.label("\t")).as_ptr());
                }
                MenuEntry::Item(_) => {}
            }
        }
        let mut p = POINT { x: 0, y: 0 };
        GetCursorPos(&mut p);
        // The documented dance so the menu closes when the user clicks elsewhere.
        SetForegroundWindow(hwnd);
        (state.on_event)(TrayEvent::MenuOpened);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN,
            p.x,
            p.y,
            0,
            hwnd,
            null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        (state.on_event)(TrayEvent::MenuClosed);
        if let Some(id) = usize::try_from(chosen).ok().filter(|&c| c > 0).and_then(|c| ids.get(c - 1)) {
            (state.on_event)(TrayEvent::MenuItem(id.clone()));
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn copy_tip(data: &mut NOTIFYICONDATAW, tip: &str) {
    let mut units: Vec<u16> = tip.encode_utf16().take(data.szTip.len() - 1).collect();
    units.push(0);
    data.szTip = [0; 128];
    data.szTip[..units.len()].copy_from_slice(&units);
}

/// An icon from straight RGBA: a 32-bit top-down DIB (BGRA) plus an empty mask, which
/// Windows reads as "use the alpha channel".
unsafe fn hicon(rgba: &[u8], width: u32, height: u32) -> HICON {
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            },
            ..std::mem::zeroed()
        };
        let dc = GetDC(null_mut());
        let mut bits: *mut std::ffi::c_void = null_mut();
        let color = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        ReleaseDC(null_mut(), dc);
        if color.is_null() || bits.is_null() {
            return null_mut();
        }
        let out = std::slice::from_raw_parts_mut(bits as *mut u8, rgba.len());
        for (o, i) in out.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            o.copy_from_slice(&[i[2], i[1], i[0], i[3]]);
        }
        let mask_bytes = vec![0u8; (width.div_ceil(16) * 2 * height) as usize];
        let mask = CreateBitmap(width as i32, height as i32, 1, 1, mask_bytes.as_ptr().cast());
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}
