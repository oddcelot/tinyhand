//! Drawing: ink colour and how long lines stay.

use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, img,
    prelude::FluentBuilder, px,
};
use tinyhand_core::settings::InkChoice;

use super::ui::{MenuChoice, Theme, card, popup, rgb_hsla, row, tip};
use super::{SettingsWindow, icons, mac};

const FADE_CHOICES: [(&str, f64); 4] = [("After 3 seconds", 3.0), ("After 6 seconds", 6.0), ("After 10 seconds", 10.0), ("Keep until cleared", 0.0)];

fn swatch(t: &Theme, choice: InkChoice, selected: bool, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let dot = match choice.colour() {
        Some(c) => div()
            .size(px(20.))
            .rounded_full()
            .bg(rgb_hsla(c))
            .border_1()
            .border_color(t.fg(if choice == InkChoice::Black { 0.35 } else { 0.15 }))
            .into_any_element(),
        None => img(icons::colour_svg(icons::rainbow_svg())).size(px(20.)).rounded_full().into_any_element(),
    };
    div()
        .id(choice.title())
        .p(px(1.))
        .rounded_full()
        .border_2()
        .border_color(if selected { t.accent() } else { gpui::transparent_black() })
        .cursor_pointer()
        .child(div().size(px(20.)).child(dot))
        .when(!selected, |d| d.hover(|s| s.opacity(0.85)))
        .tooltip(tip(choice.title()))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.change(cx, |s| s.ink_choice = choice)))
        .into_any_element()
}

pub fn render(this: &mut SettingsWindow, t: &Theme, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let s = this.settings(cx).clone();
    let ink_detail = if s.ink_choice == InkChoice::Match {
        "Matches each pointer.".to_string()
    } else {
        format!("All pointers draw in {}.", s.ink_choice.title().to_lowercase())
    };
    let swatches = div()
        .flex()
        .items_center()
        .gap(px(4.))
        .children(InkChoice::ALL.into_iter().map(|c| swatch(t, c, s.ink_choice == c, cx)).collect::<Vec<_>>());
    let fades = FADE_CHOICES
        .into_iter()
        .map(|(title, secs)| MenuChoice::new(title, (s.fade_seconds - secs).abs() < 0.01, move |this, _, cx| this.change(cx, |s| s.fade_seconds = secs)))
        .collect();
    card(
        t,
        Some("Ink"),
        vec![
            row(t, "Ink colour", Some(ink_detail), swatches),
            row(
                t,
                "Lines fade",
                Some(format!("Clear them any time with {}.", s.clear_shortcut.display(mac()))),
                popup(t, this, "fade", fades, cx),
            ),
        ],
    )
    .into_any_element()
}
