//! TinyHand overlay spike: can GPUI draw the pointer on a window above everything, on
//! macOS, Windows and Linux?
//!
//! Draws the hand on its stick following the mouse and throws glass fairy dust when the
//! left button goes down. Settings, toolbar, drawing and sounds are out of scope.
//!
//! Two ways to get at the mouse (see `Input`):
//! - macOS and Windows: the overlay lets every click through and reads the mouse with
//!   native calls. Clicks still reach the app underneath.
//! - Linux: the overlay takes the mouse itself. That is the only way Wayland reports
//!   where the pointer is, and it swallows the clicks, which is what TinyHand's tap does.
//!
//! Run from this folder:
//!     cargo run --release                     # Butterfly 3D
//!     cargo run --release -- lavender         # any hand in ../../assets
//! Quit with Ctrl+C in the terminal. On Linux, where the overlay takes the mouse,
//! right-click or Esc quits too.
//!
//! For unattended checks: TH_FAKE_MOUSE=x,y pins the pointer, TH_TAP_EVERY=seconds taps
//! on a timer, TH_SECONDS=n quits after n seconds.

mod hand;
mod native;

use std::time::{Duration, Instant};

use gpui::{
    App, BorderStyle, Bounds, Context, Corners, Edges, FocusHandle, Hsla, KeyDownEvent,
    MouseButton, PathBuilder, Pixels,
    Point, Rgba, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px, quad, size,
};

use hand::Hand;

// Same proportions as the Swift ArmView.
const HAND_LEN_FRAC: f32 = 0.13;
const MIN_ROD_FRAC: f32 = 0.05;
const GRIP_LEAN_FRAC: f32 = 0.30;
const GRIP_DEPTH_FRAC: f32 = 0.12;
const GRIP_FOLLOW: f32 = 0.08;

/// The fairy dust's jewel tones, as in TapEffect.swift.
const HUES: [(f32, f32, f32); 5] = [
    (0.96, 0.30, 0.68),
    (0.60, 0.38, 0.98),
    (0.16, 0.62, 1.00),
    (0.10, 0.84, 0.70),
    (1.00, 0.70, 0.16),
];

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

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Ring,
    Star,
    Bead,
    Bubble,
}

#[derive(Clone, Copy)]
struct Particle {
    kind: Kind,
    hue: usize,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    gravity: f32,
    size: f32,
    shrink: f32,
    spin: f32,
    angle: f32,
    age: f32,
    life: f32,
}

/// Tiny xorshift, so the spike needs no extra crate for randomness.
struct Random(u64);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    /// `base` give or take `range`.
    fn around(&mut self, base: f32, range: f32) -> f32 {
        base + (self.next() * 2.0 - 1.0) * range
    }
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
    width: f32,
    height: f32,
    grip_x: Option<f32>,
    press: f32,
    was_down: bool,
    particles: Vec<Particle>,
    random: Random,
    last_frame: Instant,
    started: Instant,
    last_auto_tap: Instant,
    fake_mouse: Option<(f32, f32)>,
    tap_every: Option<f32>,
    quit_after: Option<f32>,
}

enum Draw {
    /// Outline, highlight strip, and the angle (CSS degrees) across the stick for its shading.
    Rod([Point<Pixels>; 4], [Point<Pixels>; 4], f32),
    Hand(Bounds<Pixels>, std::sync::Arc<gpui::RenderImage>),
    Particle(Particle),
}

impl Overlay {
    fn new(input: Input, focus: FocusHandle, hand: Hand, width: f32, height: f32) -> Self {
        let env = |key: &str| std::env::var(key).ok();
        let fake_mouse = env("TH_FAKE_MOUSE").and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        });
        let seed = Instant::now().elapsed().as_nanos() as u64 ^ 0x9E37_79B9_7F4A_7C15;
        Overlay {
            input,
            focus,
            captured_down: false,
            captured_press: false,
            hand,
            fitted: false,
            width,
            height,
            grip_x: None,
            press: 0.0,
            was_down: false,
            particles: Vec::new(),
            random: Random(seed | 1),
            last_frame: Instant::now(),
            started: Instant::now(),
            last_auto_tap: Instant::now(),
            fake_mouse,
            tap_every: env("TH_TAP_EVERY").and_then(|v| v.parse().ok()),
            quit_after: env("TH_SECONDS").and_then(|v| v.parse().ok()),
        }
    }

    fn fairy_dust(&mut self, x: f32, y: f32) {
        let unit = self.height / 900.0;
        let r = &mut self.random;
        let mut add = |p: Particle| self.particles.push(p);
        add(Particle {
            kind: Kind::Ring, hue: 1, x, y, vx: 0.0, vy: 0.0, gravity: 0.0,
            size: 4.0 * unit, shrink: -150.0 * unit, spin: 0.0, angle: 0.0, age: 0.0, life: 0.5,
        });
        for hue in 0..HUES.len() {
            let fling = |kind: Kind, count: usize, speed: (f32, f32), gravity: f32, size: (f32, f32), shrink: f32, life: (f32, f32), spin: f32, r: &mut Random| {
                (0..count)
                    .map(|_| {
                        let heading = r.next() * std::f32::consts::TAU;
                        let v = r.around(speed.0, speed.1).max(0.0) * unit;
                        Particle {
                            kind, hue, x, y,
                            vx: heading.cos() * v,
                            vy: heading.sin() * v,
                            gravity: gravity * unit,
                            size: r.around(size.0, size.1).max(1.0) * unit,
                            shrink: shrink * unit,
                            spin: r.around(0.0, spin),
                            angle: r.next() * std::f32::consts::TAU,
                            age: 0.0,
                            life: r.around(life.0, life.1).max(0.2),
                        }
                    })
                    .collect::<Vec<_>>()
            };
            for p in fling(Kind::Star, 3, (120.0, 75.0), 120.0, (24.0, 11.0), 13.0, (1.1, 0.4), 4.0, r) {
                add(p);
            }
            for p in fling(Kind::Bead, 10, (100.0, 90.0), 130.0, (5.6, 2.8), 2.8, (0.85, 0.4), 3.0, r) {
                add(p);
            }
            for p in fling(Kind::Bubble, 4, (46.0, 44.0), 30.0, (12.0, 6.0), 3.4, (1.4, 0.6), 2.0, r) {
                add(p);
            }
        }
    }

    fn step(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            p.vy += p.gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.size = (p.size - p.shrink * dt).max(0.0);
            p.angle += p.spin * dt;
        }
        self.particles.retain(|p| p.age < p.life && p.size > 0.3);
    }

    /// Works out the rod and hand for this frame. Same geometry as ArmView.aim, done with y
    /// pointing up and flipped to screen coordinates at the end.
    fn aim(&mut self, cursor_x: f32, cursor_y: f32, dt: f32, scale: f32) -> Vec<Draw> {
        let (w, h) = (self.width, self.height);
        if !self.fitted {
            self.hand.fit((HAND_LEN_FRAC * h * scale).round() as u32);
            self.fitted = true;
        }
        let cursor = (cursor_x, h - cursor_y);
        let frames = dt * 120.0;
        let target_x = (cursor.0 + GRIP_LEAN_FRAC * w).clamp(w * 0.15, w * 1.05);
        let follow = 1.0 - (1.0 - GRIP_FOLLOW).powf(frames);
        let gx = self.grip_x.map_or(target_x, |g| g + (target_x - g) * follow);
        self.grip_x = Some(gx);
        let grip = (gx, -GRIP_DEPTH_FRAC * h);

        let a = &self.hand;
        let (dx, dy) = (cursor.0 - grip.0, cursor.1 - grip.1);
        let reach = dx.hypot(dy);
        let aim_angle = dy.atan2(dx);
        let hand_pts = HAND_LEN_FRAC * h * (1.0 - 0.06 * self.press);
        let s = hand_pts / (a.wrist - a.hand_top);
        let width_pts = s * a.aspect;
        let hand_tip_pts = (a.wrist - a.tip) * s;
        let tip_off_x = (a.tip_x - a.rod_x) * width_pts;
        let along = (reach * reach - tip_off_x * tip_off_x).max(0.0).sqrt();
        let rod_len = (along - hand_tip_pts).max(MIN_ROD_FRAC * h);
        let tip_angle = (rod_len + hand_tip_pts).atan2(tip_off_x);
        let rotation = aim_angle - tip_angle;

        let (sin, cos) = rotation.sin_cos();
        let to_screen = |lx: f32, ly: f32| point(px(grip.0 + lx * cos - ly * sin), px(h - (grip.1 + lx * sin + ly * cos)));
        let quad_at = |x0: f32, x1: f32, y0: f32, y1: f32| [to_screen(x0, y0), to_screen(x1, y0), to_screen(x1, y1), to_screen(x0, y1)];

        let rod_w = a.rod_width * width_pts;
        let rod_top = rod_len + 0.04 * hand_pts;
        let rod = quad_at(-rod_w / 2.0, rod_w / 2.0, 0.0, rod_top);
        let sheen = quad_at(-rod_w * 0.16, rod_w * 0.04, 0.0, rod_top);

        let (image, fw, fh) = self.hand.rotated(rotation);
        let center = to_screen((0.5 - self.hand.rod_x) * width_pts, rod_len + hand_pts / 2.0);
        let (bw, bh) = (width_pts * fw, hand_pts * fh);
        let bounds = Bounds::new(
            point(center.x - px(bw / 2.0), center.y - px(bh / 2.0)),
            size(px(bw), px(bh)),
        );
        // Lit from one side like the Swift stick; a gradient runs across the rod.
        let across = (90.0 - rotation.to_degrees()).rem_euclid(360.0);
        vec![Draw::Rod(rod, sheen, across), Draw::Hand(bounds, image)]
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
    let o = (1.0 - p.age / p.life).clamp(0.0, 1.0).powf(0.8);
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
        let debug = std::env::var_os("TH_DEBUG").is_some();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        if self.quit_after.is_some_and(|s| self.started.elapsed() > Duration::from_secs_f32(s)) {
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
        if let Some((x, y)) = self.fake_mouse {
            mouse.x = x;
            mouse.y = y;
        }
        if let Some(every) = self.tap_every {
            if now.duration_since(self.last_auto_tap).as_secs_f32() > every {
                self.last_auto_tap = now;
                mouse.down = true;
            }
        }

        if debug {
            eprintln!("frame at {:.3}s mouse {:.0},{:.0} down={}", self.started.elapsed().as_secs_f32(), mouse.x, mouse.y, mouse.down);
        }
        if mouse.down && !self.was_down {
            self.press = 1.0;
            self.fairy_dust(mouse.x, mouse.y);
        } else if !mouse.down {
            self.press -= self.press * (1.0 - (1.0 - 0.18f32).powf(dt * 120.0));
        }
        self.was_down = mouse.down;
        self.step(dt);

        let mut draws = self.aim(mouse.x, mouse.y, dt, scale);
        draws.extend(self.particles.iter().map(|p| Draw::Particle(*p)));

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
