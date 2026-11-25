//! The parts GPUI doesn't do: making the overlay ignore clicks and float above everything,
//! and reading the mouse while the overlay never receives mouse events.
//!
//! This only *watches* the left button. Swallowing clicks so the app underneath doesn't
//! get them (TinyHand's "tap") needs an event tap on macOS and a low-level mouse hook on
//! Windows, and isn't part of this spike.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Mouse position in the overlay's coordinates (points, y down, origin at the top left of
/// the display it covers) and whether the left button is down.
pub struct Mouse {
    pub x: f32,
    pub y: f32,
    pub down: bool,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use objc::runtime::{NO, Object, YES};
    use objc::{class, msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    }

    pub fn hide_from_dock() {
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            // NSApplicationActivationPolicyAccessory
            let _: bool = msg_send![app, setActivationPolicy: 1i64];
        }
    }

    pub fn make_overlay(window: &impl HasWindowHandle) -> Result<(), String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return Err("not an AppKit window".into());
        };
        unsafe {
            let view = h.ns_view.as_ptr() as *mut Object;
            let ns_window: *mut Object = msg_send![view, window];
            let _: () = msg_send![ns_window, setIgnoresMouseEvents: YES];
            let _: () = msg_send![ns_window, setHasShadow: NO];
            // Same level as the Swift app: above the screen saver level.
            let _: () = msg_send![ns_window, setLevel: 1002i64];
            // canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
            let _: () = msg_send![ns_window, setCollectionBehavior: (1u64 | 16 | 64 | 256)];
        }
        Ok(())
    }

    /// `display_height` is the height of the main display in points; AppKit counts y up
    /// from its bottom edge.
    pub fn mouse(display_height: f32, _scale: f32) -> Mouse {
        unsafe {
            let p: NSPoint = msg_send![class!(NSEvent), mouseLocation];
            Mouse {
                x: p.x as f32,
                y: display_height - p.y as f32,
                // kCGEventSourceStateCombinedSessionState, kCGMouseButtonLeft
                down: CGEventSourceButtonState(0, 0),
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub fn hide_from_dock() {}

    pub fn make_overlay(window: &impl HasWindowHandle) -> Result<(), String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return Err("not a Win32 window".into());
        };
        let hwnd = h.hwnd.get() as HWND;
        unsafe {
            // WS_EX_TRANSPARENT lets clicks fall through, but only on a layered window.
            // A layered window stays invisible until its attributes are set once.
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let extra = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | extra);
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
        Ok(())
    }

    /// Windows reports the cursor in physical pixels.
    pub fn mouse(_display_height: f32, scale: f32) -> Mouse {
        let mut p = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut p);
            Mouse {
                x: p.x as f32 / scale,
                y: p.y as f32 / scale,
                down: (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0,
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::*;

    pub fn hide_from_dock() {}

    pub fn make_overlay(_window: &impl HasWindowHandle) -> Result<(), String> {
        Err("click-through overlay isn't written for this platform yet".into())
    }

    pub fn mouse(_display_height: f32, _scale: f32) -> Mouse {
        Mouse { x: 0.0, y: 0.0, down: false }
    }
}

pub use imp::*;
