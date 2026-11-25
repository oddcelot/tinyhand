//! Appearance: pointer size and shadow.

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};
use tinyhand_core::settings::POINTER_SIZE_RANGE;

use super::SettingsWindow;
use super::icons;
use super::ui::{Theme, card, row, slider, switch};

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
        ],
    )
    .into_any_element()
}
