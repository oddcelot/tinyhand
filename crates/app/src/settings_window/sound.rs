//! Sound: the tap sound tiles and the laser dot switch.

use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use tinyhand_core::settings::TapSound;

use super::ui::{Theme, card, heading, row, switch, text};
use super::{SettingsWindow, icons};

fn tile(
    t: &Theme,
    id: &'static str,
    title: &'static str,
    symbol: &'static str,
    selected: bool,
    width: f32,
    cx: &mut Context<SettingsWindow>,
    action: impl Fn(&mut SettingsWindow, &mut Context<SettingsWindow>) + 'static,
) -> impl IntoElement {
    let hover = t.fg(0.07);
    let mut d = div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.))
        .w(px(width))
        .py(px(14.))
        .rounded(px(12.))
        .border(px(1.5))
        .cursor_pointer()
        .child(icons::icon(symbol, 20., if selected { t.accent() } else { t.secondary() }))
        .child(
            text(title, 12., if selected { t.label() } else { t.secondary() })
                .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::NORMAL }),
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| action(this, cx)));
    if selected {
        d = d.bg(t.accent().opacity(0.12)).border_color(t.accent().opacity(0.7));
    } else {
        d = d.bg(t.fg(0.035)).border_color(gpui::transparent_black()).hover(move |s| s.bg(hover));
    }
    d
}

pub fn render(this: &mut SettingsWindow, t: &Theme, window: &mut Window, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let s = this.settings(cx).clone();
    let on = s.hand_sound;
    // LazyVGrid(.adaptive(minimum: 104, maximum: 140), spacing: 8) over the page width.
    let page = f32::from(window.viewport_size().width) - 210. - 56.;
    let cols = ((page + 8.) / 112.).floor().max(1.);
    let width = ((page - 8. * (cols - 1.)) / cols).min(140.).floor();
    let mut tiles = vec![tile(t, "sound-off", "Off", icons::SPEAKER_SLASH, !on, width, cx, |this, cx| this.change(cx, |s| s.hand_sound = false))
        .into_any_element()];
    for choice in TapSound::ALL {
        tiles.push(
            tile(t, choice.id(), choice.title(), icons::WAVEFORM, on && s.tap_sound == choice, width, cx, move |this, cx| {
                this.change(cx, |s| {
                    s.tap_sound = choice;
                    s.hand_sound = true;
                });
                this.controller.read(cx).preview_sound(choice.id());
            })
            .into_any_element(),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(heading(t, "Tap sound").px(px(4.)))
                .child(div().flex().flex_wrap().gap(px(8.)).children(tiles))
                .child(text("Click a sound to hear it. It plays each time the hand taps.", 11., t.secondary()).px(px(4.))),
        )
        .child(
            card(
                t,
                None,
                vec![row(
                    t,
                    "Laser dot taps too",
                    Some("Off by default, for quiet pointing.".into()),
                    switch(t, "laser-sound", s.laser_sound, on, cx, |this, on, _, cx| this.change(cx, |s| s.laser_sound = on)),
                )],
            )
            .opacity(if on { 1. } else { 0.5 }),
        )
        .into_any_element()
}
