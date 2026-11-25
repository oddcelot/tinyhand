//! General: Accessibility (macOS, when not yet allowed), shortcuts, clicking for real,
//! and startup.

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};
use tinyhand_core::settings::InteractKey;
use tinyhand_platform::hotkeys::HotkeyId;

use super::ui::{MenuChoice, Theme, card, pill_button, popup, row, switch, text};
use super::{SettingsWindow, mac, recorder};

const ACCESSIBILITY_URL: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// The interact key's name in the menu: Swift's titles on the Mac, the PC names elsewhere.
fn interact_title(key: InteractKey) -> &'static str {
    if mac() {
        return key.title();
    }
    match key {
        InteractKey::Option => "Alt",
        InteractKey::Command if cfg!(windows) => "Windows key",
        InteractKey::Command => "Super",
        InteractKey::Control => "Ctrl",
        InteractKey::Off => "Off",
    }
}

pub fn render(this: &mut SettingsWindow, t: &Theme, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let s = this.settings(cx).clone();
    let mut blocks: Vec<AnyElement> = Vec::new();

    if mac() && !tinyhand_platform::input::is_trusted() {
        blocks.push(
            card(
                t,
                Some("Permission"),
                vec![row(
                    t,
                    "Accessibility",
                    Some("Needed so clicks become taps and you can draw.".into()),
                    pill_button(t, "open-accessibility", "Open Settings", cx, |_, _, cx| cx.open_url(ACCESSIBILITY_URL)),
                )],
            )
            .into_any_element(),
        );
    }

    blocks.push(
        card(
            t,
            Some("Shortcuts"),
            vec![
                row(t, "Turn the pointer on or off", None, recorder::field(this, t, HotkeyId::Toggle, &s.toggle_shortcut, cx)),
                row(t, "Clear the drawing", None, recorder::field(this, t, HotkeyId::Clear, &s.clear_shortcut, cx)),
            ],
        )
        .into_any_element(),
    );
    if let Some(error) = this.shortcut_error.clone() {
        blocks.push(text(error, 12., t.red()).into_any_element());
    }

    let mut clicking = vec![row(
        t,
        "Hold to use the app",
        Some("Clicks, drags and scrolls reach the app.".into()),
        popup(
            t,
            this,
            "interact-key",
            InteractKey::ALL
                .into_iter()
                .map(|k| MenuChoice::new(interact_title(k), s.interact_key == k, move |this, _, cx| this.change(cx, |s| s.interact_key = k)))
                .collect(),
            cx,
        ),
    )];
    if s.interact_key != InteractKey::Off {
        let detail = if s.interact_keeps_hand { "The hand stays, in menus too." } else { "The normal cursor comes back, in menus too." };
        clicking.push(row(
            t,
            "While holding",
            Some(detail.into()),
            popup(
                t,
                this,
                "while-holding",
                [("Show the cursor", false), ("Keep the hand", true)]
                    .into_iter()
                    .map(|(title, keep)| {
                        MenuChoice::new(title, s.interact_keeps_hand == keep, move |this, _, cx| this.change(cx, |s| s.interact_keeps_hand = keep))
                    })
                    .collect(),
                cx,
            ),
        ));
    }
    let (shift_title, shift_detail) = if mac() {
        ("Double-tap ⇧ to click through", "Clicks reach the app and the hand stays. Double-tap again for taps. The menu bar icon turns orange.")
    } else {
        ("Double-tap Shift to click through", "Clicks reach the app and the hand stays. Double-tap again for taps. The tray icon turns orange.")
    };
    clicking.push(row(
        t,
        shift_title,
        Some(shift_detail.into()),
        switch(t, "double-shift", s.double_shift_click_through, true, cx, |this, on, _, cx| {
            this.change(cx, |s| s.double_shift_click_through = on)
        }),
    ));
    blocks.push(card(t, Some("Clicking for real"), clicking).into_any_element());

    // "Check for updates automatically" joins this card once there is an updater.
    blocks.push(
        card(
            t,
            Some("Startup"),
            vec![row(
                t,
                "Open TinyHand when you log in",
                None,
                switch(t, "login", this.login, true, cx, |this, on, _, cx| {
                    match tinyhand_platform::login::set_launch_at_login(on) {
                        Ok(()) => this.login = on,
                        Err(e) => {
                            crate::controller::log(&format!("open at login: {e}"));
                            this.login = tinyhand_platform::login::launch_at_login();
                        }
                    }
                    cx.notify();
                }),
            )],
        )
        .into_any_element(),
    );

    div().flex().flex_col().gap(px(22.)).children(blocks).into_any_element()
}
