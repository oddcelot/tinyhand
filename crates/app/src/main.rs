//! TinyHand: a hand on a stick that points at things on screen, on macOS, Windows and
//! Linux. The logic lives in `tinyhand-core`, the per-OS window and input code in
//! `tinyhand-platform`; this crate draws with GPUI.
//!
//! Two ways to get at the mouse (see `Input`):
//! - macOS and Windows: the overlay lets every click through and reads the mouse with
//!   native calls. Clicks still reach the app underneath.
//! - Linux: the overlay takes the mouse itself. That is the only way Wayland reports
//!   where the pointer is, and it swallows the clicks, which is what TinyHand's tap does.
//!
//! Run from this folder:
//!     cargo run --release -p tinyhand               # Butterfly 3D
//!     cargo run --release -p tinyhand -- lavender   # any hand in assets/
//! Quit with Ctrl+C in the terminal. On Linux, where the overlay takes the mouse,
//! right-click or Esc quits too.
//!
//! For unattended checks: TH_FAKE_MOUSE=x,y pins the pointer, TH_TAP_EVERY=seconds taps
//! on a timer, TH_SECONDS=n quits after n seconds.

use tinyhand_platform as native;

use std::time::{Duration, Instant};

use gpui::{
    App, BorderStyle, Bounds, Context, Corners, Edges, FocusHandle, Hsla, KeyDownEvent,
    MouseButton, PathBuilder, Pixels,
    Point, Rgba, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px, quad, size,
};

use std::collections::HashMap;
use std::sync::Arc;

use tinyhand_core::particles::{HUES, Kind, Particle, Particles, TapEffect};
use tinyhand_core::{Arm, ArmPose, Hand};

fn tone(index: usize, alpha: f32, lighten: f32) -> Hsla {
    let (r, g, b) = HUES[index % HUES.len()];
    Rgba {
        r: r + (1.0 - r) * lighten,
        g: g + (1.0 - g) * lighten,
        b: b + (1.0 - b) * lighten,
        a: alpha,
    }
    .into()
}

fn white(alpha: f32) -> Hsla {
    Rgba { r: 1.0, g: 1.0, b: 1.0, a: alpha }.into()
}

/// Where the mouse comes from.
#[derive(Clone, Copy, PartialEq)]
enum Input {
    /// The overlay ignores the mouse; position and button are read with native calls.
    Watch,
    /// The overlay covers the screen and takes the mouse, so GPUI's own mouse events say
    /// where it is. Clicks don't reach the apps underneath.
    Capture,
}

struct Overlay {
    input: Input,
    focus: FocusHandle,
    /// Left button, as GPUI reports it in `Input::Capture`.
    captured_down: bool,
    /// Set by a press and cleared by the next frame, so a click that goes down and up
    /// between two frames still counts.
    captured_press: bool,
    hand: Hand,
    fitted: bool,
    /// GPU copies of the rotated hand, by `Hand::rotated`'s key.
    images: HashMap<i32, Arc<gpui::RenderImage>>,
    width: f32,
    height: f32,
    arm: Arm,
    effect: TapEffect,
    was_down: bool,
    particles: Particles,
    last_frame: Instant,
    started: Instant,
    last_auto_tap: Instant,
    hooks: TestHooks,
}

/// Settings for unattended runs, read from the environment once.
struct TestHooks {
    /// TH_FAKE_MOUSE=x,y pins the pointer.
    fake_mouse: Option<(f32, f32)>,
    /// TH_TAP_EVERY=seconds taps on a timer.
    tap_every: Option<f32>,
    /// TH_SECONDS=n quits after n seconds.
    quit_after: Option<f32>,
    /// TH_DEBUG logs every frame.
    debug: bool,
}

impl TestHooks {
    fn from_env() -> Self {
        let env = |key: &str| std::env::var(key).ok();
        TestHooks {
            fake_mouse: env("TH_FAKE_MOUSE").and_then(|v| {
                let (x, y) = v.split_once(',')?;
                Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
            }),
            tap_every: env("TH_TAP_EVERY").and_then(|v| v.parse().ok()),
            quit_after: env("TH_SECONDS").and_then(|v| v.parse().ok()),
            debug: std::env::var_os("TH_DEBUG").is_some(),
        }
    }
}

enum Draw {
    /// Outline, highlight strip, and the angle (CSS degrees) across the stick for its shading.
    Rod([Point<Pixels>; 4], [Point<Pixels>; 4], f32),
    Hand(Bounds<Pixels>, std::sync::Arc<gpui::RenderImage>),
    Particle(Particle),
}

impl Overlay {
    fn new(input: Input, focus: FocusHandle, hand: Hand, width: f32, height: f32) -> Self {
        Overlay {
            input,
            focus,
            captured_down: false,
            captured_press: false,
            hand,
            fitted: false,
            images: HashMap::new(),
            width,
            height,
            arm: Arm::default(),
            effect: TapEffect::default(),
            was_down: false,
            particles: Particles::default(),
            last_frame: Instant::now(),
            started: Instant::now(),
            last_auto_tap: Instant::now(),
            hooks: TestHooks::from_env(),
        }
    }

    /// This frame's stick and hand, ready to paint.
    fn arm_draws(&mut self, cursor: (f32, f32), dt: f32, scale: f32) -> Vec<Draw> {
        if !self.fitted {
            self.hand.fit(Arm::hand_pixels(self.height, scale));
            self.images.clear();
            self.fitted = true;
        }
        let pose: ArmPose = self.arm.aim(&self.hand, cursor, (self.width, self.height), dt);
        let (key, rotated) = self.hand.rotated(pose.rotation);
        let image = self
            .images
            .entry(key)
            .or_insert_with(|| Arc::new(gpui::RenderImage::new(vec![image::Frame::new(rotated.bgra.clone())])))
            .clone();
        let (bw, bh) = (pose.hand_size.0 * rotated.width_factor, pose.hand_size.1 * rotated.height_factor);
        let bounds = Bounds::new(
            point(px(pose.hand_center.0 - bw / 2.0), px(pose.hand_center.1 - bh / 2.0)),
            size(px(bw), px(bh)),
        );
        let pts = |q: [(f32, f32); 4]| q.map(|(x, y)| point(px(x), px(y)));
        vec![Draw::Rod(pts(pose.rod), pts(pose.sheen), pose.rod_shade_angle), Draw::Hand(bounds, image)]
    }
}

fn polygon(points: &[Point<Pixels>]) -> Option<gpui::Path> {
    let mut b = PathBuilder::fill();
    b.move_to(points[0]);
    for p in &points[1..] {
        b.line_to(*p);
    }
    b.close();
    b.build().ok()
}

fn star(cx: f32, cy: f32, radius: f32, angle: f32, pinch: f32) -> Option<gpui::Path> {
    let tip = |i: usize| {
        let a = angle + i as f32 * std::f32::consts::FRAC_PI_2;
        (cx + radius * a.cos(), cy + radius * a.sin())
    };
    let mut b = PathBuilder::fill();
    let start = tip(3);
    b.move_to(point(px(start.0), px(start.1)));
    for i in 0..4 {
        let (from, to) = (tip((i + 3) % 4), tip(i));
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        let ctrl = (cx + (mid.0 - cx) * pinch, cy + (mid.1 - cy) * pinch);
        b.curve_to(point(px(to.0), px(to.1)), point(px(ctrl.0), px(ctrl.1)));
    }
    b.close();
    b.build().ok()
}

fn circle(cx: f32, cy: f32, d: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(cx - d / 2.0), px(cy - d / 2.0)), size(px(d), px(d)))
}

fn paint_particle(p: &Particle, window: &mut Window) {
    let o = p.opacity();
    let (x, y, d) = (p.x, p.y, p.size);
    match p.kind {
        Kind::Ring => {
            window.paint_quad(quad(
                circle(x, y, d), Corners::all(px(d / 2.0)), white(0.0),
                Edges::all(px(2.0)), tone(1, 0.6 * o, 0.4), BorderStyle::Solid,
            ));
        }
        Kind::Star => {
            let r = d / 2.0;
            let body = linear_gradient(
                p.angle.to_degrees().rem_euclid(360.0),
                linear_color_stop(tone(p.hue, 0.85 * o, 0.0), 0.0),
                linear_color_stop(tone(p.hue + 2, 0.55 * o, 0.25), 1.0),
            );
            if let Some(path) = star(x, y, r, p.angle, 0.16) {
                window.paint_path(path, body);
            }
            if let Some(path) = star(x - r * 0.08, y - r * 0.1, r * 0.3, p.angle, 0.1) {
                window.paint_path(path, white(0.95 * o));
            }
        }
        Kind::Bead | Kind::Bubble => {
            let body = linear_gradient(
                135.0,
                linear_color_stop(tone(p.hue, 0.18 * o, 0.5), 0.0),
                linear_color_stop(tone(p.hue + 1, 0.75 * o, 0.0), 1.0),
            );
            let rim = if p.kind == Kind::Bubble { 1.2 } else { 0.8 };
            window.paint_quad(quad(
                circle(x, y, d), Corners::all(px(d / 2.0)), body,
                Edges::all(px(rim)), tone(p.hue + 1, 0.85 * o, 0.3), BorderStyle::Solid,
            ));
            let g = d * 0.32;
            window.paint_quad(quad(
                circle(x - d * 0.2, y - d * 0.2, g), Corners::all(px(g / 2.0)), white(0.9 * o),
                Edges::all(px(0.0)), white(0.0), BorderStyle::Solid,
            ));
        }
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        if self.hooks.quit_after.is_some_and(|s| self.started.elapsed() > Duration::from_secs_f32(s)) {
            cx.quit();
        }

        let scale = window.scale_factor();
        let viewport = window.viewport_size();
        if f32::from(viewport.height) > 0.0 {
            self.width = f32::from(viewport.width);
            self.height = f32::from(viewport.height);
        }
        let mut mouse = match self.input {
            Input::Watch => native::mouse(self.height, scale),
            Input::Capture => {
                let p = window.mouse_position();
                let down = self.captured_down || std::mem::take(&mut self.captured_press);
                native::Mouse { x: f32::from(p.x), y: f32::from(p.y), down }
            }
        };
        if let Some((x, y)) = self.hooks.fake_mouse {
            mouse.x = x;
            mouse.y = y;
        }
        if let Some(every) = self.hooks.tap_every {
            if now.duration_since(self.last_auto_tap).as_secs_f32() > every {
                self.last_auto_tap = now;
                mouse.down = true;
            }
        }

        if self.hooks.debug {
            eprintln!("frame at {:.3}s mouse {:.0},{:.0} down={}", self.started.elapsed().as_secs_f32(), mouse.x, mouse.y, mouse.down);
        }
        if mouse.down && !self.was_down {
            self.arm.press_now();
            self.particles.burst(self.effect, mouse.x, mouse.y, self.height / 900.0);
        } else if !mouse.down {
            self.arm.ease_press(dt);
        }
        self.was_down = mouse.down;
        self.particles.step(dt);

        let mut draws = self.arm_draws((mouse.x, mouse.y), dt, scale);
        draws.extend(self.particles.live.iter().map(|p| Draw::Particle(*p)));

        let painting = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                for draw in draws {
                    match draw {
                        Draw::Rod(rod, sheen, across) => {
                            let gray = |v: f32| Hsla::from(Rgba { r: v, g: v, b: v * 1.04, a: 1.0 });
                            if let Some(path) = polygon(&rod) {
                                let body = linear_gradient(
                                    across,
                                    linear_color_stop(gray(0.24), 0.0),
                                    linear_color_stop(gray(0.02), 1.0),
                                );
                                window.paint_path(path, body);
                            }
                            if let Some(path) = polygon(&sheen) {
                                window.paint_path(path, white(0.22));
                            }
                        }
                        Draw::Hand(bounds, image) => {
                            let _ = window.paint_image(bounds, bounds, Corners::default(), image, 0, false);
                        }
                        Draw::Particle(p) => paint_particle(&p, window),
                    }
                }
            },
        )
        .size_full();

        // Only reached in capture mode: in watch mode the window never gets the mouse.
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                if std::env::var_os("TH_DEBUG").is_some() {
                    eprintln!("mouse down event at {:?}", e.position);
                }
                this.captured_down = true;
                this.captured_press = true;
                // So Esc reaches the overlay where the system allows it.
                window.focus(&this.focus, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.captured_down = false))
            // The overlay takes every click, so there has to be a way out without a terminal.
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.quit())
            .on_key_down(|e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    cx.quit();
                }
            })
            .child(painting)
    }
}

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
            eprintln!("this Wayland compositor has no layer shell; using XWayland instead");
            Linux::X11
        }
        None => Linux::X11,
    }
}

/// How the overlay window is made on this platform. `TH_INPUT=capture` uses the Linux way
/// (take the mouse) anywhere, to try it on a Mac.
fn window_kind(#[cfg(target_os = "linux")] linux: Linux) -> (Input, WindowKind) {
    if std::env::var("TH_INPUT").is_ok_and(|v| v == "capture") && !cfg!(target_os = "linux") {
        return (Input::Capture, WindowKind::PopUp);
    }
    #[cfg(target_os = "linux")]
    {
        if linux == Linux::LayerShell {
            use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
            // A layer-shell surface on the overlay layer, stretched over the whole output.
            // It takes the keyboard only once clicked, for Esc.
            return (
                Input::Capture,
                WindowKind::LayerShell(LayerShellOptions {
                    namespace: "tinyhand".into(),
                    layer: Layer::Overlay,
                    anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                    // -1: don't make room for panels, cover them too.
                    exclusive_zone: Some(px(-1.0)),
                    keyboard_interactivity: KeyboardInteractivity::OnDemand,
                    ..Default::default()
                }),
            );
        }
        return (Input::Capture, WindowKind::PopUp);
    }
    #[allow(unreachable_code)]
    (Input::Watch, WindowKind::PopUp)
}

/// Where the hand images are: `TH_HANDS_DIR`, next to an installed binary
/// (`../share/tinyhand/hands`, as the .deb lays it out), or the repo's assets folder.
fn hand_path(name: &str) -> std::path::PathBuf {
    let file = format!("hand_{name}.png");
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("TH_HANDS_DIR") {
        dirs.push(dir.into());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin) = exe.parent() {
            dirs.push(bin.join("../share/tinyhand/hands"));
        }
    }
    dirs.push("/usr/share/tinyhand/hands".into());
    dirs.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    dirs.iter()
        .map(|d| d.join(&file))
        .find(|p| p.exists())
        .unwrap_or_else(|| dirs.last().unwrap().join(&file))
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "butterfly3d".into());
    let path = hand_path(&name);
    let hand = match Hand::load(&path) {
        Ok(hand) => hand,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    #[cfg(target_os = "linux")]
    let linux = pick_linux();
    #[cfg(target_os = "linux")]
    let app = match linux {
        Linux::LayerShell => gpui_platform::linux(gpui::WindowingModes::WAYLAND),
        Linux::X11 => gpui_platform::linux(gpui::WindowingModes::X11),
    };
    #[cfg(not(target_os = "linux"))]
    let app = gpui_platform::application();

    app.run(move |cx: &mut App| {
        native::hide_from_dock();
        // Wayland may not list displays before the first window exists; the overlay
        // takes its real size from the window once it is drawn.
        let display = cx.primary_display();
        let bounds = display.as_ref().map(|d| d.bounds()).unwrap_or_else(|| {
            Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)))
        });
        let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let (input, kind) = window_kind(
            #[cfg(target_os = "linux")]
            linux,
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: display.as_ref().map(|d| d.id()),
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
        let opened = cx.open_window(options, |window, cx| {
            if let Err(e) = native::make_overlay(window, input == Input::Watch) {
                eprintln!("overlay setup: {e}");
            }
            let focus = cx.focus_handle();
            cx.new(|_| Overlay::new(input, focus, hand, width, height))
        });
        if let Err(e) = opened {
            eprintln!("can't open the overlay: {e}");
            cx.quit();
        }
    });
}
