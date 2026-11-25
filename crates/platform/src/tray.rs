//! The menu bar / system tray icon and its menu.
//!
//! - **macOS**: an `NSStatusItem` (`tray/macos.rs`).
//! - **Windows**: `Shell_NotifyIconW` with a hidden window of our own (`tray/windows.rs`).
//! - **Linux**: a StatusNotifierItem over D-Bus via `ksni` (`tray/linux.rs`).
//!
//! Create and use the `Tray` on the main thread (GPUI's `App` callbacks run there). On
//! macOS AppKit requires it; on Windows the icon's window belongs to the creating thread,
//! whose message loop (GPUI's) delivers its clicks.
//!
//! A left click sends [`TrayEvent::Clicked`]; a right click or ⌃-click opens the menu,
//! wrapped in `MenuOpened` / `MenuClosed`. The menu is whatever the last
//! [`Tray::set_menu`] gave, so keep it current as state changes.
//!
//! `on_event` runs on the main thread on macOS and Windows (possibly inside the menu's
//! modal loop) and on the D-Bus thread on Linux, so post from it rather than doing work.
//!
//! Icons are passed as decoded RGBA rather than PNG: the Linux tray needs raw pixels and
//! this crate has no PNG decoder, while the app already has `image`. macOS shows them as
//! 18×18 point template images (pass the @2x PNG, 36×36, for sharp Retina icons); Windows
//! and Linux show the pixels as given, so pass light pixels there if the template icon
//! is black.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub id: String,
    pub title: String,
    /// Shown after the title, e.g. `"⌃⌥H"`.
    pub shortcut_hint: Option<String>,
    pub checked: bool,
    pub enabled: bool,
    pub visible: bool,
}

impl MenuItem {
    /// An enabled, visible, unchecked item.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> MenuItem {
        MenuItem { id: id.into(), title: title.into(), shortcut_hint: None, checked: false, enabled: true, visible: true }
    }

    /// Title, then `separator` and the hint if there is one.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    fn label(&self, separator: &str) -> String {
        match &self.shortcut_hint {
            Some(hint) if !hint.is_empty() => format!("{}{separator}{}", self.title, hint),
            _ => self.title.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item(MenuItem),
    Separator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    /// Left click on the icon (Linux: the tray host's "activate").
    Clicked,
    /// A menu item was chosen; carries its id.
    MenuItem(String),
    /// The menu is about to show (macOS, Windows).
    MenuOpened,
    /// The menu closed (macOS, Windows). Comes before the chosen item's `MenuItem`.
    MenuClosed,
}

/// The icon in its two states, as straight (not premultiplied) RGBA rows.
#[derive(Clone, Debug)]
pub struct TrayIcons {
    pub width: u32,
    pub height: u32,
    pub on_rgba: Vec<u8>,
    pub off_rgba: Vec<u8>,
}

impl TrayIcons {
    pub fn from_rgba(width: u32, height: u32, on_rgba: Vec<u8>, off_rgba: Vec<u8>) -> Result<TrayIcons, String> {
        let len = width as usize * height as usize * 4;
        if width == 0 || height == 0 || on_rgba.len() != len || off_rgba.len() != len {
            return Err(format!("tray icons must be {width}×{height} RGBA ({len} bytes each)"));
        }
        Ok(TrayIcons { width, height, on_rgba, off_rgba })
    }
}

/// A rectangle in global coordinates: points, origin at the top left of the main display.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// macOS `systemOrange`, used for the attention tint.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
const ATTENTION: [u8; 3] = [255, 149, 0];

/// Paints every pixel `rgb`, keeping its alpha: what tinting a template image does.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn tinted(rgba: &[u8], rgb: [u8; 3]) -> Vec<u8> {
    rgba.chunks_exact(4).flat_map(|p| [rgb[0], rgb[1], rgb[2], p[3]]).collect()
}

/// The tray icon; removed when dropped. Not `Send`: keep it on the main thread.
pub struct Tray {
    inner: imp::Tray,
}

impl Tray {
    pub fn new(icons: TrayIcons, on_event: impl Fn(TrayEvent) + Send + 'static) -> Result<Tray, String> {
        Ok(Tray { inner: imp::Tray::new(icons, Box::new(on_event))? })
    }

    /// Shows the "on" or "off" icon.
    pub fn set_active(&mut self, active: bool) {
        self.inner.set_active(active);
    }

    /// Tints the icon orange (click-through mode).
    pub fn set_attention(&mut self, attention: bool) {
        self.inner.set_attention(attention);
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        self.inner.set_tooltip(tooltip);
    }

    pub fn set_menu(&mut self, entries: Vec<MenuEntry>) {
        self.inner.set_menu(entries);
    }

    /// The status item's frame on macOS, for the overlay's pass-through rects. `None`
    /// elsewhere (tray hosts don't say where the icon is).
    pub fn frame(&self) -> Option<Rect> {
        self.inner.frame()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod imp {
    use super::*;

    pub struct Tray;

    impl Tray {
        pub fn new(_icons: TrayIcons, _on_event: Box<dyn Fn(TrayEvent) + Send>) -> Result<Tray, String> {
            Err("the tray isn't available in this build".into())
        }
        pub fn set_active(&mut self, _active: bool) {}
        pub fn set_attention(&mut self, _attention: bool) {}
        pub fn set_tooltip(&mut self, _tooltip: &str) {}
        pub fn set_menu(&mut self, _entries: Vec<MenuEntry>) {}
        pub fn frame(&self) -> Option<Rect> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_tint() {
        let mut item = MenuItem::new("toggle", "Turn Pointer On");
        assert_eq!(item.label("    "), "Turn Pointer On");
        item.shortcut_hint = Some("⌃⌥H".into());
        assert_eq!(item.label("    "), "Turn Pointer On    ⌃⌥H");
        assert_eq!(tinted(&[0, 0, 0, 200, 9, 9, 9, 0], ATTENTION), vec![255, 149, 0, 200, 255, 149, 0, 0]);
        assert!(TrayIcons::from_rgba(2, 2, vec![0; 16], vec![0; 15]).is_err());
    }
}
