//! Updates: Sparkle on macOS (Updater.swift), nothing yet on Windows and Linux.
//!
//! The binary doesn't link Sparkle. [`Updater::start`] loads `Sparkle.framework` from the
//! bundle's `Contents/Frameworks` at runtime and makes an `SPUStandardUpdaterController`,
//! but only when the app runs from a `.app` whose Info.plist has `SUFeedURL` (release
//! builds; `scripts/release.sh --rust` writes it). Everywhere else `start` gives `None`
//! and the app hides "Check for Updates…" and the settings rows, as the Swift app does.
//!
//! Create and use it on the main thread.

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;
    use std::sync::Once;

    use objc::declare::ClassDecl;
    use objc::runtime::{BOOL, Class, NO, Object, Sel, YES};
    use objc::{class, msg_send, sel, sel_impl};

    const DELEGATE_CLASS: &str = "TinyHandUpdaterDriverDelegate";

    pub struct Updater {
        /// A +1 `SPUStandardUpdaterController`, kept for the life of the app.
        controller: *mut Object,
    }

    /// A +1 `NSString`.
    unsafe fn nsstring(s: &str) -> *mut Object {
        unsafe {
            let obj: *mut Object = msg_send![class!(NSString), alloc];
            msg_send![obj, initWithBytes: s.as_ptr() as *const c_void length: s.len() encoding: 4usize]
        }
    }

    /// The main bundle's path, if the app runs from a `.app` whose Info.plist has `SUFeedURL`.
    fn bundle_with_feed() -> Option<String> {
        unsafe {
            let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
            if bundle.is_null() {
                return None;
            }
            let path: *mut Object = msg_send![bundle, bundlePath];
            if path.is_null() {
                return None;
            }
            let utf8: *const std::ffi::c_char = msg_send![path, UTF8String];
            let path = std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned();
            let key = nsstring("SUFeedURL");
            let feed: *mut Object = msg_send![bundle, objectForInfoDictionaryKey: key];
            let _: () = msg_send![key, release];
            (path.ends_with(".app") && !feed.is_null()).then_some(path)
        }
    }

    pub fn is_available() -> bool {
        bundle_with_feed().is_some()
    }

    /// `supportsGentleScheduledUpdateReminders`: yes, as in Updater.swift, so Sparkle
    /// doesn't warn about a background app.
    extern "C" fn gentle_reminders(_: &Object, _: Sel) -> BOOL {
        YES
    }

    /// `standardUserDriverWillHandleShowingUpdate:forUpdate:state:`: bring the app forward
    /// so the update window isn't hidden behind other apps (TinyHand has no Dock icon).
    extern "C" fn will_show_update(_: &Object, _: Sel, _: BOOL, _: *mut Object, _: *mut Object) {
        activate_app();
    }

    fn activate_app() {
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps: YES];
        }
    }

    fn delegate_class() -> &'static Class {
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| {
            let mut decl = ClassDecl::new(DELEGATE_CLASS, class!(NSObject)).expect("updater delegate class registered twice");
            unsafe {
                decl.add_method(sel!(supportsGentleScheduledUpdateReminders), gentle_reminders as extern "C" fn(&Object, Sel) -> BOOL);
                decl.add_method(
                    sel!(standardUserDriverWillHandleShowingUpdate:forUpdate:state:),
                    will_show_update as extern "C" fn(&Object, Sel, BOOL, *mut Object, *mut Object),
                );
            }
            if let Some(protocol) = objc::runtime::Protocol::get("SPUStandardUserDriverDelegate") {
                decl.add_protocol(protocol);
            }
            decl.register();
        });
        Class::get(DELEGATE_CLASS).expect("updater delegate class")
    }

    impl Updater {
        pub fn start() -> Result<Updater, String> {
            let bundle_path = bundle_with_feed().ok_or("no SUFeedURL in a .app")?;
            let framework = format!("{bundle_path}/Contents/Frameworks/Sparkle.framework");
            unsafe {
                let path = nsstring(&framework);
                let fw: *mut Object = msg_send![class!(NSBundle), bundleWithPath: path];
                let _: () = msg_send![path, release];
                if fw.is_null() {
                    return Err(format!("{framework} not found"));
                }
                let mut error: *mut Object = std::ptr::null_mut();
                let loaded: BOOL = msg_send![fw, loadAndReturnError: &mut error];
                if loaded == NO {
                    let detail = if error.is_null() {
                        String::from("unknown error")
                    } else {
                        let d: *mut Object = msg_send![error, description];
                        let utf8: *const std::ffi::c_char = msg_send![d, UTF8String];
                        std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
                    };
                    return Err(format!("can't load Sparkle: {detail}"));
                }
                let class = Class::get("SPUStandardUpdaterController").ok_or("Sparkle has no SPUStandardUpdaterController")?;
                // Lives as long as the app; never released.
                let delegate: *mut Object = msg_send![delegate_class(), new];
                let controller: *mut Object = msg_send![class, alloc];
                let controller: *mut Object = msg_send![controller,
                    initWithStartingUpdater: YES
                    updaterDelegate: std::ptr::null_mut::<Object>()
                    userDriverDelegate: delegate];
                if controller.is_null() {
                    return Err("SPUStandardUpdaterController init failed".into());
                }
                Ok(Updater { controller })
            }
        }

        fn updater(&self) -> *mut Object {
            unsafe { msg_send![self.controller, updater] }
        }

        pub fn check_for_updates(&self) {
            activate_app();
            unsafe {
                let _: () = msg_send![self.controller, checkForUpdates: std::ptr::null_mut::<Object>()];
            }
        }

        pub fn automatically_checks(&self) -> bool {
            let updater = self.updater();
            if updater.is_null() {
                return false;
            }
            let on: BOOL = unsafe { msg_send![updater, automaticallyChecksForUpdates] };
            on != NO
        }

        pub fn set_automatically_checks(&self, on: bool) {
            let updater = self.updater();
            if !updater.is_null() {
                unsafe {
                    let _: () = msg_send![updater, setAutomaticallyChecksForUpdates: if on { YES } else { NO }];
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    /// No updater on this OS yet; [`Updater::start`] always fails.
    pub struct Updater(());

    pub fn is_available() -> bool {
        false
    }

    impl Updater {
        pub fn start() -> Result<Updater, String> {
            Err("no updater on this OS".into())
        }

        pub fn check_for_updates(&self) {}

        pub fn automatically_checks(&self) -> bool {
            false
        }

        pub fn set_automatically_checks(&self, _on: bool) {}
    }
}

/// Sparkle's `SPUStandardUpdaterController` on macOS; a stub elsewhere.
pub use imp::Updater;

/// Whether updates can work: macOS, a `.app` with `SUFeedURL` in its Info.plist.
pub fn is_available() -> bool {
    imp::is_available()
}
