//! The parts GPUI doesn't do: making the overlay ignore clicks and float above everything,
//! and reading the mouse while the overlay never receives mouse events.
//!
//! On macOS and Windows this only *watches* the left button. Swallowing clicks so the app
//! underneath doesn't get them (TinyHand's "tap") needs an event tap on macOS and a
//! low-level mouse hook on Windows, and isn't part of this spike. On Linux the overlay
//! takes the mouse itself, which swallows the clicks for free.

pub mod cursor;
pub mod hotkeys;
pub mod input;
pub mod login;
pub mod menus;
pub mod sound;
pub mod tray;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Mouse position in the overlay's coordinates (points, y down, origin at the top left of
/// the display it covers) and whether the left button is down.
pub struct Mouse {
    pub x: f32,
    pub y: f32,
    pub down: bool,
}

/// A display in global points (y down, origin at the top left of the primary display).
/// `id` is the same number GPUI uses for the display (`u64::from(DisplayId)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
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

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        origin: NSPoint,
        size: NSPoint,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGPoint,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
        fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
        fn CGGetDisplaysWithPoint(point: CGPoint, max: u32, displays: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayBounds(display: u32) -> CGRect;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const std::ffi::c_void);
    }

    /// The pointer in global points (y down from the top of the primary display).
    pub fn cursor_global() -> Option<(f64, f64)> {
        // SAFETY: a null source gives a fresh event we own and release.
        unsafe {
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return None;
            }
            let p = CGEventGetLocation(event);
            CFRelease(event);
            Some((p.x, p.y))
        }
    }

    /// The display under a global point.
    pub fn display_at(x: f64, y: f64) -> Option<Display> {
        let mut id = 0u32;
        let mut count = 0u32;
        // SAFETY: room for one id, as `max` says.
        let err = unsafe { CGGetDisplaysWithPoint(CGPoint { x, y }, 1, &mut id, &mut count) };
        if err != 0 || count == 0 {
            return None;
        }
        // SAFETY: plain query.
        let b = unsafe { CGDisplayBounds(id) };
        Some(Display { id: id as u64, x: b.origin.x, y: b.origin.y, w: b.size.x, h: b.size.y })
    }

    pub fn hide_from_dock() {
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            // NSApplicationActivationPolicyAccessory
            let _: bool = msg_send![app, setActivationPolicy: 1i64];
        }
    }

    pub fn make_overlay(window: &impl HasWindowHandle, click_through: bool) -> Result<(), String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return Err("not an AppKit window".into());
        };
        unsafe {
            let view = h.ns_view.as_ptr() as *mut Object;
            let ns_window: *mut Object = msg_send![view, window];
            let _: () = msg_send![ns_window, setIgnoresMouseEvents: if click_through { YES } else { NO }];
            let _: () = msg_send![ns_window, setHasShadow: NO];
            // Same level as the Swift app: above the screen saver level.
            let _: () = msg_send![ns_window, setLevel: 1002i64];
            // canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
            let _: () = msg_send![ns_window, setCollectionBehavior: (1u64 | 16 | 64 | 256)];
            // GPUI makes titled windows (hidden title bar), and AppKit keeps titled windows
            // below the menu bar. The overlay covers the whole screen, menu bar included:
            // let this window class keep any frame, then set the frame again.
            allow_any_frame(ns_window);
            let screen: *mut Object = msg_send![ns_window, screen];
            if !screen.is_null() {
                let frame: NSRect = msg_send![screen, frame];
                let _: () = msg_send![ns_window, setFrame: frame display: YES];
            }
        }
        Ok(())
    }

    /// Overrides `constrainFrameRect:toScreen:` on the window's class (GPUI's panel
    /// class, only used for our own windows) so AppKit stops moving it below the menu bar.
    unsafe fn allow_any_frame(ns_window: *mut Object) {
        extern "C" fn keep(_this: &Object, _cmd: objc::runtime::Sel, rect: NSRect, _screen: *mut Object) -> NSRect {
            rect
        }
        unsafe {
            let class = objc::runtime::object_getClass(ns_window) as *mut objc::runtime::Class;
            let types = c"{CGRect={CGPoint=dd}{CGSize=dd}}@:{CGRect={CGPoint=dd}{CGSize=dd}}@";
            let imp: extern "C" fn(&Object, objc::runtime::Sel, NSRect, *mut Object) -> NSRect = keep;
            // Adds the method once; later calls fail harmlessly because it exists.
            objc::runtime::class_addMethod(class, sel!(constrainFrameRect:toScreen:), std::mem::transmute(imp), types.as_ptr());
        }
    }

    fn ns_window_of(window: &impl HasWindowHandle) -> Result<*mut Object, String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else {
            return Err("not an AppKit window".into());
        };
        // SAFETY: the view belongs to a live GPUI window.
        unsafe {
            let view = h.ns_view.as_ptr() as *mut Object;
            let ns_window: *mut Object = msg_send![view, window];
            if ns_window.is_null() {
                return Err("view has no window".into());
            }
            Ok(ns_window)
        }
    }

    /// The toolbar and hint panel: floats one level below the overlay (as the Swift
    /// toolbar does), on every Space, without AppKit's shadow (the view draws its own).
    pub fn make_panel(window: &impl HasWindowHandle) -> Result<(), String> {
        let ns_window = ns_window_of(window)?;
        // SAFETY: plain setters on a live window, on the main thread.
        unsafe {
            let _: () = msg_send![ns_window, setLevel: 1001i64];
            // canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
            let _: () = msg_send![ns_window, setCollectionBehavior: (1u64 | 16 | 64 | 256)];
            let _: () = msg_send![ns_window, setHasShadow: NO];
            let _: () = msg_send![ns_window, setHidesOnDeactivate: NO];
        }
        Ok(())
    }

    /// Moves and sizes a window to a rect in global points (y down from the top of the
    /// primary display).
    pub fn place_window(window: &impl HasWindowHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
        let ns_window = ns_window_of(window)?;
        // SAFETY: AppKit calls on the main thread; `screens` always has the primary first.
        unsafe {
            let screens: *mut Object = msg_send![class!(NSScreen), screens];
            let count: usize = msg_send![screens, count];
            if count == 0 {
                return Err("no screens".into());
            }
            let primary: *mut Object = msg_send![screens, objectAtIndex: 0usize];
            let pf: NSRect = msg_send![primary, frame];
            let frame = NSRect { origin: NSPoint { x, y: pf.size.y - (y + h) }, size: NSPoint { x: w, y: h } };
            allow_any_frame(ns_window);
            let _: () = msg_send![ns_window, setFrame: frame display: YES];
        }
        Ok(())
    }

    /// Lets clicks fall through a window (or not).
    pub fn set_ignores_mouse(window: &impl HasWindowHandle, ignore: bool) -> Result<(), String> {
        let ns_window = ns_window_of(window)?;
        // SAFETY: plain setter on a live window.
        unsafe {
            let _: () = msg_send![ns_window, setIgnoresMouseEvents: if ignore { YES } else { NO }];
        }
        Ok(())
    }

    /// A card-like window (the welcome card): no traffic lights, the blur behind it
    /// rounded to the card.
    pub fn make_card(window: &impl HasWindowHandle, radius: f64) -> Result<(), String> {
        let ns_window = ns_window_of(window)?;
        // SAFETY: AppKit calls on a live window, on the main thread.
        unsafe {
            for kind in 0u64..3 {
                let button: *mut Object = msg_send![ns_window, standardWindowButton: kind];
                if !button.is_null() {
                    let _: () = msg_send![button, setHidden: YES];
                }
            }
            let _: () = msg_send![ns_window, setMovableByWindowBackground: YES];
        }
        round_blur(window, radius)?;
        // SAFETY: as above.
        unsafe {
            let _: () = msg_send![ns_window, setHasShadow: YES];
            let _: () = msg_send![ns_window, invalidateShadow];
        }
        Ok(())
    }

    /// Rounds the corners of the blur GPUI puts behind a `Blurred` window, so a card-shaped
    /// window doesn't show a square of blur at its corners.
    pub fn round_blur(window: &impl HasWindowHandle, radius: f64) -> Result<(), String> {
        let ns_window = ns_window_of(window)?;
        // SAFETY: walks the content view's subviews and sets layer properties.
        unsafe {
            let content: *mut Object = msg_send![ns_window, contentView];
            if content.is_null() {
                return Ok(());
            }
            let subviews: *mut Object = msg_send![content, subviews];
            let count: usize = msg_send![subviews, count];
            for i in 0..count {
                let view: *mut Object = msg_send![subviews, objectAtIndex: i];
                let is_blur: bool = msg_send![view, isKindOfClass: class!(NSVisualEffectView)];
                if !is_blur {
                    continue;
                }
                let _: () = msg_send![view, setWantsLayer: YES];
                let layer: *mut Object = msg_send![view, layer];
                if !layer.is_null() {
                    let _: () = msg_send![layer, setCornerRadius: radius];
                    let _: () = msg_send![layer, setMasksToBounds: YES];
                }
            }
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

    /// Not written for this OS yet: the overlay stays on the display it opened on.
    pub fn cursor_global() -> Option<(f64, f64)> {
        None
    }

    pub fn display_at(_x: f64, _y: f64) -> Option<Display> {
        None
    }

    pub fn hide_from_dock() {}

    pub fn make_overlay(window: &impl HasWindowHandle, click_through: bool) -> Result<(), String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return Err("not a Win32 window".into());
        };
        let hwnd = h.hwnd.get() as HWND;
        unsafe {
            // WS_EX_TRANSPARENT lets clicks fall through, but only on a layered window.
            // A layered window stays invisible until its attributes are set once.
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let mut extra = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
            if click_through {
                extra |= WS_EX_TRANSPARENT;
            }
            let extra = extra as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | extra);
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
        Ok(())
    }

    fn hwnd_of(window: &impl HasWindowHandle) -> Result<HWND, String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return Err("not a Win32 window".into());
        };
        Ok(h.hwnd.get() as HWND)
    }

    /// The toolbar and hint panel: topmost, never activated, out of the taskbar.
    pub fn make_panel(window: &impl HasWindowHandle) -> Result<(), String> {
        make_overlay(window, false)
    }

    /// GPUI already placed the window from its bounds.
    pub fn place_window(_window: &impl HasWindowHandle, _x: f64, _y: f64, _w: f64, _h: f64) -> Result<(), String> {
        Ok(())
    }

    /// Lets clicks fall through a window (or not). The window must be layered
    /// (`make_panel` or `make_overlay` made it so).
    pub fn set_ignores_mouse(window: &impl HasWindowHandle, ignore: bool) -> Result<(), String> {
        let hwnd = hwnd_of(window)?;
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let bit = WS_EX_TRANSPARENT as isize;
            let new = if ignore { style | bit } else { style & !bit };
            if new != style {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new);
            }
        }
        Ok(())
    }

    pub fn make_card(_window: &impl HasWindowHandle, _radius: f64) -> Result<(), String> {
        Ok(())
    }

    pub fn round_blur(_window: &impl HasWindowHandle, _radius: f64) -> Result<(), String> {
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

#[cfg(target_os = "linux")]
mod imp {
    //! On Linux the overlay takes the mouse itself (see `Input::Capture` in main.rs), so
    //! there is nothing to read here. Wayland needs nothing else either: GPUI makes the
    //! overlay a layer-shell surface. On X11 the window manager is asked to keep the
    //! window above the others and on every workspace.
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, PropMode,
    };
    use x11rb::wrapper::ConnectionExt as _;

    /// Not written for this OS yet: the overlay stays on the display it opened on.
    pub fn cursor_global() -> Option<(f64, f64)> {
        None
    }

    pub fn display_at(_x: f64, _y: f64) -> Option<Display> {
        None
    }

    pub fn hide_from_dock() {}

    pub fn make_overlay(window: &impl HasWindowHandle, click_through: bool) -> Result<(), String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let id = match handle.as_raw() {
            RawWindowHandle::Xcb(h) => h.window.get(),
            RawWindowHandle::Xlib(h) => h.window as u32,
            RawWindowHandle::Wayland(_) => return Ok(()),
            _ => return Err("unknown window system".into()),
        };
        let _ = click_through;
        keep_above(id).map_err(|e| format!("X11: {e}"))
    }

    /// The toolbar and hint panel: kept above (X11). On Wayland GPUI makes it a
    /// layer-shell surface, which needs nothing more.
    pub fn make_panel(window: &impl HasWindowHandle) -> Result<(), String> {
        make_overlay(window, false)
    }

    /// GPUI already placed the window from its bounds (X11) or its layer-shell anchor.
    pub fn place_window(_window: &impl HasWindowHandle, _x: f64, _y: f64, _w: f64, _h: f64) -> Result<(), String> {
        Ok(())
    }

    /// Not possible here; the app forwards clicks on the panel's empty parts instead.
    pub fn set_ignores_mouse(_window: &impl HasWindowHandle, _ignore: bool) -> Result<(), String> {
        Ok(())
    }

    pub fn make_card(_window: &impl HasWindowHandle, _radius: f64) -> Result<(), String> {
        Ok(())
    }

    pub fn round_blur(_window: &impl HasWindowHandle, _radius: f64) -> Result<(), String> {
        Ok(())
    }

    fn keep_above(window: u32) -> Result<(), Box<dyn std::error::Error>> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let atom = |name: &str| -> Result<u32, Box<dyn std::error::Error>> {
            Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        };
        let state = atom("_NET_WM_STATE")?;
        let above = atom("_NET_WM_STATE_ABOVE")?;
        let sticky = atom("_NET_WM_STATE_STICKY")?;
        let skip_taskbar = atom("_NET_WM_STATE_SKIP_TASKBAR")?;
        let skip_pager = atom("_NET_WM_STATE_SKIP_PAGER")?;
        let desktop = atom("_NET_WM_DESKTOP")?;

        // For window managers that read the state when the window maps...
        conn.change_property32(PropMode::REPLACE, window, state, AtomEnum::ATOM, &[above, sticky, skip_taskbar, skip_pager])?;
        conn.change_property32(PropMode::REPLACE, window, desktop, AtomEnum::CARDINAL, &[0xFFFF_FFFF])?;
        // ...and the EWMH request for windows that are already mapped.
        let add = |a: u32, b: u32| ClientMessageEvent::new(32, window, state, [1, a, b, 1, 0]);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        conn.send_event(false, root, mask, add(above, sticky))?;
        conn.send_event(false, root, mask, add(skip_taskbar, skip_pager))?;
        conn.send_event(false, root, mask, ClientMessageEvent::new(32, window, desktop, [0xFFFF_FFFF, 1, 0, 0, 0]))?;
        conn.flush()?;
        Ok(())
    }

    pub fn mouse(_display_height: f32, _scale: f32) -> Mouse {
        Mouse { x: 0.0, y: 0.0, down: false }
    }

    /// Whether the Wayland compositor offers `zwlr_layer_shell_v1`; `None` if it can't be
    /// reached. GNOME doesn't offer it.
    pub fn wayland_has_layer_shell() -> Option<bool> {
        use wayland_client::protocol::wl_registry;
        use wayland_client::{Connection, Dispatch, QueueHandle};

        struct Globals(bool);
        impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
            fn event(state: &mut Self, _: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                if let wl_registry::Event::Global { interface, .. } = event {
                    if interface == "zwlr_layer_shell_v1" {
                        state.0 = true;
                    }
                }
            }
        }

        let conn = Connection::connect_to_env().ok()?;
        let mut queue = conn.new_event_queue();
        conn.display().get_registry(&queue.handle(), ());
        let mut globals = Globals(false);
        queue.roundtrip(&mut globals).ok()?;
        Some(globals.0)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod imp {
    use super::*;

    /// Not written for this OS yet: the overlay stays on the display it opened on.
    pub fn cursor_global() -> Option<(f64, f64)> {
        None
    }

    pub fn display_at(_x: f64, _y: f64) -> Option<Display> {
        None
    }

    pub fn hide_from_dock() {}

    pub fn make_overlay(_window: &impl HasWindowHandle, _click_through: bool) -> Result<(), String> {
        Err("overlay isn't written for this platform".into())
    }

    pub fn make_panel(_window: &impl HasWindowHandle) -> Result<(), String> {
        Err("panels aren't written for this platform".into())
    }

    pub fn place_window(_window: &impl HasWindowHandle, _x: f64, _y: f64, _w: f64, _h: f64) -> Result<(), String> {
        Ok(())
    }

    pub fn set_ignores_mouse(_window: &impl HasWindowHandle, _ignore: bool) -> Result<(), String> {
        Ok(())
    }

    pub fn make_card(_window: &impl HasWindowHandle, _radius: f64) -> Result<(), String> {
        Ok(())
    }

    pub fn round_blur(_window: &impl HasWindowHandle, _radius: f64) -> Result<(), String> {
        Ok(())
    }

    pub fn mouse(_display_height: f32, _scale: f32) -> Mouse {
        Mouse { x: 0.0, y: 0.0, down: false }
    }
}

pub use imp::*;
