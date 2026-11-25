//! About: version, links and "Reset all settings…".

use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, PromptLevel,
    StatefulInteractiveElement, Styled, Window, div, img, px,
};
use tinyhand_core::settings::Shortcut;
use tinyhand_platform::hotkeys::HotkeyId;

use super::ui::{Theme, card, text, text_button};
use super::{SettingsWindow, app_icon, icons, mac};

const LINKS: [(&str, &str, &str); 4] = [
    ("Website", "tinyhand.app", "https://tinyhand.app"),
    ("Source code", "github.com/nilbuild/tinyhand", "https://github.com/nilbuild/tinyhand"),
    ("Made by", "@nilbuild on X", "https://x.com/nilbuild"),
    ("License", "MIT", "https://github.com/nilbuild/tinyhand/blob/main/LICENSE"),
];

fn link_row(t: &Theme, title: &'static str, detail: &'static str, url: &'static str) -> AnyElement {
    let hover = t.fg(0.04);
    div()
        .id(title)
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(14.))
        .min_h(px(44.))
        .rounded(px(12.))
        .hover(move |s| s.bg(hover))
        .cursor_pointer()
        .child(div().flex_1().child(text(title, 13., t.label())))
        .child(text(detail, 12., t.secondary()))
        .child(icons::icon(icons::ARROW_UP_RIGHT, 10., t.secondary()))
        .on_click(move |_: &ClickEvent, _, cx| cx.open_url(url))
        .into_any_element()
}

pub fn render(_this: &mut SettingsWindow, t: &Theme, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let tagline = if mac() { "A tiny hand for your Mac. Free and open source." } else { "A tiny hand for your screen. Free and open source." };
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(16.))
                .child(img(app_icon()).size(px(72.)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(text("TinyHand", 18., t.label()).font_weight(FontWeight::BOLD))
                        .child(text(format!("Version {}", env!("CARGO_PKG_VERSION")), 12., t.secondary()))
                        .child(text(tagline, 12., t.secondary())),
                ),
        )
        .child(card(t, None, LINKS.iter().map(|&(a, b, c)| link_row(t, a, b, c)).collect()))
        .child(div().flex().justify_end().child(text_button(t, "reset-all", "Reset all settings…", cx, reset_all)))
        .into_any_element()
}

/// The confirm, then every setting back to its default and the shortcuts re-registered.
fn reset_all(this: &mut SettingsWindow, window: &mut Window, cx: &mut Context<SettingsWindow>) {
    let answer = window.prompt(
        PromptLevel::Warning,
        "Reset all settings?",
        Some(if mac() {
            "Shortcuts, toolbar pointers, size, shadow, drawing and sound go back to how they were at first. Open at login and Accessibility stay as they are."
        } else {
            "Shortcuts, toolbar pointers, size, shadow, drawing and sound go back to how they were at first. Open at login stays as it is."
        }),
        &["Reset", "Cancel"],
        cx,
    );
    let _ = this;
    cx.spawn(async move |this, cx| {
        if answer.await != Ok(0) {
            return;
        }
        let _ = this.update(cx, |this, cx| {
            this.controller.update(cx, |c, cx| {
                c.update_settings(cx, |s| s.reset_all());
                for (id, s) in [(HotkeyId::Toggle, Shortcut::toggle_default()), (HotkeyId::Clear, Shortcut::clear_default())] {
                    if let Err(e) = c.set_shortcut(id, s, cx) {
                        crate::controller::log(&format!("reset shortcut: {e}"));
                    }
                }
            });
            this.shortcut_error = None;
            this.pin_message = None;
            cx.notify();
        });
    })
    .detach();
}
