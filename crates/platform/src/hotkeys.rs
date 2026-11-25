//! Global shortcuts, written as text like `"ctrl+alt+h"` so a stored shortcut means the
//! same on every OS. Wraps the `global-hotkey` crate.
//!
//! Where presses come from, per OS:
//! - **macOS**: Carbon hot keys. They arrive through the main thread's NSApp event loop,
//!   which GPUI runs, so `Hotkeys::new` must be called on the main thread and the handler
//!   runs on it.
//! - **Windows**: `RegisterHotKey` on a hidden window owned by the thread that called
//!   `Hotkeys::new`. That thread needs a message loop; GPUI's main thread has one. The
//!   handler runs on that thread.
//! - **Linux X11** (and XWayland): `global-hotkey` grabs keys on its own X connection and
//!   thread; the handler runs on that thread.
//! - **Linux, pure Wayland** (no `DISPLAY`): Wayland has no way to grab keys for other
//!   apps, so `Hotkeys::new` returns [`HotkeyError::Unavailable`]. Use the tray menu or a
//!   compositor key binding running `tinyhand toggle` instead.
//!
//! The handler can run on any thread, so post to the app's main thread from it.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyId {
    Toggle,
    Clear,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyError {
    /// The text isn't a shortcut this module understands.
    Parse(String),
    /// Another app holds this shortcut.
    Taken,
    /// Another of our own ids already uses this shortcut.
    Duplicate(HotkeyId),
    /// Global shortcuts don't exist here (pure Wayland).
    Unavailable(String),
    /// Anything else the OS reported.
    Os(String),
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyError::Parse(s) => write!(f, "not a shortcut: {s}"),
            HotkeyError::Taken => write!(f, "another app already uses this shortcut"),
            HotkeyError::Duplicate(id) => write!(f, "this shortcut is already used for {id:?}"),
            HotkeyError::Unavailable(s) => write!(f, "global shortcuts are unavailable: {s}"),
            HotkeyError::Os(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for HotkeyError {}

type Handler = Box<dyn Fn(HotkeyId) + Send + 'static>;

/// `global-hotkey` takes one process-wide event handler, set once. It forwards here, so a
/// new `Hotkeys` (or a new handler) still gets the presses.
struct Dispatch {
    handler: Mutex<Option<Handler>>,
    /// `global-hotkey` id → ours, for every registered shortcut.
    ids: Mutex<HashMap<u32, HotkeyId>>,
}

fn dispatch() -> &'static Dispatch {
    static DISPATCH: OnceLock<Dispatch> = OnceLock::new();
    DISPATCH.get_or_init(|| {
        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            if event.state != HotKeyState::Pressed {
                return;
            }
            let d = dispatch();
            let Some(id) = d.ids.lock().unwrap().get(&event.id).copied() else {
                return;
            };
            if let Some(handler) = d.handler.lock().unwrap().as_ref() {
                handler(id);
            }
        }));
        Dispatch { handler: Mutex::new(None), ids: Mutex::new(HashMap::new()) }
    })
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    registered: HashMap<HotkeyId, HotKey>,
}

impl Hotkeys {
    /// See the module docs for which thread to call this on.
    pub fn new() -> Result<Hotkeys, HotkeyError> {
        #[cfg(target_os = "linux")]
        if std::env::var_os("DISPLAY").is_none_or(|d| d.is_empty()) {
            return Err(HotkeyError::Unavailable(
                "this is a Wayland session without X11; Wayland doesn't let apps register global shortcuts".into(),
            ));
        }
        dispatch();
        let manager = GlobalHotKeyManager::new().map_err(|e| HotkeyError::Os(e.to_string()))?;
        Ok(Hotkeys { manager, registered: HashMap::new() })
    }

    /// Presses of registered shortcuts call `handler`, possibly on another thread.
    pub fn set_handler(&self, handler: impl Fn(HotkeyId) + Send + 'static) {
        *dispatch().handler.lock().unwrap() = Some(Box::new(handler));
    }

    /// Registers `shortcut` for `id`, replacing what `id` had. On failure `id` is left
    /// unregistered.
    pub fn register(&mut self, id: HotkeyId, shortcut: &str) -> Result<(), HotkeyError> {
        let hotkey = parse(shortcut)?;
        if let Some((&other, _)) = self.registered.iter().find(|(o, h)| **o != id && h.id() == hotkey.id()) {
            return Err(HotkeyError::Duplicate(other));
        }
        self.unregister(id);
        match self.manager.register(hotkey) {
            Ok(()) => {
                dispatch().ids.lock().unwrap().insert(hotkey.id(), id);
                self.registered.insert(id, hotkey);
                Ok(())
            }
            // We never hold `hotkey` here (checked above), so this means another app does.
            Err(global_hotkey::Error::AlreadyRegistered(_)) => Err(HotkeyError::Taken),
            Err(e) => Err(HotkeyError::Os(e.to_string())),
        }
    }

    pub fn unregister(&mut self, id: HotkeyId) {
        if let Some(hotkey) = self.registered.remove(&id) {
            dispatch().ids.lock().unwrap().remove(&hotkey.id());
            if let Err(e) = self.manager.unregister(hotkey) {
                eprintln!("hotkeys: unregistering {id:?}: {e}");
            }
        }
    }

    pub fn unregister_all(&mut self) {
        let ids: Vec<HotkeyId> = self.registered.keys().copied().collect();
        for id in ids {
            self.unregister(id);
        }
    }

    pub fn is_registered(&self, id: HotkeyId) -> bool {
        self.registered.contains_key(&id)
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        self.unregister_all();
    }
}

/// Parses `"ctrl+alt+h"`: any of `ctrl`, `alt`, `shift`, `cmd`, then one key. Case and
/// spaces around `+` don't matter. `cmd` is ⌘ on macOS and the Windows/Super key elsewhere.
pub fn parse(shortcut: &str) -> Result<HotKey, HotkeyError> {
    let err = || HotkeyError::Parse(shortcut.to_string());
    let parts: Vec<String> = shortcut.split('+').map(|p| p.trim().to_ascii_lowercase()).collect();
    let (key, mods) = parts.split_last().ok_or_else(err)?;
    let mut modifiers = Modifiers::empty();
    for m in mods {
        modifiers |= match m.as_str() {
            "ctrl" | "control" => Modifiers::CONTROL,
            "alt" | "option" | "opt" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            "cmd" | "command" | "super" | "meta" | "win" => Modifiers::META,
            _ => return Err(err()),
        };
    }
    let code = key_code(key).ok_or_else(err)?;
    Ok(HotKey::new(Some(modifiers), code))
}

fn key_code(key: &str) -> Option<Code> {
    use Code::*;
    const LETTERS: [Code; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [Code; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    const FKEYS: [Code; 12] = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];

    let bytes = key.as_bytes();
    if bytes.len() == 1 {
        let c = bytes[0];
        return match c {
            b'a'..=b'z' => Some(LETTERS[(c - b'a') as usize]),
            b'0'..=b'9' => Some(DIGITS[(c - b'0') as usize]),
            _ => None,
        };
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<usize>().ok()) {
        return (1..=12).contains(&n).then(|| FKEYS[n - 1]);
    }
    Some(match key {
        "space" => Space,
        "enter" | "return" => Enter,
        "tab" => Tab,
        "backspace" => Backspace,
        "delete" => Delete,
        "escape" | "esc" => Escape,
        "left" => ArrowLeft,
        "right" => ArrowRight,
        "up" => ArrowUp,
        "down" => ArrowDown,
        "home" => Home,
        "end" => End,
        "pageup" => PageUp,
        "pagedown" => PageDown,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        assert_eq!(parse("ctrl+alt+h").unwrap(), HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyH));
        assert_eq!(parse("Cmd + Shift + F12").unwrap(), HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), Code::F12));
        assert_eq!(parse("escape").unwrap(), HotKey::new(None, Code::Escape));
        assert_eq!(parse("alt+7").unwrap().key, Code::Digit7);
        assert_eq!(parse("ctrl+pagedown").unwrap().key, Code::PageDown);
        for bad in ["", "ctrl+", "hyper+h", "ctrl+f13", "ctrl+hh", "ctrl+alt"] {
            assert!(matches!(parse(bad), Err(HotkeyError::Parse(_))), "{bad}");
        }
    }

    /// Registers and unregisters a real shortcut. Needs a desktop session (on macOS, the
    /// main thread isn't required just to register).
    #[test]
    #[ignore]
    fn register_round_trip() {
        let mut keys = Hotkeys::new().unwrap();
        keys.set_handler(|_| {});
        keys.register(HotkeyId::Toggle, "ctrl+alt+shift+f9").unwrap();
        assert_eq!(keys.register(HotkeyId::Clear, "ctrl+alt+shift+f9"), Err(HotkeyError::Duplicate(HotkeyId::Toggle)));
        keys.register(HotkeyId::Escape, "escape").unwrap();
        keys.register(HotkeyId::Toggle, "ctrl+alt+shift+f10").unwrap();
        keys.unregister(HotkeyId::Escape);
        assert!(!keys.is_registered(HotkeyId::Escape));
        keys.unregister_all();
        assert!(!keys.is_registered(HotkeyId::Toggle));
    }
}
