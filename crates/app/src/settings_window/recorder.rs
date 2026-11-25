//! The shortcut recorder (ShortcutRecorder in SettingsWindow.swift): click it, press the
//! new shortcut. It needs ⌃, ⌥ or ⌘ (Ctrl, Alt or Super off the Mac); Esc or a click
//! anywhere else cancels. The global shortcuts are switched off while it listens, so
//! pressing the current one doesn't toggle the pointer.

use gpui::{
    AnyElement, ClickEvent, Context, FocusHandle, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ModifiersChangedEvent, Modifiers, ParentElement, StatefulInteractiveElement, Styled, Window, div, hsla,
    prelude::FluentBuilder, px,
};
use tinyhand_core::settings::Shortcut;
use tinyhand_platform::hotkeys::HotkeyId;

use super::ui::{Theme, text, tip};
use super::{SettingsWindow, mac};

pub struct Recorder {
    pub recording: Option<HotkeyId>,
    /// What the recorder shows while listening ("Press keys", the held modifiers, or
    /// the "Add ⌃, ⌥ or ⌘" hint), and whether it is dimmed text rather than key caps.
    label: Option<(String, bool)>,
    focus: FocusHandle,
}

impl Recorder {
    pub fn new(cx: &mut Context<SettingsWindow>) -> Recorder {
        Recorder { recording: None, label: None, focus: cx.focus_handle() }
    }
}

fn press_keys() -> (String, bool) {
    ("Press keys".into(), true)
}

fn need_modifier() -> &'static str {
    if mac() { "Add ⌃, ⌥ or ⌘" } else { "Add Ctrl, Alt or Super" }
}

/// The held modifiers, as the recorder shows them while listening.
fn modifier_text(m: &Modifiers) -> String {
    let names: [(bool, &str, &str); 4] = [(m.control, "⌃", "Ctrl"), (m.alt, "⌥", "Alt"), (m.shift, "⇧", "Shift"), (m.platform, "⌘", "Super")];
    let held = names.iter().filter(|n| n.0);
    if mac() { held.map(|n| n.1).collect() } else { held.map(|n| n.2).collect::<Vec<_>>().join("+") }
}

/// GPUI's key name as a shortcut key, or `None` for keys a shortcut can't use.
fn shortcut_key(key: &str) -> Option<String> {
    const NAMED: [&str; 14] = ["space", "enter", "tab", "backspace", "delete", "escape", "left", "right", "up", "down", "home", "end", "pageup", "pagedown"];
    if NAMED.contains(&key) {
        return Some(key.to_string());
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=12).contains(&n).then(|| key.to_string());
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_whitespace() => Some(c.to_lowercase().collect()),
        _ => None,
    }
}

/// Splits a shortcut's text into key caps: each leading modifier symbol, then the key
/// (macOS), or the `+`-separated parts elsewhere. `None` means show it as plain text.
fn caps(text: &str) -> Option<Vec<String>> {
    if !mac() {
        return Some(text.split('+').filter(|p| !p.is_empty()).map(str::to_string).collect());
    }
    let mut caps: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next().filter(|c| "⌃⌥⇧⌘".contains(*c)) {
        caps.push(c.to_string());
        rest = &rest[c.len_utf8()..];
    }
    if caps.is_empty() && rest.chars().count() > 4 {
        return None;
    }
    if !rest.is_empty() {
        caps.push(rest.to_string());
    }
    Some(caps)
}

fn keycap(t: &Theme, key: String) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(20.))
        .min_w(px(22.))
        .px(px(6.))
        .rounded(px(5.))
        .bg(t.fg(0.1))
        .border(px(0.5))
        .border_color(t.fg(0.18))
        .shadow(vec![gpui::BoxShadow::new(px(0.), px(1.), hsla(0., 0., 0., 0.15))])
        .child(text(key, 12., t.label()).font_weight(FontWeight::MEDIUM))
        .into_any_element()
}

/// The recorder for one shortcut.
pub fn field(this: &SettingsWindow, t: &Theme, id: HotkeyId, current: &Shortcut, cx: &mut Context<SettingsWindow>) -> impl IntoElement {
    let recording = this.recorder.recording == Some(id);
    let (label, dim) = match (&this.recorder.label, recording) {
        (Some(l), true) => l.clone(),
        (None, true) => press_keys(),
        _ => (current.display(mac()), false),
    };
    let content: Vec<AnyElement> = match caps(&label).filter(|_| !dim) {
        Some(parts) => parts.into_iter().map(|p| keycap(t, p)).collect(),
        None => vec![text(label, 12., t.secondary()).font_weight(FontWeight::MEDIUM).into_any_element()],
    };
    let name = if id == HotkeyId::Toggle { "record-toggle" } else { "record-clear" };
    div()
        .id(name)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .min_w(px(132.))
        .h(px(30.))
        .px(px(6.))
        .rounded(px(7.))
        .bg(t.fg(if recording { 0.09 } else { 0.05 }))
        .cursor_pointer()
        .children(content)
        .tooltip(tip("Click, then press the new shortcut. Esc cancels."))
        .when(recording, |d| {
            d.track_focus(&this.recorder.focus)
                .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| {
                    cx.stop_propagation();
                    key_down(this, id, e, window, cx);
                }))
                .on_modifiers_changed(cx.listener(|this, e: &ModifiersChangedEvent, _, cx| {
                    let held = modifier_text(&e.modifiers);
                    this.recorder.label = Some(if held.is_empty() { press_keys() } else { (held, false) });
                    cx.notify();
                }))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| stop(this, cx)))
        })
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| start(this, id, window, cx)))
}

fn start(this: &mut SettingsWindow, id: HotkeyId, window: &mut Window, cx: &mut Context<SettingsWindow>) {
    if this.recorder.recording == Some(id) {
        return;
    }
    if this.recorder.recording.is_none() {
        this.controller.update(cx, |c, _| c.suspend_hotkeys(true));
    }
    this.recorder.recording = Some(id);
    this.recorder.label = None;
    window.focus(&this.recorder.focus, cx);
    cx.notify();
}

/// Stops listening and turns the global shortcuts back on.
pub fn stop(this: &mut SettingsWindow, cx: &mut Context<SettingsWindow>) {
    if this.recorder.recording.take().is_some() {
        this.controller.update(cx, |c, _| c.suspend_hotkeys(false));
    }
    this.recorder.label = None;
    cx.notify();
}

fn key_down(this: &mut SettingsWindow, id: HotkeyId, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<SettingsWindow>) {
    let k = &e.keystroke;
    if k.key == "escape" {
        return stop(this, cx);
    }
    let m = k.modifiers;
    if !(m.control || m.alt || m.platform) {
        this.recorder.label = Some((need_modifier().into(), true));
        cx.notify();
        return;
    }
    let Some(key) = shortcut_key(&k.key) else { return };
    let shortcut = Shortcut::new(m.control, m.alt, m.shift, m.platform, &key);
    // Turn the old shortcuts back on first: set_shortcut swaps a registered one.
    stop(this, cx);
    let display = shortcut.display(mac());
    let result = this.controller.update(cx, |c, cx| c.set_shortcut(id, shortcut, cx));
    this.shortcut_error = result.err().map(|_| format!("{display} is already used by another app. Pick a different shortcut."));
    cx.notify();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_shortcut_keys() {
        assert_eq!(shortcut_key("h").as_deref(), Some("h"));
        assert_eq!(shortcut_key("H").as_deref(), Some("h"));
        assert_eq!(shortcut_key("pageup").as_deref(), Some("pageup"));
        assert_eq!(shortcut_key("f12").as_deref(), Some("f12"));
        assert_eq!(shortcut_key("f13"), None);
        assert_eq!(shortcut_key("shift"), None);
    }

    #[test]
    fn caps_split_like_the_swift_recorder() {
        if mac() {
            assert_eq!(caps("⌃⌥H"), Some(vec!["⌃".into(), "⌥".into(), "H".into()]));
            assert_eq!(caps("Press keys"), None);
            assert_eq!(caps("⌘Space"), Some(vec!["⌘".into(), "Space".into()]));
        } else {
            assert_eq!(caps("Ctrl+Alt+H"), Some(vec!["Ctrl".into(), "Alt".into(), "H".into()]));
        }
    }
}
