//! Appearance: pointer size, shadow and tap effect.

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};
use tinyhand_core::particles::TapEffect;
use tinyhand_core::settings::{POINTER_SIZE_RANGE, tap_effect_title};

use super::SettingsWindow;
use super::icons;
use super::ui::{MenuChoice, Theme, card, popup, row, slider, switch};

pub fn render(this: &mut SettingsWindow, t: &Theme, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let s = this.settings(cx).clone();
    let (lo, hi) = (*POINTER_SIZE_RANGE.start(), *POINTER_SIZE_RANGE.end());
    let size = div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(icons::icon(icons::HAND, 11., t.secondary()))
        .child(slider(t, this, 180., (s.pointer_size - lo) / (hi - lo), cx))
        .child(icons::icon(icons::HAND, 17., t.secondary()));
    let effects = [TapEffect::Off, TapEffect::FairyDust, TapEffect::FairyDustSubtle]
        .into_iter()
        .map(|e| MenuChoice::new(tap_effect_title(e), s.tap_effect == e, move |this, _, cx| this.change(cx, |s| s.tap_effect = e)))
        .collect();
    card(
        t,
        Some("Pointer"),
        vec![
            row(t, "Size", None, size),
            row(
                t,
                "Shadow under the pointer",
                None,
                switch(t, "shadow", s.show_shadow, true, cx, |this, on, _, cx| this.change(cx, |s| s.show_shadow = on)),
            ),
            row(t, "Tap effect", Some("Plays where the pointer taps.".into()), popup(t, this, "tap-effect", effects, cx)),
        ],
    )
    .into_any_element()
}
