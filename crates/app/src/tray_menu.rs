//! The tray menu and tooltips, as AppDelegate.swift builds them.

use tinyhand_core::settings::Settings;
use tinyhand_platform::tray::{MenuEntry, MenuItem};

pub const ALLOW: &str = "allow";
pub const TOGGLE: &str = "toggle";
pub const CLEAR: &str = "clear";
pub const SOUND: &str = "sound";
pub const SETTINGS: &str = "settings";
pub const QUIT: &str = "quit";

pub fn menu(settings: &Settings, active: bool, sound_on: bool, trusted: bool) -> Vec<MenuEntry> {
    let mac = cfg!(target_os = "macos");
    let mut allow = MenuItem::new(ALLOW, "Allow Accessibility…");
    allow.visible = !trusted;
    let mut toggle = MenuItem::new(TOGGLE, if active { "Turn Pointer Off" } else { "Turn Pointer On" });
    toggle.shortcut_hint = Some(settings.toggle_shortcut.display(mac));
    let mut clear = MenuItem::new(CLEAR, "Clear Drawing");
    clear.shortcut_hint = Some(settings.clear_shortcut.display(mac));
    let mut sound = MenuItem::new(SOUND, "Tap Sound");
    sound.checked = sound_on;
    let mut prefs = MenuItem::new(SETTINGS, "Settings…");
    let mut quit = MenuItem::new(QUIT, "Quit TinyHand");
    if mac {
        prefs.shortcut_hint = Some("⌘,".into());
        quit.shortcut_hint = Some("⌘Q".into());
    }
    vec![
        MenuEntry::Item(allow),
        MenuEntry::Item(toggle),
        MenuEntry::Item(clear),
        MenuEntry::Item(sound),
        MenuEntry::Separator,
        MenuEntry::Item(prefs),
        MenuEntry::Separator,
        MenuEntry::Item(quit),
    ]
}

pub fn tooltip(active: bool, click_through: bool) -> &'static str {
    if active && click_through {
        "TinyHand clicks go through — double-tap ⇧ to tap again, click to turn off"
    } else if active {
        "TinyHand is on — click to turn off, right-click for the menu"
    } else {
        "TinyHand — click to turn on, right-click for the menu"
    }
}
