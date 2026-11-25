//! macOS cursor hiding and the Dock's frame, as `CursorHider.swift`.

use core_foundation::array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation::base::{CFGetTypeID, CFRelease, CFTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};
use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};
use std::ffi::{CStr, c_char, c_void};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayHideCursor(display: u32) -> i32;
    fn CGDisplayShowCursor(display: u32) -> i32;
}

type AXUIElementRef = CFTypeRef;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXValueGetTypeID() -> CFTypeID;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> u8;
}

// AppKit holds NSRunningApplication; GPUI links it anyway, this makes tests find it too.
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

const RTLD_NOW: i32 = 2;

type MainConn = unsafe extern "C" fn() -> i32;
type SetProp = unsafe extern "C" fn(i32, i32, CFStringRef, CFTypeRef) -> i32;
type Seed = unsafe extern "C" fn() -> i32;

/// Looks a symbol up in everything already loaded (CoreGraphics/SkyLight hold the CGS ones).
fn symbol(name: &CStr) -> Option<usize> {
    // SAFETY: dlopen(NULL) returns the main program's handle; dlsym only reads.
    unsafe {
        let handle = dlopen(std::ptr::null(), RTLD_NOW);
        if handle.is_null() {
            return None;
        }
        let sym = dlsym(handle, name.as_ptr());
        (!sym.is_null()).then_some(sym as usize)
    }
}

fn background_hiding_symbols() -> Option<(MainConn, SetProp)> {
    static SYMS: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    let (a, b) = (*SYMS.get_or_init(|| Some((symbol(c"CGSMainConnectionID")?, symbol(c"CGSSetConnectionProperty")?))))?;
    // SAFETY: these private functions have had these signatures for many macOS releases;
    // the Swift app makes the same calls.
    unsafe { Some((std::mem::transmute::<usize, MainConn>(a), std::mem::transmute::<usize, SetProp>(b))) }
}

fn cursor_seed() -> Option<Seed> {
    static SYM: OnceLock<Option<usize>> = OnceLock::new();
    let s = (*SYM.get_or_init(|| symbol(c"CGSCurrentCursorSeed")))?;
    // SAFETY: as above.
    Some(unsafe { std::mem::transmute::<usize, Seed>(s) })
}

pub fn enable_background_hiding() {
    let Some((main_conn, set_prop)) = background_hiding_symbols() else {
        return;
    };
    let key = CFString::from_static_string("SetsCursorInBackground");
    // SAFETY: the connection id comes from CGS itself; key and value outlive the call.
    unsafe {
        let cid = main_conn();
        set_prop(cid, cid, key.as_concrete_TypeRef(), CFBoolean::true_value().as_CFTypeRef());
    }
}

#[derive(Default)]
pub struct Os {
    hide_count: u32,
}

impl Os {
    pub fn hide(&mut self) {
        self.hide_count = 1;
        // SAFETY: plain CG calls.
        unsafe { CGDisplayHideCursor(CGMainDisplayID()) };
    }

    pub fn show(&mut self) {
        for _ in 0..self.hide_count {
            // SAFETY: plain CG calls.
            unsafe { CGDisplayShowCursor(CGMainDisplayID()) };
        }
        self.hide_count = 0;
    }

    pub fn seed(&self) -> i32 {
        // SAFETY: takes no arguments.
        cursor_seed().map_or(0, |f| unsafe { f() })
    }

    pub fn reassert(&mut self) {
        enable_background_hiding();
        // SAFETY: plain CG calls.
        unsafe { CGDisplayHideCursor(CGMainDisplayID()) };
        self.hide_count += 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct DockState {
    frame: Option<Frame>,
    last_read: Option<Instant>,
    reading: bool,
}

/// The Dock's frame, read through Accessibility (needs the app to be trusted).
pub struct DockArea {
    state: Arc<Mutex<DockState>>,
}

impl Default for DockArea {
    fn default() -> Self {
        Self::new()
    }
}

impl DockArea {
    pub fn new() -> Self {
        DockArea { state: Arc::new(Mutex::new(DockState { frame: None, last_read: None, reading: false })) }
    }

    /// Whether (`x`, `y`), global top-left points, is within 8 pt of the Dock. Starts a
    /// background read when the frame is over a second old.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (known, refresh) = {
            let mut s = self.state.lock().unwrap();
            let due = !s.reading && s.last_read.is_none_or(|t| t.elapsed() > Duration::from_secs(1));
            if due {
                s.reading = true;
                s.last_read = Some(Instant::now());
            }
            (s.frame, due)
        };
        if refresh {
            let state = self.state.clone();
            let spawned = std::thread::Builder::new().name("TinyHand dock area".into()).spawn(move || {
                let fresh = read_dock();
                let mut s = state.lock().unwrap();
                s.frame = fresh;
                s.reading = false;
            });
            if spawned.is_err() {
                self.state.lock().unwrap().reading = false;
            }
        }
        known.is_some_and(|f| {
            let m = 8.0;
            x >= f.x - m && x < f.x + f.w + m && y >= f.y - m && y < f.y + f.h + m
        })
    }
}

fn dock_pid() -> Option<i32> {
    let class = Class::get("NSRunningApplication")?;
    let id = CFString::from_static_string("com.apple.dock");
    // SAFETY: CFString is toll-free bridged to NSString; the array and its items are
    // autoreleased into the caller's pool.
    unsafe {
        let apps: *mut Object = msg_send![class, runningApplicationsWithBundleIdentifier: id.as_concrete_TypeRef() as *mut Object];
        if apps.is_null() {
            return None;
        }
        let first: *mut Object = msg_send![apps, firstObject];
        if first.is_null() {
            return None;
        }
        let pid: i32 = msg_send![first, processIdentifier];
        Some(pid)
    }
}

/// Copies an attribute; the caller releases the result.
unsafe fn copy_attr(element: AXUIElementRef, name: &'static str) -> Option<CFTypeRef> {
    let attr = CFString::from_static_string(name);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: `element` is a live AX element; `value` receives a +1 reference on success.
    let err = unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value) };
    (err == 0 && !value.is_null()).then_some(value)
}

/// Reads an AXValue of `kind` (1 = CGPoint, 2 = CGSize) into two doubles, then releases it.
unsafe fn take_pair(value: Option<CFTypeRef>, kind: u32) -> Option<(f64, f64)> {
    let value = value?;
    let mut out = [0f64; 2];
    // SAFETY: CGPoint and CGSize are both two doubles; `value` is ours to release.
    let ok = unsafe {
        let ok = CFGetTypeID(value) == AXValueGetTypeID() && AXValueGetValue(value, kind, out.as_mut_ptr() as *mut c_void) != 0;
        CFRelease(value);
        ok
    };
    ok.then_some((out[0], out[1]))
}

fn read_dock() -> Option<Frame> {
    // SAFETY: a pool for the autoreleased AppKit objects on this short-lived thread.
    let pool = unsafe { objc_autoreleasePoolPush() };
    let frame = read_dock_inner();
    // SAFETY: pops the pool pushed above.
    unsafe { objc_autoreleasePoolPop(pool) };
    frame
}

fn read_dock_inner() -> Option<Frame> {
    let pid = dock_pid()?;
    // SAFETY: every Copy/Create result is released below; the list item is borrowed from
    // the children array, which is released last.
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return None;
        }
        AXUIElementSetMessagingTimeout(app, 0.25);
        let frame = (|| {
            let children = copy_attr(app, "AXChildren")?;
            let list = (|| {
                if CFGetTypeID(children) != CFArrayGetTypeID()
                    || CFArrayGetCount(children as CFArrayRef) < 1
                {
                    return None;
                }
                let list = CFArrayGetValueAtIndex(children as CFArrayRef, 0) as AXUIElementRef;
                let origin = take_pair(copy_attr(list, "AXPosition"), 1).unwrap_or((0.0, 0.0));
                let size = take_pair(copy_attr(list, "AXSize"), 2)?;
                (size != (0.0, 0.0)).then_some(Frame { x: origin.0, y: origin.1, w: size.0, h: size.1 })
            })();
            CFRelease(children);
            list
        })();
        CFRelease(app);
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_read_does_not_crash() {
        // Without Accessibility this is None; with it, a frame with a positive size.
        if let Some(f) = read_dock() {
            assert!(f.w > 0.0 && f.h > 0.0);
        }
        let _ = cursor_seed().map(|f| unsafe { f() });
    }
}
