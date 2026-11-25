//! The welcome card (WelcomeWindow.swift): shown on the first launch, and on macOS
//! whenever TinyHand isn't allowed to use Accessibility yet. Two states:
//!
//! - not trusted: "Allow Accessibility" and Open Settings (the system prompt and the
//!   Accessibility pane), with a tip 10 s after that if it still isn't allowed;
//! - trusted: "You're all set" and Start, which closes the card and turns the pointer on.
//!
//! It checks the permission every 0.5 s, so it flips by itself once allowed. Windows and
//! Linux need no permission, so there it only ever shows the trusted state, once.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, FontWeight, Hsla, KeyDownEvent, MouseButton, RenderImage,
    Task, Window, WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    div, hsla, img, prelude::*, px, rgb, size, svg,
};

use tinyhand_platform as native;
use tinyhand_platform::input;

use crate::assets;
use crate::controller::{Controller, log};

const WIDTH: f32 = 340.0;
const HEIGHT: f32 = 300.0;
const RADIUS: f32 = 20.0;
const TIP_AFTER: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct Welcome {
    window: Option<WindowHandle<WelcomeView>>,
    opening: bool,
    asked_at: Option<Instant>,
    /// TH_SHOW=welcome-untrusted: show the untrusted state whatever the permission.
    pub simulate_untrusted: bool,
    icon: Option<Arc<RenderImage>>,
}

impl Controller {
    fn welcome_trusted(&self) -> bool {
        input::is_trusted() && !self.welcome.simulate_untrusted
    }

    /// `WelcomeWindow.show`: opens the card (or brings it forward) and remembers that it
    /// was shown.
    pub fn show_welcome(&mut self, cx: &mut Context<Self>) {
        if !self.settings.welcome_shown {
            self.settings.welcome_shown = true;
            self.save_settings();
        }
        cx.activate(true);
        if let Some(handle) = self.welcome.window {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            });
            return;
        }
        if self.welcome.opening {
            return;
        }
        self.welcome.opening = true;
        if self.welcome.icon.is_none() {
            self.welcome.icon = assets::app_icon()
                .and_then(|p| image::open(&p).map_err(|e| log(&format!("app icon: {e}"))).ok())
                .map(|i| assets::render_image(image::imageops::resize(&i.into_rgba8(), 228, 228, image::imageops::FilterType::Lanczos3)));
        }
        let this = cx.entity();
        let display = self.display;
        let kind = self.card_kind();
        cx.defer(move |cx: &mut App| {
            let display_id = display.map(|d| gpui::DisplayId::from(d.id)).or_else(|| cx.primary_display().map(|d| d.id()));
            let bounds = Bounds::centered(display_id, size(px(WIDTH), px(HEIGHT)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                titlebar: None,
                window_background: if cfg!(target_os = "macos") {
                    WindowBackgroundAppearance::Blurred
                } else {
                    WindowBackgroundAppearance::Transparent
                },
                focus: true,
                show: true,
                kind,
                is_movable: true,
                is_resizable: false,
                is_minimizable: false,
                app_id: Some("tinyhand".into()),
                ..Default::default()
            };
            let view_of = this.clone();
            let opened = cx.open_window(options, move |window, cx| {
                if let Err(e) = native::make_card(window, RADIUS as f64) {
                    log(&format!("welcome card setup: {e}"));
                }
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                cx.new(|cx| WelcomeView::new(view_of, focus, window, cx))
            });
            this.update(cx, |c, _| {
                c.welcome.opening = false;
                match opened {
                    Ok(handle) => c.welcome.window = Some(handle),
                    Err(e) => log(&format!("can't open the welcome card: {e}")),
                }
            });
        });
    }

    /// A normal window; on a layer-shell compositor (which may tile normal windows) a
    /// centred surface on the top layer that takes the keyboard when clicked.
    fn card_kind(&self) -> WindowKind {
        #[cfg(target_os = "linux")]
        if let WindowKind::LayerShell(_) = (self.window_kind)() {
            use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
            return WindowKind::LayerShell(LayerShellOptions {
                namespace: "tinyhand-welcome".into(),
                layer: Layer::Top,
                anchor: Anchor::empty(),
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                ..Default::default()
            });
        }
        WindowKind::Normal
    }

    /// Closes the card.
    pub fn close_welcome(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.welcome.window.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    /// Redraw the card (a setting it shows changed).
    pub fn refresh_welcome(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.welcome.window {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.refresh());
            });
        }
    }

    /// Open Settings, or Start.
    fn welcome_button(&mut self, cx: &mut Context<Self>) {
        if self.welcome_trusted() {
            self.close_welcome(cx);
            self.activate(cx);
            return;
        }
        self.welcome.asked_at = Some(Instant::now());
        input::request_trust();
    }

    fn welcome_snapshot(&self) -> Snapshot {
        let trusted = self.welcome_trusted();
        let mac = cfg!(target_os = "macos");
        let tip = !trusted && self.welcome.asked_at.is_some_and(|at| at.elapsed() >= TIP_AFTER);
        Snapshot {
            title: if trusted { "You're all set" } else { "Allow Accessibility" },
            subtitle: if trusted {
                format!("Press {} any time to start.", self.settings.toggle_shortcut.display(mac))
            } else {
                "So your clicks become taps, not real clicks.".into()
            },
            button: if trusted { "Start" } else { "Open Settings" },
            tip,
            icon: self.welcome.icon.clone(),
        }
    }
}

struct Snapshot {
    title: &'static str,
    subtitle: String,
    button: &'static str,
    tip: bool,
    icon: Option<Arc<RenderImage>>,
}

pub struct WelcomeView {
    controller: Entity<Controller>,
    focus: FocusHandle,
    _refresh: Task<()>,
}

impl WelcomeView {
    fn new(controller: Entity<Controller>, focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Checks the permission every 0.5 s (WelcomeWindow's timer).
        let refresh = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        // Closed by the system (⌘W, the window list): forget it.
        let closing = controller.clone();
        window.on_window_should_close(cx, move |_, cx| {
            closing.update(cx, |c, _| c.welcome.window = None);
            true
        });
        WelcomeView { controller, focus, _refresh: refresh }
    }
}

struct Palette {
    card: Hsla,
    border: Hsla,
    label: Hsla,
    secondary: Hsla,
    tertiary: Hsla,
}

fn palette(appearance: WindowAppearance) -> Palette {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Palette {
            card: hsla(240.0 / 360.0, 0.03, 0.15, if cfg!(target_os = "macos") { 0.78 } else { 0.97 }),
            border: hsla(0.0, 0.0, 1.0, 0.12),
            label: hsla(0.0, 0.0, 1.0, 0.92),
            secondary: hsla(0.0, 0.0, 1.0, 0.55),
            tertiary: hsla(0.0, 0.0, 1.0, 0.28),
        },
        WindowAppearance::Light | WindowAppearance::VibrantLight => Palette {
            card: hsla(0.0, 0.0, 0.965, if cfg!(target_os = "macos") { 0.82 } else { 0.98 }),
            border: hsla(0.0, 0.0, 0.0, 0.10),
            label: hsla(0.0, 0.0, 0.0, 0.88),
            secondary: hsla(0.0, 0.0, 0.0, 0.5),
            tertiary: hsla(0.0, 0.0, 0.0, 0.26),
        },
    }
}

impl Render for WelcomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.controller.read(cx).welcome_snapshot();
        let p = palette(window.appearance());
        let press = self.controller.clone();
        let close = self.controller.clone();
        let keys = self.controller.clone();

        let icon = match s.icon {
            Some(i) => img(i).size(px(76.0)).into_any_element(),
            None => div().size(px(76.0)).into_any_element(),
        };
        let mut stack = div()
            .flex()
            .flex_col()
            .items_center()
            .child(icon)
            .child(
                div()
                    .mt(px(16.0))
                    .text_size(px(19.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.label)
                    .child(s.title),
            )
            .child(div().mt(px(6.0)).text_size(px(13.0)).text_color(p.secondary).child(s.subtitle))
            .child(
                div()
                    .id("welcome-button")
                    .mt(px(22.0))
                    .w(px(180.0))
                    .h(px(28.0))
                    .rounded(px(8.0))
                    .bg(rgb(0x0a84ff))
                    .hover(|st| st.bg(rgb(0x1f8fff)))
                    .active(|st| st.bg(rgb(0x0070e0)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(gpui::white())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| press.update(cx, |c, cx| c.welcome_button(cx)))
                    .child(s.button),
            );
        if s.tip {
            stack = stack.child(
                div().mt(px(12.0)).text_size(px(11.0)).text_color(p.secondary).child("Already on? Remove it and add it again."),
            );
        }

        div()
            .id("welcome")
            .track_focus(&self.focus)
            .size_full()
            .font_family(".SystemUIFont")
            .rounded(px(RADIUS))
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .on_key_down(move |e: &KeyDownEvent, _, cx| match e.keystroke.key.as_str() {
                "escape" => keys.update(cx, |c, cx| c.close_welcome(cx)),
                "enter" => keys.update(cx, |c, cx| c.welcome_button(cx)),
                _ => {}
            })
            .child(div().mt(px(8.0)).child(stack))
            .child(
                div()
                    .id("welcome-close")
                    .absolute()
                    .top(px(12.0))
                    .right(px(12.0))
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| close.update(cx, |c, cx| c.close_welcome(cx)))
                    .child(svg().path("icons/xmark.svg").size(px(15.0)).text_color(p.tertiary)),
            )
    }
}
