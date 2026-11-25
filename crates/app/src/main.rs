//! TinyHand: a hand on a stick that points at things on screen, on macOS, Windows and
//! Linux. The logic lives in `tinyhand-core`, the per-OS window and input code in
//! `tinyhand-platform`; this crate is the app, drawn with GPUI.
//!
//! - `main.rs`: command line, one instance per user, settings, start-up.
//! - `controller.rs`: on/off, tray, shortcuts, input tap, commands (AppDelegate and
//!   OverlayController in the Swift app).
//! - `overlay.rs`: the overlay window and what moves on it; `paint.rs` draws it.
//! - `tray_menu.rs`, `shift.rs` (double-⇧), `instance.rs`, `assets.rs`.
//!
//! The app starts with the pointer off, as a tray icon. Commands: `tinyhand on|off|toggle|
//! clear|style <id>|sound on|off|toggle` (or a `tinyhand://` URL) go to the running copy.
//!
//! For unattended checks: TH_FAKE_MOUSE=x,y pins the pointer (and never taps input),
//! TH_TAP_EVERY=seconds taps on a timer, TH_SECONDS=n quits after n seconds, TH_ACTIVATE=1
//! turns the pointer on at launch, TH_DEBUG=1 logs every frame, TH_CONFIG_DIR puts the
//! settings elsewhere, TH_HANDS_DIR the custom hands, TH_INPUT=capture uses the Linux way
//! of reading the mouse anywhere, TH_BACKEND=x11 forces X11 on Linux.

mod assets;
mod controller;
mod instance;
mod overlay;
mod paint;
mod shift;
mod tray_menu;

use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{App, AppContext, QuitMode, WindowKind};
use tinyhand_core::automation::{self, Command};
use tinyhand_core::settings::{self, Settings};
use tinyhand_platform as native;

use controller::{Controller, Input, Msg, log};

const USAGE: &str = "usage: tinyhand [on | off | toggle | clear | style <id> | sound [on|off|toggle] | tinyhand://…]";

/// Which Linux display server to use.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq)]
enum Linux {
    /// A Wayland compositor with layer shell (KDE, Hyprland, Sway, ...).
    LayerShell,
    /// X11, or XWayland on a Wayland compositor without layer shell (GNOME, so stock
    /// Ubuntu). Every GNOME session runs XWayland.
    X11,
}

#[cfg(target_os = "linux")]
fn pick_linux() -> Linux {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    if !wayland || std::env::var("TH_BACKEND").is_ok_and(|v| v == "x11") {
        return Linux::X11;
    }
    match native::wayland_has_layer_shell() {
        Some(true) => Linux::LayerShell,
        Some(false) => {
            log("this Wayland compositor has no layer shell; using XWayland instead");
            Linux::X11
        }
        None => Linux::X11,
    }
}

/// How the overlay window is made on this platform. `TH_INPUT=capture` uses the Linux way
/// (take the mouse) anywhere, to try it on a Mac.
fn window_kind(#[cfg(target_os = "linux")] linux: Linux) -> (Input, Box<dyn Fn() -> WindowKind>) {
    if std::env::var("TH_INPUT").is_ok_and(|v| v == "capture") && !cfg!(target_os = "linux") {
        return (Input::Capture, Box::new(|| WindowKind::PopUp));
    }
    #[cfg(target_os = "linux")]
    {
        if linux == Linux::LayerShell {
            use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
            use gpui::px;
            // A layer-shell surface on the overlay layer, stretched over the whole output.
            // It takes the keyboard only once clicked, for Esc.
            return (
                Input::Capture,
                Box::new(|| {
                    WindowKind::LayerShell(LayerShellOptions {
                        namespace: "tinyhand".into(),
                        layer: Layer::Overlay,
                        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                        // -1: don't make room for panels, cover them too.
                        exclusive_zone: Some(px(-1.0)),
                        keyboard_interactivity: KeyboardInteractivity::OnDemand,
                        ..Default::default()
                    })
                }),
            );
        }
        return (Input::Capture, Box::new(|| WindowKind::PopUp));
    }
    #[allow(unreachable_code)]
    (Input::Watch, Box::new(|| WindowKind::PopUp))
}

/// The settings file, or on a Mac's first run the Swift app's UserDefaults.
fn load_settings() -> Settings {
    let path = settings::settings_path();
    if cfg!(target_os = "macos") && !path.exists() && std::env::var_os("TH_CONFIG_DIR").is_none() {
        let exported = std::process::Command::new("defaults")
            .args(["export", "com.kamranahmed.tinyhand", "-"])
            .output();
        if let Some(out) = exported.ok().filter(|o| o.status.success() && !o.stdout.is_empty()) {
            let s = settings::import_macos_defaults(&out.stdout);
            log("imported the settings of the Swift app");
            if let Err(e) = s.save() {
                log(&format!("can't save settings: {e}"));
            }
            return s;
        }
    }
    Settings::load()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pending: Option<Command> = None;
    if !args.is_empty() {
        let Some(command) = automation::parse_args(&args) else {
            eprintln!("{USAGE}");
            std::process::exit(2);
        };
        match instance::send(&args) {
            Ok(true) => return,
            Ok(false) => pending = Some(command),
            Err(e) => {
                eprintln!("tinyhand: {e}");
                std::process::exit(1);
            }
        }
    } else if instance::running() {
        eprintln!("TinyHand is already running.");
        return;
    }

    let settings = load_settings();

    #[cfg(target_os = "linux")]
    let linux = pick_linux();
    #[cfg(target_os = "linux")]
    let app = match linux {
        Linux::LayerShell => gpui_platform::linux(gpui::WindowingModes::WAYLAND),
        Linux::X11 => gpui_platform::linux(gpui::WindowingModes::X11),
    };
    #[cfg(not(target_os = "linux"))]
    let app = gpui_platform::application();
    // A tray app: closing the overlay must not quit.
    let app = app.with_quit_mode(QuitMode::Explicit);

    let (tx, mut rx) = mpsc::unbounded::<Msg>();
    {
        let tx = tx.clone();
        app.on_open_urls(move |urls| {
            for url in urls {
                match automation::parse_url(&url) {
                    Some(command) => {
                        let _ = tx.unbounded_send(Msg::Command(command));
                    }
                    None => log(&format!("unknown url: {url}")),
                }
            }
        });
    }

    let (input, kind) = window_kind(
        #[cfg(target_os = "linux")]
        linux,
    );

    app.run(move |cx: &mut App| {
        native::hide_from_dock();
        let listen_tx = tx.clone();
        if let Err(e) = instance::listen(move |command| {
            let _ = listen_tx.unbounded_send(Msg::Command(command));
        }) {
            log(&format!("can't listen for commands: {e}"));
        }

        let controller = cx.new(|_| Controller::new(settings, input, kind, tx));
        let pump = controller.clone();
        cx.spawn(async move |cx| {
            while let Some(msg) = rx.next().await {
                pump.update(cx, |c, cx| c.handle(msg, cx));
            }
        })
        .detach();

        let quitting = controller.clone();
        cx.on_app_quit(move |cx| {
            quitting.update(cx, |c, cx| c.shutdown(cx));
            async {}
        })
        .detach();

        controller.update(cx, |c, cx| {
            c.launch();
            if let Some(command) = pending.take() {
                c.command(command, cx);
            }
            if c.hooks.activate {
                c.activate(cx);
            }
            if let Some(seconds) = c.hooks.quit_after {
                cx.spawn(async move |_, cx| {
                    cx.background_executor().timer(Duration::from_secs_f32(seconds)).await;
                    log("TH_SECONDS reached, quitting");
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
    });
}
