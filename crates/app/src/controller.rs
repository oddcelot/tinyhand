//! The app's state machine: AppDelegate.swift and the non-drawing half of
//! OverlayController.swift. Turns the pointer on and off, owns the tray, the shortcuts,
//! the input tap, the cursor hider and the tap sound, and handles `tinyhand` commands.
//!
//! Everything from other threads (tap, tray, shortcuts, command socket) arrives as a
//! [`Msg`] on a channel that a main-thread task drains into [`Controller::handle`].

use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedSender;
use gpui::{
    App, AppContext, Bounds, Context, DisplayId, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use tinyhand_core::automation::{self, Command, SoundAction, State};
use tinyhand_core::custom_hands::CustomHands;
use tinyhand_core::settings::Settings;
use tinyhand_core::styles::{self, StyleKind};
use tinyhand_platform as native;
use tinyhand_platform::cursor::CursorHider;
use tinyhand_platform::hotkeys::{HotkeyId, Hotkeys};
use tinyhand_platform::input::{self, InputEvent, InputTap, Modifier};
use tinyhand_platform::sound::TapSounds;
use tinyhand_platform::tray::{Tray, TrayEvent};

use crate::overlay::{OverlayView, Scene};
use crate::shift::{Mods, ShiftTaps};
use crate::toolbar::Toolbar;
use crate::welcome::Welcome;
use crate::{assets, tray_menu};

pub enum Msg {
    Tray(TrayEvent),
    Hotkey(HotkeyId),
    Input(InputEvent),
    Command(Command),
}

/// Where the mouse comes from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Input {
    /// The overlay ignores the mouse; position comes from native calls and presses from
    /// the input tap (macOS, Windows).
    Watch,
    /// The overlay covers the screen and takes the mouse, so GPUI's own mouse events say
    /// where it is (Linux, or `TH_INPUT=capture`).
    Capture,
}

/// Settings for unattended runs, read from the environment once.
pub struct TestHooks {
    /// TH_FAKE_MOUSE=x,y pins the pointer. The input tap is never started with it.
    pub fake_mouse: Option<(f32, f32)>,
    /// TH_TAP_EVERY=seconds taps on a timer.
    pub tap_every: Option<f32>,
    /// TH_SECONDS=n quits after n seconds.
    pub quit_after: Option<f32>,
    /// TH_DEBUG logs every frame.
    pub debug: bool,
    /// TH_ACTIVATE=1 turns the pointer on at launch.
    pub activate: bool,
    /// TH_SHOW=toolbar|toolbar-untrusted|ink|tooltip|hint|welcome|welcome-untrusted opens that
    /// window at launch, for screenshots (the toolbar ones turn the pointer on).
    pub show: Option<String>,
}

impl TestHooks {
    pub fn from_env() -> Self {
        let env = |key: &str| std::env::var(key).ok();
        TestHooks {
            fake_mouse: env("TH_FAKE_MOUSE").and_then(|v| {
                let (x, y) = v.split_once(',')?;
                Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
            }),
            tap_every: env("TH_TAP_EVERY").and_then(|v| v.parse().ok()),
            quit_after: env("TH_SECONDS").and_then(|v| v.parse().ok()),
            debug: std::env::var_os("TH_DEBUG").is_some(),
            activate: env("TH_ACTIVATE").is_some_and(|v| v == "1" || v == "true"),
            show: env("TH_SHOW").filter(|v| !v.is_empty()),
        }
    }

    /// Any hook for unattended runs is set (the welcome card then stays away unless
    /// TH_SHOW asks for it).
    pub fn unattended(&self) -> bool {
        self.fake_mouse.is_some() || self.tap_every.is_some() || self.quit_after.is_some() || self.activate
    }
}

pub fn log(message: &str) {
    eprintln!("tinyhand: {message}");
}

pub struct Controller {
    pub settings: Settings,
    pub custom: CustomHands,
    pub hooks: TestHooks,
    pub input: Input,
    pub(crate) window_kind: Box<dyn Fn() -> WindowKind>,
    tx: UnboundedSender<Msg>,
    tray: Option<Tray>,
    /// Sparkle, in a release build on macOS (Updater.swift); `None` everywhere else.
    pub updater: Option<native::updater::Updater>,
    hotkeys: Option<Hotkeys>,
    tap: Option<InputTap>,
    cursor: CursorHider,
    sounds: TapSounds,
    window: Option<WindowHandle<OverlayView>>,
    /// The display the overlay covers, in global points (macOS; `None` elsewhere).
    pub display: Option<native::Display>,
    moving: bool,
    pub active: bool,
    pub click_through: bool,
    own_menu_open: bool,
    shift: ShiftTaps,
    pub mods: Mods,
    pub interact_pressing: bool,
    pub guard_cursor_until: Instant,
    /// The real cursor is showing and the hand is hidden (⌥ held, our menu open).
    pub normal_cursor: bool,
    esc_registered: bool,
    last_lock_check: Instant,
    pub scene: Scene,
    pub toolbar: Toolbar,
    pub welcome: Welcome,
    /// Open menus of any app (macOS), rescanned often while a menu is likely.
    menu_rects: Vec<input::Rect>,
    last_menu_scan: Instant,
    /// A press on the menu bar: a menu is opening, though not on screen yet.
    menu_opening_until: Instant,
    last_button_event: Instant,
    menu_hover: bool,
}

impl Controller {
    pub fn new(
        settings: Settings,
        input: Input,
        window_kind: Box<dyn Fn() -> WindowKind>,
        tx: UnboundedSender<Msg>,
    ) -> Controller {
        let custom = CustomHands::load_default();
        let sounds = TapSounds::new(assets::sound_dirs());
        sounds.preload(settings.tap_sound.id());
        let style_id = settings.style_existing(custom.hands());
        let scene = Scene::new(&style_id, &custom);
        Controller {
            settings,
            custom,
            hooks: TestHooks::from_env(),
            input,
            window_kind,
            tx,
            tray: None,
            updater: None,
            hotkeys: None,
            tap: None,
            cursor: CursorHider::new(),
            sounds,
            window: None,
            display: None,
            moving: false,
            active: false,
            click_through: false,
            own_menu_open: false,
            shift: ShiftTaps::default(),
            mods: Mods::default(),
            interact_pressing: false,
            guard_cursor_until: Instant::now(),
            normal_cursor: false,
            esc_registered: false,
            last_lock_check: Instant::now(),
            scene,
            toolbar: Toolbar::default(),
            welcome: Welcome::default(),
            menu_rects: Vec::new(),
            last_menu_scan: Instant::now(),
            menu_opening_until: Instant::now(),
            last_button_event: Instant::now(),
            menu_hover: false,
        }
    }

    /// AppDelegate.applicationDidFinishLaunching, minus what later phases add.
    pub fn launch(&mut self) {
        native::cursor::enable_background_hiding();
        self.first_run_login();
        if native::updater::is_available() {
            match native::updater::Updater::start() {
                Ok(updater) => {
                    log(&format!("updates: Sparkle started, automatic checks {}", updater.automatically_checks()));
                    self.updater = Some(updater);
                }
                Err(e) => log(&format!("no updates: {e}")),
            }
        }
        self.setup_tray();
        self.setup_hotkeys();
        self.write_state();
    }

    fn first_run_login(&mut self) {
        if self.settings.login_item_set_up || !native::login::should_offer_first_run_setup() {
            return;
        }
        self.settings.login_item_set_up = true;
        self.save_settings();
        let result = native::login::set_launch_at_login(true);
        log(&format!("first run: open at login = {result:?}"));
    }

    fn setup_tray(&mut self) {
        let icons = match assets::tray_icons() {
            Ok(icons) => icons,
            Err(e) => return log(&format!("no tray icon: {e}")),
        };
        let tx = self.tx.clone();
        match Tray::new(icons, move |e| {
            let _ = tx.unbounded_send(Msg::Tray(e));
        }) {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => log(&format!("no tray icon: {e}")),
        }
        self.update_tray();
    }

    fn setup_hotkeys(&mut self) {
        match Hotkeys::new() {
            Ok(keys) => {
                let tx = self.tx.clone();
                keys.set_handler(move |id| {
                    let _ = tx.unbounded_send(Msg::Hotkey(id));
                });
                self.hotkeys = Some(keys);
            }
            Err(e) => return log(&format!("no global shortcuts: {e}")),
        }
        let toggle = self.settings.toggle_shortcut.to_string();
        let clear = self.settings.clear_shortcut.to_string();
        if let Some(keys) = self.hotkeys.as_mut() {
            for (id, text) in [(HotkeyId::Toggle, toggle), (HotkeyId::Clear, clear)] {
                if let Err(e) = keys.register(id, &text) {
                    log(&format!("shortcut {text}: {e}"));
                }
            }
        }
    }

    // ---- For the settings, toolbar and welcome windows ----

    /// Changes settings, saves them and applies what depends on them. Use this for every
    /// change made from a window; shortcuts go through `set_shortcut` instead.
    pub fn update_settings(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        let before_style = self.settings.pointer_style.clone();
        let before_sound = self.settings.tap_sound;
        change(&mut self.settings);
        self.settings.normalise();
        if self.settings.pointer_style != before_style {
            let id = self.settings.pointer_style.clone();
            self.scene.set_style(&id, &self.custom);
            self.settings.pointer_style = self.scene.style.id.clone();
        }
        if self.settings.tap_sound != before_sound {
            self.sounds.preload(self.settings.tap_sound.id());
        }
        if let Some(tap) = self.tap.as_ref() {
            tap.set_interact_key(self.interact_modifier());
        }
        self.save_settings();
        self.update_tray();
        self.write_state();
        self.toolbar.dirty = true;
        self.wake(cx);
        self.refresh_welcome(cx);
        cx.notify();
    }

    /// Sets the toggle or clear shortcut. On failure (taken by another app, or not
    /// valid) the old shortcut stays registered and the error says why.
    pub fn set_shortcut(&mut self, id: HotkeyId, shortcut: tinyhand_core::settings::Shortcut, cx: &mut Context<Self>) -> Result<(), String> {
        let old = match id {
            HotkeyId::Toggle => self.settings.toggle_shortcut.clone(),
            HotkeyId::Clear => self.settings.clear_shortcut.clone(),
            HotkeyId::Escape => return Err("Esc can't be changed".into()),
        };
        if let Some(keys) = self.hotkeys.as_mut() {
            keys.unregister(id);
            if let Err(e) = keys.register(id, &shortcut.to_string()) {
                let _ = keys.register(id, &old.to_string());
                return Err(e.to_string());
            }
        }
        self.update_settings(cx, |s| match id {
            HotkeyId::Toggle => s.toggle_shortcut = shortcut,
            _ => s.clear_shortcut = shortcut,
        });
        Ok(())
    }

    /// While a shortcut recorder is listening, the global shortcuts must not fire.
    pub fn suspend_hotkeys(&mut self, suspended: bool) {
        let Some(keys) = self.hotkeys.as_mut() else { return };
        if suspended {
            keys.unregister(HotkeyId::Toggle);
            keys.unregister(HotkeyId::Clear);
        } else {
            for (id, s) in [(HotkeyId::Toggle, self.settings.toggle_shortcut.to_string()), (HotkeyId::Clear, self.settings.clear_shortcut.to_string())] {
                if let Err(e) = keys.register(id, &s) {
                    log(&format!("shortcut {s}: {e}"));
                }
            }
        }
    }

    /// After custom hands were added, renamed or deleted.
    pub fn reload_custom_hands(&mut self, cx: &mut Context<Self>) {
        self.custom = CustomHands::load_default();
        self.forget_thumbnails();
        self.update_settings(cx, |_| {});
    }

    /// Plays the chosen tap sound once (sound page preview).
    pub fn preview_sound(&self, id: &str) {
        self.sounds.play(id);
    }

    pub fn sound_on(&self) -> bool {
        if self.scene.style.kind == StyleKind::Laser { self.settings.laser_sound } else { self.settings.hand_sound }
    }

    pub(crate) fn set_sound_on(&mut self, on: bool) {
        if self.scene.style.kind == StyleKind::Laser {
            self.settings.laser_sound = on;
        } else {
            self.settings.hand_sound = on;
        }
        self.save_settings();
        self.update_tray();
        self.write_state();
        self.toolbar.dirty = true;
    }

    pub(crate) fn save_settings(&self) {
        if let Err(e) = self.settings.save() {
            log(&format!("can't save settings: {e}"));
        }
    }

    pub fn play_tap_sound(&self) {
        if self.sound_on() {
            self.sounds.play(self.settings.tap_sound.id());
        }
    }

    fn update_tray(&mut self) {
        let (active, through, sound) = (self.active, self.click_through, self.sound_on());
        let trusted = input::is_trusted();
        let menu = tray_menu::menu(&self.settings, active, sound, trusted, self.updater.is_some());
        if let Some(tray) = self.tray.as_mut() {
            tray.set_active(active);
            tray.set_attention(active && through);
            tray.set_tooltip(tray_menu::tooltip(active, through));
            tray.set_menu(menu);
        }
    }

    pub fn write_state(&self) {
        let state = State::new(self.active, &self.scene.style.id, self.sound_on(), self.custom.hands());
        if let Err(e) = automation::write_state(&automation::state_path(), &state) {
            log(&format!("can't write state.json: {e}"));
        }
    }

    /// On quit: pointer off, `state.json` gone.
    pub fn shutdown(&mut self, cx: &mut Context<Self>) {
        self.deactivate("quit", cx);
        let _ = automation::remove_state(&automation::state_path());
        crate::instance::forget();
        if let Some(keys) = self.hotkeys.as_mut() {
            keys.unregister_all();
        }
    }

    pub fn handle(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Tray(TrayEvent::Clicked) => self.toggle(cx),
            Msg::Tray(TrayEvent::MenuOpened) => {
                self.own_menu_open = true;
                self.suspend_input(true);
                self.wake(cx);
            }
            Msg::Tray(TrayEvent::MenuClosed) => {
                self.own_menu_open = false;
                self.suspend_input(false);
                self.wake(cx);
            }
            Msg::Tray(TrayEvent::MenuItem(id)) => self.menu_item(&id, cx),
            Msg::Hotkey(HotkeyId::Toggle) => self.toggle(cx),
            Msg::Hotkey(HotkeyId::Clear) => self.clear_ink(cx),
            Msg::Hotkey(HotkeyId::Escape) => self.exit(cx),
            Msg::Input(event) => self.input_event(event, cx),
            Msg::Command(command) => self.command(command, cx),
        }
    }

    fn menu_item(&mut self, id: &str, cx: &mut Context<Self>) {
        match id {
            tray_menu::ALLOW => {
                self.exit(cx);
                self.show_welcome(cx);
            }
            tray_menu::TOGGLE => self.toggle(cx),
            tray_menu::CLEAR => self.clear_ink(cx),
            tray_menu::SOUND => self.set_sound_on(!self.sound_on()),
            tray_menu::SETTINGS => {
                self.exit(cx);
                let this = cx.entity();
                cx.defer(move |cx| crate::settings_window::open(this, cx));
            }
            tray_menu::UPDATES => {
                self.exit(cx);
                if let Some(updater) = &self.updater {
                    updater.check_for_updates();
                }
            }
            tray_menu::QUIT => cx.quit(),
            _ => {}
        }
    }

    pub fn command(&mut self, command: Command, cx: &mut Context<Self>) {
        log(&format!("command: {command:?}"));
        match command {
            Command::On => self.activate(cx),
            Command::Off => self.deactivate("command", cx),
            Command::Toggle => self.toggle(cx),
            Command::Clear => self.clear_ink(cx),
            Command::Style(id) => {
                if self.pick(&id, cx) {
                    self.activate(cx);
                }
            }
            Command::Sound(action) => {
                let on = match action {
                    SoundAction::On => true,
                    SoundAction::Off => false,
                    SoundAction::Toggle => !self.sound_on(),
                };
                self.set_sound_on(on);
            }
        }
    }

    /// Picks a pointer style and saves it. False if there is no such style.
    pub fn pick(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if styles::find(id, self.custom.hands()).is_none() {
            log(&format!("unknown pointer style: {id}"));
            return false;
        }
        self.scene.set_style(id, &self.custom);
        self.settings.pointer_style = self.scene.style.id.clone();
        self.save_settings();
        self.update_tray();
        self.write_state();
        self.toolbar.dirty = true;
        self.wake(cx);
        true
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.active {
            self.deactivate("toggle", cx);
        } else {
            self.activate(cx);
        }
    }

    pub fn clear_ink(&mut self, cx: &mut Context<Self>) {
        self.scene.ink.clear();
        self.wake(cx);
    }

    /// Esc and the menu: clear the drawing and turn off.
    pub fn exit(&mut self, cx: &mut Context<Self>) {
        self.scene.ink.clear();
        self.deactivate("exit", cx);
    }

    /// Whether this run reads the mouse natively and may tap it. Never with TH_FAKE_MOUSE,
    /// so an unattended run can't swallow the user's clicks.
    pub(crate) fn taps_input(&self) -> bool {
        self.input == Input::Watch && self.hooks.fake_mouse.is_none()
    }

    pub fn activate(&mut self, cx: &mut Context<Self>) {
        if self.active {
            return;
        }
        if self.taps_input() && cfg!(target_os = "macos") && !input::is_trusted() {
            log("activate skipped: not trusted for Accessibility");
            self.update_tray();
            self.show_welcome(cx);
            return;
        }
        self.active = true;
        log("pointer on");
        self.sounds.warm();
        self.cursor.hide();
        self.normal_cursor = false;
        self.scene.wake();
        self.open_window(cx);

        if self.taps_input() {
            if self.tap.is_none() {
                let tx = self.tx.clone();
                match InputTap::start(move |e| {
                    let _ = tx.unbounded_send(Msg::Input(e));
                }) {
                    Ok(tap) => self.tap = Some(tap),
                    Err(e) => log(&format!("no input tap, clicks reach apps: {e}")),
                }
            }
            if let Some(tap) = &self.tap {
                tap.set_interact_key(self.interact_modifier());
                tap.set_click_through(false);
                tap.set_enabled(true);
            }
        }
        self.click_through = false;
        self.shift.reset();
        self.set_esc(true);
        self.toolbar_activated();
        self.update_tray();
        self.write_state();
    }

    pub fn deactivate(&mut self, reason: &str, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        self.active = false;
        log(&format!("pointer off: {reason}"));
        if let Some(tap) = &self.tap {
            tap.set_enabled(false);
            tap.set_click_through(false);
            tap.set_pass_through(Vec::new());
        }
        self.click_through = false;
        self.shift.reset();
        self.interact_pressing = false;
        self.scene.release();
        self.scene.ink.clear();
        self.close_window(cx);
        self.toolbar_deactivated(cx);
        self.menu_rects.clear();
        self.menu_hover = false;
        self.normal_cursor = false;
        self.own_menu_open = false;
        self.set_esc(false);
        self.cursor.show();
        self.update_tray();
        self.write_state();
    }

    fn suspend_input(&mut self, suspended: bool) {
        if let (true, Some(tap)) = (self.active, &self.tap) {
            tap.set_enabled(!suspended);
        }
        self.set_esc(!suspended);
    }

    fn interact_modifier(&self) -> Option<Modifier> {
        use tinyhand_core::settings::InteractKey;
        match self.settings.interact_key {
            InteractKey::Option => Some(Modifier::Alt),
            InteractKey::Command => Some(Modifier::Command),
            InteractKey::Control => Some(Modifier::Control),
            InteractKey::Off => None,
        }
    }

    /// Whether the interact key is held right now.
    pub fn interact_held(&self) -> bool {
        match self.interact_modifier() {
            Some(Modifier::Alt) => self.mods.alt,
            Some(Modifier::Command) => self.mods.command,
            Some(Modifier::Control) => self.mods.control,
            None => false,
        }
    }

    /// ⌘ draws an arrowhead (Ctrl on Windows and Linux, where ⊞/Super belong to the OS).
    pub fn wants_arrow(&self) -> bool {
        if cfg!(target_os = "macos") { self.mods.command } else { self.mods.control }
    }

    /// Esc is a global shortcut only while the pointer is on, no menu of ours is open and
    /// clicks aren't going through.
    pub fn set_esc(&mut self, wanted: bool) {
        let on = wanted && self.active && !self.own_menu_open && !self.click_through;
        if on == self.esc_registered {
            return;
        }
        let Some(keys) = self.hotkeys.as_mut() else { return };
        if on {
            match keys.register(HotkeyId::Escape, "escape") {
                Ok(()) => self.esc_registered = true,
                Err(e) => log(&format!("Esc shortcut: {e}")),
            }
        } else {
            keys.unregister(HotkeyId::Escape);
            self.esc_registered = false;
        }
    }

    fn set_click_through(&mut self, on: bool, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        self.click_through = on;
        if let Some(tap) = &self.tap {
            tap.set_click_through(on);
        }
        log(if on { "click-through on" } else { "click-through off" });
        if on {
            self.scene.release();
        }
        self.click_through_hint(on);
        self.set_esc(true);
        self.update_tray();
        self.wake(cx);
    }

    /// The tap's global coordinates in the overlay's.
    fn to_local(&self, x: f64, y: f64) -> (f32, f32) {
        match self.display {
            Some(d) => ((x - d.x) as f32, (y - d.y) as f32),
            // Windows reports physical pixels of the primary display.
            None if cfg!(target_os = "windows") => {
                let s = self.scene.scale.max(1.0) as f64;
                ((x / s) as f32, (y / s) as f32)
            }
            None => (x as f32, y as f32),
        }
    }

    fn input_event(&mut self, event: InputEvent, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        if !matches!(event, InputEvent::Modifiers { .. } | InputEvent::LeftDragged { .. }) {
            self.last_button_event = Instant::now();
        }
        match event {
            InputEvent::MouseDown { x, y, .. } => {
                let p = self.to_local(x, y);
                self.press(p);
            }
            InputEvent::MouseUp { .. } => self.scene.release(),
            InputEvent::LeftDragged { x, y } => {
                let p = self.to_local(x, y);
                self.drag(p);
            }
            InputEvent::InteractDown => {
                self.scene.arm.press_now();
                self.scene.held = true;
                self.interact_pressing = true;
            }
            InputEvent::InteractUp => {
                self.scene.held = false;
                self.interact_pressing = false;
                self.guard_cursor_until = Instant::now() + Duration::from_secs(2);
                self.cursor.reassert(false, true);
            }
            InputEvent::PassThroughPress { x, y } => {
                let bar = self.display.and_then(|d| native::menus::menu_bar_rect(&d));
                if bar.is_some_and(|b| b.contains(x, y)) {
                    self.menu_opening_until = Instant::now() + Duration::from_secs(1);
                }
            }
            InputEvent::ScreenLocked => self.deactivate("click on locked screen", cx),
            InputEvent::Modifiers { shift, control, alt, command } => {
                self.modifiers_changed(Mods { shift, control, alt, command }, cx);
            }
        }
        self.wake(cx);
    }

    pub fn modifiers_changed(&mut self, mods: Mods, cx: &mut Context<Self>) {
        self.mods = mods;
        if self.scene.ink.is_drawing() {
            let arrow = self.wants_arrow();
            self.scene.ink.set_arrow(arrow);
        }
        if !self.settings.double_shift_click_through || self.input != Input::Watch {
            return;
        }
        if self.shift.changed(mods, Instant::now(), input::input_counter()) {
            self.set_click_through(!self.click_through, cx);
        }
    }

    /// A swallowed press (`inputTap.onMouseDown`): press the hand, play the sound, and
    /// remember where a drag would start.
    pub fn press(&mut self, at: (f32, f32)) {
        self.toolbar.hide_hint();
        self.scene.press(at);
        self.play_tap_sound();
    }

    /// A drag 4 pt past the press starts a stroke; ⌘ adds an arrowhead.
    pub fn drag(&mut self, to: (f32, f32)) {
        let colour = self.settings.ink_choice.resolved(self.scene.style.ink);
        let arrow = self.wants_arrow();
        self.scene.drag(to, colour, arrow);
    }

    /// Redraw the overlay soon (after events from other threads).
    pub fn wake(&mut self, cx: &mut Context<Self>) {
        self.scene.wake();
        self.toolbar.dirty = true;
        if let Some(handle) = self.window {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.refresh());
            });
        }
    }

    /// Polls for a locked screen about once a second (macOS; the tap only reports a click
    /// on a locked screen).
    pub fn check_lock(&mut self, cx: &mut Context<Self>) {
        if self.last_lock_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_lock_check = Instant::now();
        if input::screen_locked() {
            self.deactivate("screen locked", cx);
        }
    }

    /// Shows or hides the real cursor and sets the tap's pass-through rects
    /// (`OverlayController.updateZones`; the toolbar half is `update_toolbar`).
    pub fn update_zones(&mut self, global_mouse: Option<(f64, f64)>) {
        let now = Instant::now();
        let drawing = self.scene.ink.is_drawing();
        let interacting = self.interact_held() || self.interact_pressing;

        // Menus: the menu bar, and any app's open menu. Scanning the window list costs a
        // little, so it is done often only while a menu is likely.
        let menu_bar = self.display.and_then(|d| native::menus::menu_bar_rect(&d));
        let mouse_in = |r: &input::Rect| global_mouse.is_some_and(|(x, y)| r.contains(x, y));
        let near_bar = menu_bar.is_some_and(|b| mouse_in(&input::Rect::new(b.x, b.y - 40.0, b.w, b.h + 80.0)));
        let likely = !self.menu_rects.is_empty()
            || self.own_menu_open
            || now < self.menu_opening_until
            || now.duration_since(self.last_button_event) < Duration::from_secs(2)
            || near_bar;
        let every = Duration::from_millis(if likely { 50 } else { 250 });
        if cfg!(target_os = "macos") && now.duration_since(self.last_menu_scan) > every {
            self.last_menu_scan = now;
            self.menu_rects = native::menus::open_menu_rects();
        }
        let over_menu = menu_bar.is_some_and(|b| mouse_in(&b))
            || self.menu_rects.iter().any(mouse_in)
            || now < self.menu_opening_until;
        self.menu_hover = over_menu;

        let normal = !drawing
            && !self.settings.interact_keeps_hand
            && ((interacting && !self.click_through) || over_menu || self.own_menu_open);
        let no_menus = self.menu_rects.is_empty();
        self.set_esc(no_menus);
        if normal != self.normal_cursor {
            self.normal_cursor = normal;
            if normal {
                self.cursor.show();
            } else {
                self.cursor.hide();
            }
        }
        if !normal {
            let over_dock = global_mouse.is_some_and(|(x, y)| native::cursor::dock_contains(x, y));
            let urgent = interacting || self.menu_hover || now < self.guard_cursor_until;
            self.cursor.reassert(over_dock, urgent);
        }
        if let Some(tap) = &self.tap {
            let mut rects: Vec<input::Rect> = self
                .tray
                .as_ref()
                .and_then(|t| t.frame())
                .map(|r| input::Rect::new(r.x, r.y, r.w, r.h))
                .into_iter()
                .collect();
            rects.extend(menu_bar);
            rects.extend(self.menu_rects.iter().copied());
            rects.extend(self.toolbar_pass_through());
            tap.set_pass_through(rects);
        }
    }

    pub fn has_tap(&self) -> bool {
        self.tap.is_some()
    }

    pub fn cursor_settling(&self) -> bool {
        self.cursor.is_settling()
    }

    /// The display under the mouse moved: move the overlay there (macOS).
    pub fn follow_display(&mut self, display: native::Display, cx: &mut Context<Self>) {
        if self.moving || self.display.is_some_and(|d| d.id == display.id) {
            return;
        }
        self.moving = true;
        // Swift: show the toolbar for a moment on the new display.
        self.toolbar.reveal_until = Some(Instant::now() + Duration::from_millis(1500));
        self.scene.release();
        self.scene.ink.clear();
        let this = cx.entity();
        cx.defer(move |cx| {
            this.update(cx, |c, cx| {
                c.close_window(cx);
                c.open_window(cx);
                c.moving = false;
            });
        });
    }

    fn close_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.window.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    /// Opens the overlay over the display under the mouse. Deferred, because GPUI draws a
    /// new window straight away and the drawing reads this controller.
    fn open_window(&mut self, cx: &mut Context<Self>) {
        let this = cx.entity();
        // macOS: the display under the real mouse (or the main one for a pinned test mouse).
        let display = if self.input == Input::Watch {
            let at = if self.hooks.fake_mouse.is_some() { Some((0.0, 0.0)) } else { native::cursor_global() };
            at.and_then(|(x, y)| native::display_at(x, y))
        } else {
            None
        };
        self.display = display;
        let kind = (self.window_kind)();
        let input = self.input;
        cx.defer(move |cx: &mut App| {
            let (display_id, bounds) = match display {
                Some(d) => (
                    Some(DisplayId::from(d.id)),
                    Bounds::new(point(px(0.0), px(0.0)), size(px(d.w as f32), px(d.h as f32))),
                ),
                None => {
                    // Wayland may not list displays before the first window exists; the
                    // overlay takes its real size from the window once it is drawn.
                    let primary = cx.primary_display();
                    let bounds = primary.as_ref().map(|d| d.bounds()).unwrap_or_else(|| {
                        Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)))
                    });
                    (primary.map(|d| d.id()), bounds)
                }
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                focus: false,
                show: true,
                kind,
                // The overlay never has focus; GPUI would otherwise hold it to ~30 fps.
                inactive_frame_interval: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            };
            let view_of = this.clone();
            let opened = cx.open_window(options, move |window, cx| {
                if let Err(e) = native::make_overlay(window, input == Input::Watch) {
                    log(&format!("overlay setup: {e}"));
                }
                let focus = cx.focus_handle();
                cx.new(|_| OverlayView::new(view_of, focus))
            });
            this.update(cx, |c, cx| match opened {
                // Turned off again before the window opened.
                Ok(handle) if !c.active => {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                }
                Ok(handle) => {
                    // Again now that GPUI has placed and shown it (macOS moved it below
                    // the menu bar).
                    let _ = handle.update(cx, |_, window, _| native::make_overlay(window, input == Input::Watch));
                    c.window = Some(handle);
                }
                Err(e) => {
                    log(&format!("can't open the overlay: {e}"));
                    c.deactivate("no overlay window", cx);
                }
            });
        });
    }
}
