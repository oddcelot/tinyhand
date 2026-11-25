//! Global mouse interception ("the tap"), ported from `InputTap.swift`.
//!
//! While the tap is enabled, mouse downs and ups are swallowed so the apps underneath never
//! get a click, drags reach them as plain moves, and the app hears about every press through
//! [`InputEvent`]s. Some presses still go through:
//!
//! - with the interact modifier held (or click-through latched by a double ⇧), the press and
//!   everything up to its matching up pass through, with the modifier removed from the
//!   events ([`InputEvent::InteractDown`]);
//! - presses inside a pass-through rect (menu bar, toolbar, open menus) pass through up to
//!   their up ([`InputEvent::PassThroughPress`]);
//! - on a locked screen presses pass through ([`InputEvent::ScreenLocked`]).
//!
//! `on_event` is called on the tap's own thread (macOS: the "TinyHand event tap" run loop
//! thread; Windows: the hook thread). It must not block: the OS waits for it before it
//! delivers the event, and both OSes turn the tap off when it takes too long. Send the event
//! to a channel and return.
//!
//! Per OS:
//! - **macOS**: a `CGEventTap` (session tap, head insert, default tap). Needs Accessibility.
//! - **Windows**: `WH_MOUSE_LL` and `WH_KEYBOARD_LL` hooks. A low-level hook can only pass or
//!   drop an event, not change it, so: drags pass as they are (the button press was dropped,
//!   so apps only ever see moves); the interact modifier can't be removed from a press that
//!   passes through (the app gets e.g. an Alt-click); a locked screen is never reported
//!   (the hooks don't see the secure desktop's input); Windows silently removes hooks that
//!   are too slow, which can't be detected or undone. Coordinates are in the process's DPI
//!   space (physical pixels for a per-monitor-aware app).
//! - **Linux**: not supported; the app takes the mouse with its capture window instead.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as imp;

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// A modifier key that makes a press reach the app ("interact key").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Modifier {
    /// ⌥ Option on macOS, Alt elsewhere.
    Alt,
    /// ⌘ Command on macOS, the Windows key on Windows.
    Command,
    Control,
}

/// A rectangle in global screen points, y down, origin at the top left of the primary display.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x, y, w, h }
    }

    /// Same rule as `CGRect.contains`: the left and top edges are inside, the right and
    /// bottom edges are not.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    /// Middle and extra buttons, numbered like macOS (2 = middle).
    Other(u32),
}

/// What the tap tells the app. Coordinates are global screen points (see [`Rect`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    /// A press that was swallowed.
    MouseDown { x: f64, y: f64, button: MouseButton },
    /// A release that was swallowed.
    MouseUp { x: f64, y: f64, button: MouseButton },
    /// The mouse moved with the (swallowed) left button down.
    LeftDragged { x: f64, y: f64 },
    /// A press that passes through because the interact modifier is held or click-through
    /// is latched.
    InteractDown,
    /// A press inside a pass-through rect, which passes through.
    PassThroughPress { x: f64, y: f64 },
    /// A press on a locked screen, which passes through. The app should turn off.
    ScreenLocked,
    /// The modifier keys changed (only while the tap is enabled).
    Modifiers { shift: bool, control: bool, alt: bool, command: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputError {
    /// This OS (or session) has no way to swallow clicks for other apps.
    Unsupported,
    /// macOS: the app isn't trusted for Accessibility, or the tap couldn't be created.
    NotTrusted,
    /// Windows: only one tap can run at a time.
    AlreadyRunning,
    /// The OS refused; the text says what failed.
    Failed(String),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::Unsupported => write!(f, "swallowing clicks isn't supported here"),
            InputError::NotTrusted => write!(f, "the event tap needs Accessibility permission"),
            InputError::AlreadyRunning => write!(f, "an input tap is already running"),
            InputError::Failed(why) => write!(f, "input tap failed: {why}"),
        }
    }
}

impl std::error::Error for InputError {}

/// The global mouse tap. Dropping it removes the tap and stops its thread.
pub struct InputTap {
    inner: imp::Tap,
}

impl InputTap {
    /// Installs the tap, disabled. Call [`set_enabled`](Self::set_enabled) to start
    /// swallowing. `on_event` runs on the tap thread and must return quickly.
    pub fn start(on_event: impl Fn(InputEvent) + Send + 'static) -> Result<InputTap, InputError> {
        imp::Tap::start(Box::new(on_event)).map(|inner| InputTap { inner })
    }

    /// Turns swallowing on or off. Also forgets an interact press in progress.
    pub fn set_enabled(&self, on: bool) {
        {
            let mut s = self.inner.shared().state.lock().unwrap();
            s.enabled = on;
            s.interact_press = false;
        }
        self.inner.enabled_changed(on);
    }

    pub fn set_interact_key(&self, key: Option<Modifier>) {
        self.inner.shared().state.lock().unwrap().interact_key = key;
    }

    /// Latched by a double ⇧: every press reaches the app, as if the interact key were held.
    pub fn set_click_through(&self, on: bool) {
        self.inner.shared().state.lock().unwrap().click_through = on;
    }

    pub fn set_pass_through(&self, rects: Vec<Rect>) {
        self.inner.shared().state.lock().unwrap().pass_through = rects;
    }

    /// The longest the OS took to hand an event to the tap, in ms (macOS; 0 elsewhere).
    pub fn max_wait_ms(&self) -> f64 {
        f64::from_bits(self.inner.shared().max_wait.load(Ordering::Relaxed))
    }

    pub fn reset_wait(&self) {
        self.inner.shared().max_wait.store(0f64.to_bits(), Ordering::Relaxed);
    }
}

/// Whether the app may tap input (macOS: trusted for Accessibility; elsewhere true).
pub fn is_trusted() -> bool {
    imp::is_trusted()
}

/// macOS: shows the Accessibility prompt and opens Privacy & Security › Accessibility.
/// Elsewhere does nothing.
pub fn request_trust() {
    imp::request_trust()
}

/// Whether [`InputTap::start`] can work right now.
pub fn can_tap() -> bool {
    imp::can_tap()
}

/// A count of key presses and mouse presses that only goes up. Comparing it across a ⇧ tap
/// tells a clean tap from typing a capital letter. macOS: the HID system's counters.
/// Windows: counted by the running tap's hooks (0 without a tap). Linux: always 0.
pub fn input_counter() -> u64 {
    imp::input_counter()
}

/// Whether the left mouse button is down right now (Linux: always false).
pub fn left_button_down() -> bool {
    imp::left_button_down()
}

// ---- the state machine both taps share ----

pub(crate) struct State {
    pub enabled: bool,
    pub interact_key: Option<Modifier>,
    pub click_through: bool,
    pub pass_through: Vec<Rect>,
    pub interact_press: bool,
    pub passing_through: bool,
}

pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub max_wait: AtomicU64,
    on_event: Mutex<Box<dyn Fn(InputEvent) + Send>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // not every OS builds every variant
pub(crate) enum Kind {
    Down(MouseButton),
    Up(MouseButton),
    /// A move with a button held; `left` if it is the left button.
    Drag { left: bool },
}

/// What to do with the event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Action {
    Pass,
    /// Pass, with this modifier removed from the event's flags.
    PassStripping(Option<Modifier>),
    Swallow,
    /// Pass as a plain mouse move.
    Move,
}

impl Shared {
    #[allow(dead_code)]
    pub fn new(on_event: Box<dyn Fn(InputEvent) + Send>) -> Self {
        Shared {
            state: Mutex::new(State {
                enabled: false,
                interact_key: None,
                click_through: false,
                pass_through: Vec::new(),
                interact_press: false,
                passing_through: false,
            }),
            max_wait: AtomicU64::new(0f64.to_bits()),
            on_event: Mutex::new(on_event),
        }
    }

    pub fn emit(&self, event: InputEvent) {
        if let Ok(f) = self.on_event.lock() {
            f(event);
        }
    }

    #[allow(dead_code)]
    pub fn enabled(&self) -> bool {
        self.state.lock().unwrap().enabled
    }

    #[allow(dead_code)]
    pub fn note_wait(&self, ms: f64) {
        if ms < 600_000.0 {
            let mut old = self.max_wait.load(Ordering::Relaxed);
            while ms > f64::from_bits(old) {
                match self.max_wait.compare_exchange_weak(old, ms.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => break,
                    Err(now) => old = now,
                }
            }
        }
    }

    /// `InputTap.handle` from the Swift app, for a mouse event at (`x`, `y`).
    /// `key_held` says whether a modifier is down (event flags or the current HID state);
    /// `locked` whether the screen is locked. Events are emitted after the lock is released.
    #[allow(dead_code)]
    pub fn decide(
        &self,
        kind: Kind,
        x: f64,
        y: f64,
        key_held: impl Fn(Modifier) -> bool,
        locked: impl Fn() -> bool,
    ) -> Action {
        let (action, event) = self.decide_inner(kind, x, y, key_held, locked);
        if let Some(event) = event {
            self.emit(event);
        }
        action
    }

    fn decide_inner(
        &self,
        kind: Kind,
        x: f64,
        y: f64,
        key_held: impl Fn(Modifier) -> bool,
        locked: impl Fn() -> bool,
    ) -> (Action, Option<InputEvent>) {
        let mut s = self.state.lock().unwrap();
        if !s.enabled {
            return (Action::Pass, None);
        }
        let key = s.interact_key;
        let mut event = None;

        if let Kind::Down(_) = kind {
            let via_key = key.is_some_and(&key_held);
            if via_key || s.click_through {
                s.interact_press = true;
                event = Some(InputEvent::InteractDown);
            }
        }
        if s.interact_press {
            if let Kind::Up(_) = kind {
                s.interact_press = false;
            }
            return (Action::PassStripping(key), event);
        }

        if let Kind::Down(_) = kind {
            s.passing_through = s.pass_through.iter().any(|r| r.contains(x, y));
            if s.passing_through {
                event = Some(InputEvent::PassThroughPress { x, y });
            }
        }
        if s.passing_through {
            if let Kind::Up(_) = kind {
                s.passing_through = false;
            }
            return (Action::Pass, event);
        }
        drop(s);

        match kind {
            Kind::Down(button) => {
                if locked() {
                    (Action::Pass, Some(InputEvent::ScreenLocked))
                } else {
                    (Action::Swallow, Some(InputEvent::MouseDown { x, y, button }))
                }
            }
            Kind::Up(button) => {
                if locked() {
                    (Action::Pass, None)
                } else {
                    (Action::Swallow, Some(InputEvent::MouseUp { x, y, button }))
                }
            }
            Kind::Drag { left } => (Action::Move, left.then_some(InputEvent::LeftDragged { x, y })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn shared() -> (Shared, Arc<Mutex<Vec<InputEvent>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let l = log.clone();
        let s = Shared::new(Box::new(move |e| l.lock().unwrap().push(e)));
        s.state.lock().unwrap().enabled = true;
        (s, log)
    }

    const L: MouseButton = MouseButton::Left;

    #[test]
    fn swallows_and_rewrites_drags() {
        let (s, log) = shared();
        assert_eq!(s.decide(Kind::Down(L), 1.0, 2.0, |_| false, || false), Action::Swallow);
        assert_eq!(s.decide(Kind::Drag { left: true }, 3.0, 4.0, |_| false, || false), Action::Move);
        assert_eq!(s.decide(Kind::Up(L), 3.0, 4.0, |_| false, || false), Action::Swallow);
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                InputEvent::MouseDown { x: 1.0, y: 2.0, button: L },
                InputEvent::LeftDragged { x: 3.0, y: 4.0 },
                InputEvent::MouseUp { x: 3.0, y: 4.0, button: L },
            ]
        );
    }

    #[test]
    fn disabled_passes_everything() {
        let (s, log) = shared();
        s.state.lock().unwrap().enabled = false;
        assert_eq!(s.decide(Kind::Down(L), 0.0, 0.0, |_| true, || false), Action::Pass);
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn interact_key_passes_until_up() {
        let (s, log) = shared();
        s.state.lock().unwrap().interact_key = Some(Modifier::Alt);
        let alt = |m| m == Modifier::Alt;
        let strip = Action::PassStripping(Some(Modifier::Alt));
        assert_eq!(s.decide(Kind::Down(L), 0.0, 0.0, alt, || false), strip);
        // Released ⌥ mid-press: the rest of the press still passes.
        assert_eq!(s.decide(Kind::Drag { left: true }, 0.0, 0.0, |_| false, || false), strip);
        assert_eq!(s.decide(Kind::Up(L), 0.0, 0.0, |_| false, || false), strip);
        assert_eq!(s.decide(Kind::Down(L), 0.0, 0.0, |_| false, || false), Action::Swallow);
        assert_eq!(log.lock().unwrap()[0], InputEvent::InteractDown);
    }

    #[test]
    fn click_through_and_pass_through_rects() {
        let (s, log) = shared();
        s.state.lock().unwrap().click_through = true;
        assert_eq!(s.decide(Kind::Down(L), 0.0, 0.0, |_| false, || false), Action::PassStripping(None));
        s.decide(Kind::Up(L), 0.0, 0.0, |_| false, || false);
        s.state.lock().unwrap().click_through = false;
        s.state.lock().unwrap().pass_through = vec![Rect::new(10.0, 10.0, 5.0, 5.0)];
        assert_eq!(s.decide(Kind::Down(L), 12.0, 12.0, |_| false, || false), Action::Pass);
        assert_eq!(s.decide(Kind::Up(L), 50.0, 50.0, |_| false, || false), Action::Pass);
        assert_eq!(s.decide(Kind::Down(L), 15.0, 12.0, |_| false, || false), Action::Swallow);
        assert!(log.lock().unwrap().contains(&InputEvent::PassThroughPress { x: 12.0, y: 12.0 }));
    }

    #[test]
    fn locked_screen_passes() {
        let (s, log) = shared();
        assert_eq!(s.decide(Kind::Down(L), 0.0, 0.0, |_| false, || true), Action::Pass);
        assert_eq!(s.decide(Kind::Up(L), 0.0, 0.0, |_| false, || true), Action::Pass);
        assert_eq!(*log.lock().unwrap(), vec![InputEvent::ScreenLocked]);
    }

    #[test]
    fn max_wait_keeps_the_largest() {
        let (s, _) = shared();
        s.note_wait(5.0);
        s.note_wait(2.0);
        s.note_wait(700_000.0);
        assert_eq!(f64::from_bits(s.max_wait.load(Ordering::Relaxed)), 5.0);
    }

    /// Starts the real tap for under 3 s. Only runs with TINYHAND_TAP_TEST=1, and swallows
    /// clicks only if TINYHAND_TAP_TEST_SWALLOW=1 as well.
    #[test]
    #[ignore]
    fn live_tap_smoke() {
        if std::env::var("TINYHAND_TAP_TEST").as_deref() != Ok("1") {
            eprintln!("set TINYHAND_TAP_TEST=1 to run");
            return;
        }
        if !can_tap() {
            match InputTap::start(|_| {}) {
                Err(e) => eprintln!("can't tap, start() said: {e}"),
                Ok(_) => panic!("can_tap() was false but start() worked"),
            }
            return;
        }
        let tap = InputTap::start(|e| eprintln!("{e:?}")).expect("start");
        let swallow = std::env::var("TINYHAND_TAP_TEST_SWALLOW").as_deref() == Ok("1");
        tap.set_enabled(swallow);
        std::thread::sleep(std::time::Duration::from_millis(2500));
        tap.set_enabled(false);
        drop(tap);
        eprintln!("input_counter = {}", input_counter());
    }

    #[test]
    fn start_is_clean_without_permission() {
        // Never enables the tap, so no click is swallowed.
        match InputTap::start(|_| {}) {
            Ok(tap) => drop(tap),
            Err(e) => assert!(!e.to_string().is_empty()),
        }
        let _ = (is_trusted(), input_counter(), left_button_down());
    }
}
