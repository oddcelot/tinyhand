//! `NSStatusItem` with a button target and menu delegate declared at runtime. Main
//! thread only: every call here talks to AppKit.

use super::{ATTENTION, MenuEntry, Rect, TrayEvent, TrayIcons, tinted};
use objc::declare::ClassDecl;
use objc::runtime::{Class, NO, Object, Sel, YES};
use objc::{class, msg_send, sel, sel_impl};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::Once;

#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

const VARIABLE_LENGTH: f64 = -1.0;
const EVENT_RIGHT_MOUSE_UP: usize = 4;
const MASK_LEFT_MOUSE_UP: u64 = 1 << 2;
const MASK_RIGHT_MOUSE_UP: u64 = 1 << 4;
const MODIFIER_CONTROL: usize = 1 << 18;
const STATE_ON: isize = 1;
const STATE_OFF: isize = 0;

const CLASS_NAME: &str = "TinyHandTrayTarget";
const STATE_IVAR: &str = "tinyhandState";

/// What the Objective-C target reaches through its ivar.
struct Shared {
    on_event: Box<dyn Fn(TrayEvent) + Send>,
    /// Item ids by menu item tag.
    ids: RefCell<Vec<String>>,
    item: *mut Object,
    menu: Cell<*mut Object>,
}

pub struct Tray {
    shared: Box<Shared>,
    target: *mut Object,
    /// on, off, tinted on, tinted off.
    images: [*mut Object; 4],
    active: bool,
    attention: bool,
}

impl Tray {
    pub fn new(icons: TrayIcons, on_event: Box<dyn Fn(TrayEvent) + Send>) -> Result<Tray, String> {
        unsafe {
            let class = target_class();
            let images = [
                image(&icons.on_rgba, icons.width, icons.height, true),
                image(&icons.off_rgba, icons.width, icons.height, true),
                image(&tinted(&icons.on_rgba, ATTENTION), icons.width, icons.height, false),
                image(&tinted(&icons.off_rgba, ATTENTION), icons.width, icons.height, false),
            ];
            if images.iter().any(|i| i.is_null()) {
                return Err("couldn't make the status item images".into());
            }

            let bar: *mut Object = msg_send![class!(NSStatusBar), systemStatusBar];
            let item: *mut Object = msg_send![bar, statusItemWithLength: VARIABLE_LENGTH];
            if item.is_null() {
                return Err("couldn't make the status item".into());
            }
            let _: *mut Object = msg_send![item, retain];

            let shared = Box::new(Shared {
                on_event,
                ids: RefCell::new(Vec::new()),
                item,
                menu: Cell::new(null_mut()),
            });
            let target: *mut Object = msg_send![class, new];
            (*target).set_ivar::<*mut c_void>(STATE_IVAR, &*shared as *const Shared as *mut c_void);

            let button: *mut Object = msg_send![item, button];
            let _: () = msg_send![button, setTarget: target];
            let _: () = msg_send![button, setAction: sel!(statusClicked:)];
            let _: i64 = msg_send![button, sendActionOn: MASK_LEFT_MOUSE_UP | MASK_RIGHT_MOUSE_UP];

            let mut tray = Tray { shared, target, images, active: false, attention: false };
            tray.update_image();
            tray.set_menu(Vec::new());
            Ok(tray)
        }
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        self.update_image();
    }

    pub fn set_attention(&mut self, attention: bool) {
        self.attention = attention;
        self.update_image();
    }

    fn update_image(&self) {
        let index = usize::from(!self.active) + if self.attention { 2 } else { 0 };
        unsafe {
            let button: *mut Object = msg_send![self.shared.item, button];
            let _: () = msg_send![button, setImage: self.images[index]];
        }
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        unsafe {
            let button: *mut Object = msg_send![self.shared.item, button];
            let s = nsstring(tooltip);
            let _: () = msg_send![button, setToolTip: s];
            let _: () = msg_send![s, release];
        }
    }

    pub fn set_menu(&mut self, entries: Vec<MenuEntry>) {
        let mut ids = Vec::new();
        unsafe {
            let menu: *mut Object = msg_send![class!(NSMenu), new];
            let _: () = msg_send![menu, setAutoenablesItems: NO];
            let _: () = msg_send![menu, setDelegate: self.target];
            for entry in &entries {
                match entry {
                    MenuEntry::Separator => {
                        let s: *mut Object = msg_send![class!(NSMenuItem), separatorItem];
                        let _: () = msg_send![menu, addItem: s];
                    }
                    MenuEntry::Item(m) => {
                        // Four spaces before the hint, like the Swift menu.
                        let title = nsstring(&m.label("    "));
                        let empty = nsstring("");
                        let item: *mut Object = msg_send![class!(NSMenuItem), alloc];
                        let item: *mut Object =
                            msg_send![item, initWithTitle: title action: sel!(menuItemClicked:) keyEquivalent: empty];
                        let _: () = msg_send![title, release];
                        let _: () = msg_send![empty, release];
                        let _: () = msg_send![item, setTarget: self.target];
                        let _: () = msg_send![item, setTag: ids.len() as isize];
                        let _: () = msg_send![item, setState: if m.checked { STATE_ON } else { STATE_OFF }];
                        let _: () = msg_send![item, setEnabled: if m.enabled { YES } else { NO }];
                        let _: () = msg_send![item, setHidden: if m.visible { NO } else { YES }];
                        ids.push(m.id.clone());
                        let _: () = msg_send![menu, addItem: item];
                        let _: () = msg_send![item, release];
                    }
                }
            }
            *self.shared.ids.borrow_mut() = ids;
            let old = self.shared.menu.replace(menu);
            if !old.is_null() {
                let _: () = msg_send![old, release];
            }
        }
    }

    pub fn frame(&self) -> Option<Rect> {
        unsafe {
            let button: *mut Object = msg_send![self.shared.item, button];
            if button.is_null() {
                return None;
            }
            let window: *mut Object = msg_send![button, window];
            if window.is_null() {
                return None;
            }
            let f: NSRect = msg_send![window, frame];
            let screens: *mut Object = msg_send![class!(NSScreen), screens];
            let main: *mut Object = msg_send![screens, firstObject];
            if main.is_null() {
                return None;
            }
            let s: NSRect = msg_send![main, frame];
            Some(Rect {
                x: f.origin.x,
                y: s.size.height - (f.origin.y + f.size.height),
                w: f.size.width,
                h: f.size.height,
            })
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let item = self.shared.item;
            let button: *mut Object = msg_send![item, button];
            let _: () = msg_send![button, setTarget: null_mut::<Object>()];
            let bar: *mut Object = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem: item];
            let _: () = msg_send![item, release];
            let menu = self.shared.menu.replace(null_mut());
            if !menu.is_null() {
                let _: () = msg_send![menu, setDelegate: null_mut::<Object>()];
                let _: () = msg_send![menu, release];
            }
            (*self.target).set_ivar::<*mut c_void>(STATE_IVAR, null_mut());
            let _: () = msg_send![self.target, release];
            for image in self.images {
                let _: () = msg_send![image, release];
            }
        }
    }
}

/// The `Shared` behind a target, or `None` once its `Tray` is gone.
unsafe fn shared<'a>(this: &Object) -> Option<&'a Shared> {
    unsafe {
        let p = *this.get_ivar::<*mut c_void>(STATE_IVAR) as *const Shared;
        p.as_ref()
    }
}

extern "C" fn status_clicked(this: &Object, _: Sel, _sender: *mut Object) {
    unsafe {
        let Some(s) = shared(this) else { return };
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let event: *mut Object = msg_send![app, currentEvent];
        let wants_menu = !event.is_null() && {
            let kind: usize = msg_send![event, type];
            let flags: usize = msg_send![event, modifierFlags];
            kind == EVENT_RIGHT_MOUSE_UP || flags & MODIFIER_CONTROL != 0
        };
        let menu = s.menu.get();
        if !wants_menu || menu.is_null() {
            (s.on_event)(TrayEvent::Clicked);
            return;
        }
        // Same trick as the Swift app: attach the menu just for this click, so a left
        // click stays a plain toggle. performClick: returns once the menu closes.
        let _: *mut Object = msg_send![menu, retain];
        let _: () = msg_send![s.item, setMenu: menu];
        let button: *mut Object = msg_send![s.item, button];
        let _: () = msg_send![button, performClick: null_mut::<Object>()];
        let _: () = msg_send![s.item, setMenu: null_mut::<Object>()];
        let _: () = msg_send![menu, release];
    }
}

extern "C" fn menu_item_clicked(this: &Object, _: Sel, sender: *mut Object) {
    unsafe {
        let Some(s) = shared(this) else { return };
        let tag: isize = msg_send![sender, tag];
        let id = usize::try_from(tag).ok().and_then(|t| s.ids.borrow().get(t).cloned());
        if let Some(id) = id {
            (s.on_event)(TrayEvent::MenuItem(id));
        }
    }
}

extern "C" fn menu_will_open(this: &Object, _: Sel, _menu: *mut Object) {
    if let Some(s) = unsafe { shared(this) } {
        (s.on_event)(TrayEvent::MenuOpened);
    }
}

extern "C" fn menu_did_close(this: &Object, _: Sel, _menu: *mut Object) {
    if let Some(s) = unsafe { shared(this) } {
        (s.on_event)(TrayEvent::MenuClosed);
    }
}

fn target_class() -> &'static Class {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let mut decl = ClassDecl::new(CLASS_NAME, class!(NSObject)).expect("tray target class registered twice");
        decl.add_ivar::<*mut c_void>(STATE_IVAR);
        unsafe {
            decl.add_method(sel!(statusClicked:), status_clicked as extern "C" fn(&Object, Sel, *mut Object));
            decl.add_method(sel!(menuItemClicked:), menu_item_clicked as extern "C" fn(&Object, Sel, *mut Object));
            decl.add_method(sel!(menuWillOpen:), menu_will_open as extern "C" fn(&Object, Sel, *mut Object));
            decl.add_method(sel!(menuDidClose:), menu_did_close as extern "C" fn(&Object, Sel, *mut Object));
        }
        if let Some(protocol) = objc::runtime::Protocol::get("NSMenuDelegate") {
            decl.add_protocol(protocol);
        }
        decl.register();
    });
    Class::get(CLASS_NAME).expect("tray target class")
}

/// A +1 `NSString`.
unsafe fn nsstring(s: &str) -> *mut Object {
    unsafe {
        let obj: *mut Object = msg_send![class!(NSString), alloc];
        msg_send![obj, initWithBytes: s.as_ptr() as *const c_void length: s.len() encoding: 4usize]
    }
}

/// A +1 18×18 point `NSImage` from straight RGBA pixels.
unsafe fn image(rgba: &[u8], width: u32, height: u32, template: bool) -> *mut Object {
    unsafe {
        let color_space = nsstring("NSDeviceRGBColorSpace");
        let rep: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: *mut Object = msg_send![rep,
            initWithBitmapDataPlanes: null_mut::<*mut u8>()
            pixelsWide: width as isize
            pixelsHigh: height as isize
            bitsPerSample: 8isize
            samplesPerPixel: 4isize
            hasAlpha: YES
            isPlanar: NO
            colorSpaceName: color_space
            // NSBitmapFormatAlphaNonpremultiplied
            bitmapFormat: 2usize
            bytesPerRow: (width * 4) as isize
            bitsPerPixel: 32isize];
        let _: () = msg_send![color_space, release];
        if rep.is_null() {
            return null_mut();
        }
        let data: *mut u8 = msg_send![rep, bitmapData];
        std::ptr::copy_nonoverlapping(rgba.as_ptr(), data, rgba.len());
        let size = NSSize { width: 18.0, height: 18.0 };
        let _: () = msg_send![rep, setSize: size];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithSize: size];
        let _: () = msg_send![image, addRepresentation: rep];
        let _: () = msg_send![rep, release];
        let _: () = msg_send![image, setTemplate: if template { YES } else { NO }];
        image
    }
}
