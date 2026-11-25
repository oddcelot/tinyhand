//! macOS menu bar and open menus (ported from `OverlayController.menuBarRect` / `openMenus`).
//!
//! All rectangles are in global screen points, y down, origin at the top left of the primary
//! display, like [`Rect`] and [`Display`]. On other systems everything here is empty.

use crate::{Display, input::Rect};

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::CFString;
    use objc::runtime::Object;
    use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    // SAFETY: matches the layout and encoding of NSRect on 64-bit macOS.
    unsafe impl Encode for CGRect {
        fn encode() -> Encoding {
            // SAFETY: a well-formed struct encoding string.
            unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
        }
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
        fn CGWindowLevelForKey(key: i32) -> i32;
        fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut CGRect) -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
        fn CFNumberGetValue(number: *const c_void, the_type: isize, out: *mut c_void) -> bool;
    }

    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
    const NULL_WINDOW_ID: u32 = 0;
    const POP_UP_MENU_WINDOW_LEVEL_KEY: i32 = 11;
    const CF_NUMBER_SINT32_TYPE: isize = 3;

    /// The AppKit frames (y up) of the screen showing `display`, and of the primary screen's
    /// height: `(frame, visible_frame, primary_height)`.
    fn screen_frames(display: &Display) -> Option<(CGRect, CGRect, f64)> {
        // SAFETY: plain AppKit calls on the main thread; the pool frees what they autorelease.
        unsafe {
            let pool: *mut Object = msg_send![class!(NSAutoreleasePool), new];
            let result = (|| {
                let screens: *mut Object = msg_send![class!(NSScreen), screens];
                if screens.is_null() {
                    return None;
                }
                let count: usize = msg_send![screens, count];
                if count == 0 {
                    return None;
                }
                let primary: *mut Object = msg_send![screens, objectAtIndex: 0usize];
                let primary_frame: CGRect = msg_send![primary, frame];
                let key = CFString::from_static_string("NSScreenNumber");
                for i in 0..count {
                    let screen: *mut Object = msg_send![screens, objectAtIndex: i];
                    let desc: *mut Object = msg_send![screen, deviceDescription];
                    if desc.is_null() {
                        continue;
                    }
                    let number: *mut Object =
                        msg_send![desc, objectForKey: key.as_concrete_TypeRef() as *mut Object];
                    if number.is_null() {
                        continue;
                    }
                    let id: u32 = msg_send![number, unsignedIntValue];
                    if id as u64 == display.id {
                        let frame: CGRect = msg_send![screen, frame];
                        let visible: CGRect = msg_send![screen, visibleFrame];
                        return Some((frame, visible, primary_frame.size.height));
                    }
                }
                None
            })();
            let _: () = msg_send![pool, drain];
            result
        }
    }

    /// Flips an AppKit rect (y up from the bottom of the primary screen) to y down.
    fn to_y_down(r: CGRect, primary_h: f64) -> Rect {
        Rect::new(
            r.origin.x,
            primary_h - (r.origin.y + r.size.height),
            r.size.width,
            r.size.height,
        )
    }

    /// The usable area of a display (without menu bar and Dock), or `None` if it isn't found.
    pub fn visible_frame(display: &Display) -> Option<Rect> {
        let (_, visible, primary_h) = screen_frames(display)?;
        Some(to_y_down(visible, primary_h))
    }

    /// The menu bar strip along the top of a display, at least 24 points tall.
    pub fn menu_bar_rect(display: &Display) -> Option<Rect> {
        let (frame, visible, primary_h) = screen_frames(display)?;
        let h = (frame.origin.y + frame.size.height - (visible.origin.y + visible.size.height)).max(24.0);
        let top = to_y_down(frame, primary_h);
        Some(Rect::new(top.x, top.y, top.w, h))
    }

    /// Bounds of the on-screen pop-up menu windows, in global points (y down).
    pub fn open_menu_rects() -> Vec<Rect> {
        let mut out = Vec::new();
        let layer_key = CFString::from_static_string("kCGWindowLayer");
        let bounds_key = CFString::from_static_string("kCGWindowBounds");
        // SAFETY: the list is a +1 CFArray we release; every value read from it is only
        // borrowed while the array is alive, and the key strings outlive the loop.
        unsafe {
            let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, NULL_WINDOW_ID);
            if list.is_null() {
                return out;
            }
            let menu_level = CGWindowLevelForKey(POP_UP_MENU_WINDOW_LEVEL_KEY);
            for i in 0..CFArrayGetCount(list) {
                let info = CFArrayGetValueAtIndex(list, i);
                if info.is_null() {
                    continue;
                }
                let layer = CFDictionaryGetValue(info, layer_key.as_concrete_TypeRef() as *const c_void);
                let mut level = 0i32;
                if layer.is_null()
                    || !CFNumberGetValue(layer, CF_NUMBER_SINT32_TYPE, &mut level as *mut i32 as *mut c_void)
                    || level != menu_level
                {
                    continue;
                }
                let dict = CFDictionaryGetValue(info, bounds_key.as_concrete_TypeRef() as *const c_void);
                if dict.is_null() {
                    continue;
                }
                let mut r = CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { width: 0.0, height: 0.0 } };
                if CGRectMakeWithDictionaryRepresentation(dict, &mut r) {
                    out.push(Rect::new(r.origin.x, r.origin.y, r.size.width, r.size.height));
                }
            }
            CFRelease(list as CFTypeRef);
        }
        out
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;

    /// The usable area of a display; only known on macOS.
    pub fn visible_frame(_display: &Display) -> Option<Rect> {
        None
    }

    /// The menu bar strip of a display; only known on macOS.
    pub fn menu_bar_rect(_display: &Display) -> Option<Rect> {
        None
    }

    /// Bounds of open pop-up menus; always empty off macOS.
    pub fn open_menu_rects() -> Vec<Rect> {
        Vec::new()
    }
}

pub use imp::*;

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn menu_bar_and_visible_frame_of_main_display() {
        let d = crate::display_at(0.0, 0.0).expect("main display");
        let bar = menu_bar_rect(&d).expect("menu bar rect");
        assert!(bar.h >= 24.0);
        assert_eq!(bar.y, d.y);
        let v = visible_frame(&d).expect("visible frame");
        assert!(v.x >= d.x && v.y >= d.y && v.x + v.w <= d.x + d.w + 0.5 && v.y + v.h <= d.y + d.h + 0.5);
        let _ = open_menu_rects();
    }
}
