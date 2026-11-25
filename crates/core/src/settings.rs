//! Preferences from Preferences.swift, kept in `settings.json` with the same key names.

use crate::particles::TapEffect;
use crate::styles::{self, DEFAULT_PINNED, DEFAULT_STYLE, MAX_PINNED};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The pointer size slider's range (a fraction of the screen height).
pub const POINTER_SIZE_RANGE: std::ops::RangeInclusive<f32> = 0.08..=0.20;

// ---------------------------------------------------------------- shortcuts

/// A global shortcut, written as text like `ctrl+alt+h`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Command on macOS, Super/Windows key elsewhere.
    pub cmd: bool,
    /// `space`, `enter`, `tab`, `backspace`, `delete`, `escape`, `left`, `right`, `up`,
    /// `down`, `home`, `end`, `pageup`, `pagedown`, `f1`..`f12`, or one lowercase character.
    pub key: String,
}

const NAMED_KEYS: [&str; 14] = [
    "space", "enter", "tab", "backspace", "delete", "escape", "left", "right", "up", "down",
    "home", "end", "pageup", "pagedown",
];

fn valid_key(key: &str) -> bool {
    if NAMED_KEYS.contains(&key) {
        return true;
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=12).contains(&n) && !key.starts_with("f0");
    }
    let mut chars = key.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_whitespace())
}

impl Shortcut {
    pub fn new(ctrl: bool, alt: bool, shift: bool, cmd: bool, key: &str) -> Shortcut {
        Shortcut {
            ctrl,
            alt,
            shift,
            cmd,
            key: key.to_lowercase(),
        }
    }

    pub fn toggle_default() -> Shortcut {
        Shortcut::new(true, true, false, false, "h")
    }

    pub fn clear_default() -> Shortcut {
        Shortcut::new(true, true, false, false, "x")
    }

    /// At least one of ctrl, alt or cmd, as the Swift recorder demands.
    pub fn has_required_modifier(&self) -> bool {
        self.ctrl || self.alt || self.cmd
    }

    /// Text for the settings page: `⌃⌥H` on macOS, `Ctrl+Alt+H` elsewhere.
    pub fn display(&self, mac: bool) -> String {
        if mac {
            let mut s = String::new();
            for (on, sym) in [(self.ctrl, "⌃"), (self.alt, "⌥"), (self.shift, "⇧"), (self.cmd, "⌘")] {
                if on {
                    s.push_str(sym);
                }
            }
            s.push_str(&match self.key.as_str() {
                "space" => "Space".to_string(),
                "enter" => "↩".into(),
                "tab" => "⇥".into(),
                "backspace" => "⌫".into(),
                "delete" => "⌦".into(),
                "escape" => "⎋".into(),
                "left" => "←".into(),
                "right" => "→".into(),
                "up" => "↑".into(),
                "down" => "↓".into(),
                "home" => "↖".into(),
                "end" => "↘".into(),
                "pageup" => "⇞".into(),
                "pagedown" => "⇟".into(),
                k => k.to_uppercase(),
            });
            s
        } else {
            let mut parts: Vec<String> = Vec::new();
            for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.cmd, "Super")] {
                if on {
                    parts.push(name.into());
                }
            }
            parts.push(match self.key.as_str() {
                "space" => "Space".to_string(),
                "enter" => "Enter".into(),
                "tab" => "Tab".into(),
                "backspace" => "Backspace".into(),
                "delete" => "Delete".into(),
                "escape" => "Esc".into(),
                "left" => "Left".into(),
                "right" => "Right".into(),
                "up" => "Up".into(),
                "down" => "Down".into(),
                "home" => "Home".into(),
                "end" => "End".into(),
                "pageup" => "PageUp".into(),
                "pagedown" => "PageDown".into(),
                k => k.to_uppercase(),
            });
            parts.join("+")
        }
    }
}

impl fmt::Display for Shortcut {
    /// The stored text form, such as `ctrl+alt+h`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [(self.ctrl, "ctrl"), (self.alt, "alt"), (self.shift, "shift"), (self.cmd, "cmd")] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

impl FromStr for Shortcut {
    type Err = String;

    /// Reads the text form. Also accepts option, control, command, super, meta and win as
    /// modifier names, and any case or spacing.
    fn from_str(text: &str) -> Result<Shortcut, String> {
        let text = text.trim().to_lowercase();
        // A "+" key is written "ctrl++".
        let (mods, key) = if text == "+" {
            ("", "+")
        } else if let Some(m) = text.strip_suffix("++") {
            (m, "+")
        } else {
            match text.rsplit_once('+') {
                Some((m, k)) => (m, k),
                None => ("", text.as_str()),
            }
        };
        let mut s = Shortcut::new(false, false, false, false, key.trim());
        if !valid_key(&s.key) {
            return Err(format!("unknown key '{key}'"));
        }
        for m in mods.split('+').map(str::trim).filter(|m| !m.is_empty()) {
            match m {
                "ctrl" | "control" => s.ctrl = true,
                "alt" | "option" | "opt" => s.alt = true,
                "shift" => s.shift = true,
                "cmd" | "command" | "super" | "meta" | "win" => s.cmd = true,
                other => return Err(format!("unknown modifier '{other}'")),
            }
        }
        Ok(s)
    }
}

impl Serialize for Shortcut {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Shortcut {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Shortcut, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

// -------------------------------------------------------------------- enums

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InteractKey {
    #[default]
    Option,
    Command,
    Control,
    Off,
}

impl InteractKey {
    pub const ALL: [InteractKey; 4] = [InteractKey::Option, InteractKey::Command, InteractKey::Control, InteractKey::Off];

    pub fn title(self) -> &'static str {
        match self {
            InteractKey::Option => "⌥ Option",
            InteractKey::Command => "⌘ Command",
            InteractKey::Control => "⌃ Control",
            InteractKey::Off => "Off",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            InteractKey::Option => "⌥",
            InteractKey::Command => "⌘",
            InteractKey::Control => "⌃",
            InteractKey::Off => "",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InkChoice {
    /// Use the pointer's own ink colour.
    #[default]
    Match,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    White,
    Black,
}

impl InkChoice {
    pub const ALL: [InkChoice; 9] = [
        InkChoice::Match,
        InkChoice::Red,
        InkChoice::Orange,
        InkChoice::Yellow,
        InkChoice::Green,
        InkChoice::Blue,
        InkChoice::Purple,
        InkChoice::White,
        InkChoice::Black,
    ];

    pub fn title(self) -> &'static str {
        match self {
            InkChoice::Match => "Match the pointer",
            InkChoice::Red => "Red",
            InkChoice::Orange => "Orange",
            InkChoice::Yellow => "Yellow",
            InkChoice::Green => "Green",
            InkChoice::Blue => "Blue",
            InkChoice::Purple => "Purple",
            InkChoice::White => "White",
            InkChoice::Black => "Black",
        }
    }

    /// The colour as 0xRRGGBB; `None` for `Match`.
    pub fn colour(self) -> Option<u32> {
        Some(match self {
            InkChoice::Match => return None,
            InkChoice::Red => styles::INK_RED,
            InkChoice::Orange => 0xFF8A00,
            InkChoice::Yellow => 0xFFCC00,
            InkChoice::Green => 0x30C25A,
            InkChoice::Blue => 0x0A84FF,
            InkChoice::Purple => 0x8754F2,
            InkChoice::White => 0xFFFFFF,
            InkChoice::Black => 0x1D1D21,
        })
    }

    /// The colour to draw with for a pointer whose own ink is `style_ink`.
    pub fn resolved(self, style_ink: u32) -> u32 {
        self.colour().unwrap_or(style_ink)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TapSound {
    #[default]
    Thock,
    Click,
    Tock,
    Clack,
    Typewriter,
    Snap,
    Wood,
    Pop,
    Boop,
    Blip,
    Coin,
    Pew,
    Squeak,
    Quack,
    Chirp,
}

impl TapSound {
    pub const ALL: [TapSound; 15] = [
        TapSound::Thock,
        TapSound::Click,
        TapSound::Tock,
        TapSound::Clack,
        TapSound::Typewriter,
        TapSound::Snap,
        TapSound::Wood,
        TapSound::Pop,
        TapSound::Boop,
        TapSound::Blip,
        TapSound::Coin,
        TapSound::Pew,
        TapSound::Squeak,
        TapSound::Quack,
        TapSound::Chirp,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TapSound::Thock => "thock",
            TapSound::Click => "click",
            TapSound::Tock => "tock",
            TapSound::Clack => "clack",
            TapSound::Typewriter => "typewriter",
            TapSound::Snap => "snap",
            TapSound::Wood => "wood",
            TapSound::Pop => "pop",
            TapSound::Boop => "boop",
            TapSound::Blip => "blip",
            TapSound::Coin => "coin",
            TapSound::Pew => "pew",
            TapSound::Squeak => "squeak",
            TapSound::Quack => "quack",
            TapSound::Chirp => "chirp",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            TapSound::Wood => "Wood block",
            TapSound::Thock => "Thock",
            TapSound::Click => "Click",
            TapSound::Tock => "Tock",
            TapSound::Clack => "Clack",
            TapSound::Typewriter => "Typewriter",
            TapSound::Snap => "Snap",
            TapSound::Pop => "Pop",
            TapSound::Boop => "Boop",
            TapSound::Blip => "Blip",
            TapSound::Coin => "Coin",
            TapSound::Pew => "Pew",
            TapSound::Squeak => "Squeak",
            TapSound::Quack => "Quack",
            TapSound::Chirp => "Chirp",
        }
    }
}

/// The settings value for a tap effect: `off`, `fairyDust` or `fairyDustSubtle`.
pub fn tap_effect_id(e: TapEffect) -> &'static str {
    match e {
        TapEffect::Off => "off",
        TapEffect::FairyDust => "fairyDust",
        TapEffect::FairyDustSubtle => "fairyDustSubtle",
    }
}

pub fn tap_effect_title(e: TapEffect) -> &'static str {
    match e {
        TapEffect::Off => "Off",
        TapEffect::FairyDust => "Fairy dust",
        TapEffect::FairyDustSubtle => "Fairy dust, subtle",
    }
}

pub fn tap_effect_from_id(id: &str) -> Option<TapEffect> {
    match id {
        "off" => Some(TapEffect::Off),
        "fairyDust" => Some(TapEffect::FairyDust),
        "fairyDustSubtle" => Some(TapEffect::FairyDustSubtle),
        _ => None,
    }
}

/// serde adapter for `TapEffect`, which has no serde of its own.
mod tap_effect_serde {
    use super::*;

    pub fn serialize<S: Serializer>(e: &TapEffect, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(tap_effect_id(*e))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<TapEffect, D::Error> {
        let id = String::deserialize(d)?;
        tap_effect_from_id(&id).ok_or_else(|| serde::de::Error::custom(format!("unknown tap effect '{id}'")))
    }
}

// ----------------------------------------------------------------- settings

/// Every preference. Missing keys take their Swift defaults; unknown keys are ignored.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(rename = "shortcut.toggle")]
    pub toggle_shortcut: Shortcut,
    #[serde(rename = "shortcut.clear")]
    pub clear_shortcut: Shortcut,
    #[serde(rename = "pointerSize")]
    pub pointer_size: f32,
    #[serde(rename = "showShadow")]
    pub show_shadow: bool,
    #[serde(rename = "fadeSeconds")]
    pub fade_seconds: f64,
    #[serde(rename = "sound.hands")]
    pub hand_sound: bool,
    #[serde(rename = "sound.laser")]
    pub laser_sound: bool,
    #[serde(rename = "tapSound")]
    pub tap_sound: TapSound,
    #[serde(rename = "tapEffect", with = "tap_effect_serde")]
    pub tap_effect: TapEffect,
    #[serde(rename = "interactKeepsHand")]
    pub interact_keeps_hand: bool,
    #[serde(rename = "doubleShiftClickThrough")]
    pub double_shift_click_through: bool,
    #[serde(rename = "interactKey")]
    pub interact_key: InteractKey,
    #[serde(rename = "inkChoice")]
    pub ink_choice: InkChoice,
    #[serde(rename = "showToolbar")]
    pub show_toolbar: bool,
    #[serde(rename = "showInkButton")]
    pub show_ink_button: bool,
    #[serde(rename = "pinnedStyles")]
    pub pinned_styles: Vec<String>,
    /// A style id; see `styles::find`.
    #[serde(rename = "pointerStyle")]
    pub pointer_style: String,
    #[serde(rename = "welcomeShown")]
    pub welcome_shown: bool,
    #[serde(rename = "firstRunHintShown")]
    pub first_run_hint_shown: bool,
    #[serde(rename = "loginItemSetUp")]
    pub login_item_set_up: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            toggle_shortcut: Shortcut::toggle_default(),
            clear_shortcut: Shortcut::clear_default(),
            pointer_size: 0.13,
            show_shadow: true,
            fade_seconds: 3.0,
            hand_sound: true,
            laser_sound: false,
            tap_sound: TapSound::Thock,
            tap_effect: TapEffect::FairyDust,
            interact_keeps_hand: false,
            double_shift_click_through: true,
            interact_key: InteractKey::Option,
            ink_choice: InkChoice::Match,
            show_toolbar: true,
            show_ink_button: true,
            pinned_styles: default_pinned(),
            pointer_style: DEFAULT_STYLE.into(),
            welcome_shown: false,
            first_run_hint_shown: false,
            login_item_set_up: false,
        }
    }
}

fn default_pinned() -> Vec<String> {
    DEFAULT_PINNED.iter().map(|s| s.to_string()).collect()
}

/// Drops repeats (keeping the first), falls back to the defaults when empty, and keeps at
/// most `MAX_PINNED`.
pub fn normalise_pinned(raw: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut v: Vec<String> = raw.iter().filter(|s| seen.insert(s.as_str())).cloned().collect();
    v.truncate(MAX_PINNED);
    if v.is_empty() {
        default_pinned()
    } else {
        v
    }
}

/// Old saved style names: "dot" became the laser, "sunny" became coral.
pub fn migrate_style(id: &str) -> &str {
    match id {
        "dot" => "laser",
        "sunny" => "coral",
        other => other,
    }
}

impl Settings {
    /// Applies the legacy style names and the pinned-styles rules.
    pub fn normalise(&mut self) {
        self.pointer_style = migrate_style(&self.pointer_style).to_string();
        self.pinned_styles = normalise_pinned(&self.pinned_styles);
    }

    /// The pinned styles that exist, given the custom hands (a deleted hand drops out).
    /// Empty falls back to the defaults.
    pub fn pinned_existing(&self, custom: &[crate::custom_hands::CustomHand]) -> Vec<String> {
        let v: Vec<String> = self
            .pinned_styles
            .iter()
            .filter(|id| styles::find(id, custom).is_some())
            .cloned()
            .collect();
        if v.is_empty() {
            default_pinned()
        } else {
            v
        }
    }

    /// The saved pointer style if it exists, else the default.
    pub fn style_existing(&self, custom: &[crate::custom_hands::CustomHand]) -> String {
        if styles::find(&self.pointer_style, custom).is_some() {
            self.pointer_style.clone()
        } else {
            DEFAULT_STYLE.into()
        }
    }

    /// The "Reset" button: every key Swift's resetAll removes goes back to its default.
    /// The welcome, first-run hint and login item flags stay.
    pub fn reset_all(&mut self) {
        *self = Settings {
            welcome_shown: self.welcome_shown,
            first_run_hint_shown: self.first_run_hint_shown,
            login_item_set_up: self.login_item_set_up,
            ..Settings::default()
        };
    }

    /// Reads settings JSON. Anything unreadable gives the defaults; a key with a wrong
    /// type or value is skipped on its own, the rest still load.
    pub fn from_json(text: &str) -> Settings {
        let Ok(Value::Object(input)) = serde_json::from_str::<Value>(text) else {
            return Settings::default();
        };
        let Ok(Value::Object(mut map)) = serde_json::to_value(Settings::default()) else {
            return Settings::default();
        };
        for (key, value) in input {
            if !map.contains_key(&key) {
                continue;
            }
            let old = map.insert(key.clone(), value);
            if serde_json::from_value::<Settings>(Value::Object(map.clone())).is_err() {
                if let Some(old) = old {
                    map.insert(key, old);
                }
            }
        }
        let mut s: Settings = serde_json::from_value(Value::Object(map)).unwrap_or_default();
        s.normalise();
        s
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Reads the file. Missing or corrupt gives the defaults; this never panics.
    pub fn load_from(path: &Path) -> Settings {
        std::fs::read(path)
            .map(|b| Settings::from_json(&String::from_utf8_lossy(&b)))
            .unwrap_or_default()
    }

    /// Writes the file atomically (temp file, then rename), creating the folder.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        write_atomic(path, self.to_json().as_bytes())
    }

    pub fn load() -> Settings {
        Settings::load_from(&settings_path())
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&settings_path())
    }
}

// ------------------------------------------------------------------- files

/// Where TinyHand keeps its files: `~/Library/Application Support/TinyHand` on macOS,
/// `~/.config/tinyhand` on Linux, `%APPDATA%\TinyHand` on Windows.
///
/// `TH_CONFIG_DIR` overrides it, for tests that must not touch the real settings.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("TH_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(if cfg!(target_os = "linux") { "tinyhand" } else { "TinyHand" })
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Writes to a temp file next to `path`, then renames it over `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let result = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

// -------------------------------------------------------- macOS defaults import

/// Carbon modifier bits in the old shortcut dictionaries.
const CMD_KEY: i64 = 0x100;
const SHIFT_KEY: i64 = 0x200;
const OPTION_KEY: i64 = 0x800;
const CONTROL_KEY: i64 = 0x1000;

/// The key name for a Carbon virtual key code (kVK_*), or `None` if unknown.
pub fn carbon_key_name(code: i64) -> Option<&'static str> {
    Some(match code {
        0x00 => "a", 0x01 => "s", 0x02 => "d", 0x03 => "f", 0x04 => "h", 0x05 => "g",
        0x06 => "z", 0x07 => "x", 0x08 => "c", 0x09 => "v", 0x0B => "b", 0x0C => "q",
        0x0D => "w", 0x0E => "e", 0x0F => "r", 0x10 => "y", 0x11 => "t",
        0x12 => "1", 0x13 => "2", 0x14 => "3", 0x15 => "4", 0x16 => "6", 0x17 => "5",
        0x18 => "=", 0x19 => "9", 0x1A => "7", 0x1B => "-", 0x1C => "8", 0x1D => "0",
        0x1E => "]", 0x1F => "o", 0x20 => "u", 0x21 => "[", 0x22 => "i", 0x23 => "p",
        0x25 => "l", 0x26 => "j", 0x27 => "'", 0x28 => "k", 0x29 => ";", 0x2A => "\\",
        0x2B => ",", 0x2C => "/", 0x2D => "n", 0x2E => "m", 0x2F => ".", 0x32 => "`",
        0x24 => "enter", 0x30 => "tab", 0x31 => "space", 0x33 => "backspace",
        0x35 => "escape", 0x75 => "delete",
        0x7B => "left", 0x7C => "right", 0x7D => "down", 0x7E => "up",
        0x73 => "home", 0x77 => "end", 0x74 => "pageup", 0x79 => "pagedown",
        0x7A => "f1", 0x78 => "f2", 0x63 => "f3", 0x76 => "f4", 0x60 => "f5", 0x61 => "f6",
        0x62 => "f7", 0x64 => "f8", 0x65 => "f9", 0x6D => "f10", 0x67 => "f11", 0x6F => "f12",
        _ => return None,
    })
}

/// Converts a Carbon `{keyCode, modifiers}` pair to a shortcut.
pub fn shortcut_from_carbon(key_code: i64, modifiers: i64) -> Option<Shortcut> {
    let key = carbon_key_name(key_code)?;
    Some(Shortcut::new(
        modifiers & CONTROL_KEY != 0,
        modifiers & OPTION_KEY != 0,
        modifiers & SHIFT_KEY != 0,
        modifiers & CMD_KEY != 0,
        key,
    ))
}

fn plist_number(v: &plist::Value) -> Option<f64> {
    v.as_real().or_else(|| v.as_signed_integer().map(|i| i as f64))
}

fn plist_shortcut(v: &plist::Value) -> Option<Shortcut> {
    let d = v.as_dictionary()?;
    let code = d.get("keyCode").and_then(plist_number)?;
    let mods = d.get("modifiers").and_then(plist_number)?;
    shortcut_from_carbon(code as i64, mods as i64)
}

/// Reads the output of `defaults export com.kamranahmed.tinyhand -` (an XML plist).
/// Keys that are missing or not understood keep their defaults; bad input gives all
/// defaults.
pub fn import_macos_defaults(plist_bytes: &[u8]) -> Settings {
    let mut s = Settings::default();
    let Ok(root) = plist::Value::from_reader(io::Cursor::new(plist_bytes)) else {
        return s;
    };
    let Some(d) = root.as_dictionary() else {
        return s;
    };
    let text = |k: &str| d.get(k).and_then(|v| v.as_string());
    let flag = |k: &str| d.get(k).and_then(|v| v.as_boolean());
    let num = |k: &str| d.get(k).and_then(plist_number);
    fn pick<T: serde::de::DeserializeOwned>(d: &plist::Dictionary, k: &str) -> Option<T> {
        let t = d.get(k)?.as_string()?;
        serde_json::from_value(Value::String(t.into())).ok()
    }

    if let Some(v) = d.get("shortcut.toggle").and_then(plist_shortcut) {
        s.toggle_shortcut = v;
    }
    if let Some(v) = d.get("shortcut.clear").and_then(plist_shortcut) {
        s.clear_shortcut = v;
    }
    if let Some(v) = num("pointerSize") {
        s.pointer_size = v as f32;
    }
    if let Some(v) = flag("showShadow") {
        s.show_shadow = v;
    }
    if let Some(v) = num("fadeSeconds") {
        s.fade_seconds = v;
    }
    if let Some(v) = flag("sound.hands") {
        s.hand_sound = v;
    }
    if let Some(v) = flag("sound.laser") {
        s.laser_sound = v;
    }
    if let Some(v) = pick(d, "tapSound") {
        s.tap_sound = v;
    }
    if let Some(v) = text("tapEffect").and_then(tap_effect_from_id) {
        s.tap_effect = v;
    }
    if let Some(v) = flag("interactKeepsHand") {
        s.interact_keeps_hand = v;
    }
    if let Some(v) = flag("doubleShiftClickThrough") {
        s.double_shift_click_through = v;
    }
    if let Some(v) = pick(d, "interactKey") {
        s.interact_key = v;
    }
    if let Some(v) = pick(d, "inkChoice") {
        s.ink_choice = v;
    }
    if let Some(v) = flag("showToolbar") {
        s.show_toolbar = v;
    }
    if let Some(v) = flag("showInkButton") {
        s.show_ink_button = v;
    }
    if let Some(a) = d.get("pinnedStyles").and_then(|v| v.as_array()) {
        s.pinned_styles = a.iter().filter_map(|v| v.as_string().map(String::from)).collect();
    }
    if let Some(v) = text("pointerStyle") {
        s.pointer_style = v.to_string();
    }
    if let Some(v) = flag("welcomeShown") {
        s.welcome_shown = v;
    }
    if let Some(v) = flag("firstRunHintShown") {
        s.first_run_hint_shown = v;
    }
    if let Some(v) = flag("loginItemSetUp") {
        s.login_item_set_up = v;
    }
    s.normalise();
    s
}
