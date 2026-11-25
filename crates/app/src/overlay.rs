//! The overlay window: what is on screen while the pointer is on (ArmView.swift, Ink.swift,
//! TapEffect.swift and the tick of OverlayController.swift).
//!
//! The window's root view, [`OverlayView`], holds no state of its own; each frame asks the
//! [`Controller`] for the mouse, advances the [`Scene`] and paints it. Frames are asked
//! for only while something moves (120 Hz on most displays); otherwise a 20 Hz timer
//! keeps polling the mouse, as the Swift app's idle timer does.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    Bounds, Context, Entity, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, Task, Window, canvas, div, point, prelude::*, px, size,
};
use image::RgbaImage;

use tinyhand_core::custom_hands::CustomHands;
use tinyhand_core::hand::Rotated;
use tinyhand_core::ink::{self, Ink};
use tinyhand_core::particles::{Particles, TapEffect};
use tinyhand_core::styles::{self, Style, StyleKind};
use tinyhand_core::{Arm, Hand};
use tinyhand_platform as native;

use crate::controller::{Controller, Input, log};
use crate::paint::{self, Draw};
use crate::shift::Mods;

/// After the mouse stops for this long, frames drop to the idle rate.
const IDLE_AFTER: Duration = Duration::from_millis(500);
const IDLE_INTERVAL: Duration = Duration::from_millis(50);
/// Lift of the hand above the screen, as a fraction of its height (ArmView `liftFrac`).
const LIFT_FRAC: f32 = 0.06;
const SHADOW_OPACITY: f32 = 0.26;
/// Most rotations kept on the GPU before the cache starts over.
const MAX_IMAGES: usize = 240;

/// A hand image, fitted to its drawn size, with its rotations and shadows on the GPU.
struct Sprite {
    path: PathBuf,
    hand: Hand,
    /// (screen height, scale, size) the hand was fitted for.
    fitted: Option<(f32, f32, f32)>,
    images: HashMap<i32, Arc<gpui::RenderImage>>,
    /// Blurred silhouettes and their padding in device pixels.
    shadows: HashMap<i32, (Arc<gpui::RenderImage>, f32)>,
}

impl Sprite {
    fn load(path: PathBuf) -> Result<Sprite, String> {
        let hand = Hand::load(&path)?;
        Ok(Sprite { path, hand, fitted: None, images: HashMap::new(), shadows: HashMap::new() })
    }

    fn drop_images(&mut self, window: &mut Window) {
        for image in self.images.drain().map(|(_, i)| i).chain(self.shadows.drain().map(|(_, (i, _))| i)) {
            let _ = window.drop_image(image);
        }
    }

    /// Re-reads the image when the size it is drawn at changed, so a bigger hand stays sharp.
    fn fit(&mut self, key: (f32, f32, f32), window: &mut Window) {
        if self.fitted == Some(key) {
            return;
        }
        if self.fitted.is_some() {
            match Hand::load(&self.path) {
                Ok(hand) => self.hand = hand,
                Err(e) => log(&e),
            }
        }
        self.drop_images(window);
        self.hand.fit(Arm::hand_pixels(key.0, key.1, key.2));
        self.fitted = Some(key);
    }
}

/// A soft shadow for a rotated hand: its silhouette in black at the shadow opacity,
/// padded and blurred. GPUI can't blur, so this is done once per rotation on the CPU.
fn shadow_image(rotated: &Rotated, sigma: f32) -> (RgbaImage, f32) {
    let pad = (sigma * 2.5).ceil().max(1.0) as u32;
    let (w, h) = rotated.bgra.dimensions();
    let mut out = RgbaImage::new(w + 2 * pad, h + 2 * pad);
    for (x, y, p) in rotated.bgra.enumerate_pixels() {
        let a = (p[3] as f32 * SHADOW_OPACITY).round() as u8;
        out.put_pixel(x + pad, y + pad, image::Rgba([0, 0, 0, a]));
    }
    (image::imageops::fast_blur(&out, sigma.max(0.5)), pad as f32)
}

fn gpu(image: RgbaImage) -> Arc<gpui::RenderImage> {
    Arc::new(gpui::RenderImage::new(vec![image::Frame::new(image)]))
}

/// Everything drawn on the overlay, and the press/drag state behind it.
pub struct Scene {
    pub style: Style,
    sprite: Option<Sprite>,
    pub arm: Arm,
    pub ink: Ink,
    pub particles: Particles,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    /// Where the current press went down, while the button is held.
    down_point: Option<(f32, f32)>,
    button_seen_down: bool,
    pub held: bool,
    /// Capture mode: the left button as GPUI reports it.
    captured_down: bool,
    last_mouse: (f32, f32),
    still_since: Instant,
    last_frame: Instant,
    clock: Instant,
    last_auto_tap: Instant,
    auto_pressed: bool,
    /// Watch mode without a tap: the polled button, to see presses.
    polled_down: bool,
}

impl Scene {
    pub fn new(style_id: &str, custom: &CustomHands) -> Scene {
        let now = Instant::now();
        let mut scene = Scene {
            style: styles::find(styles::LASER, &[]).expect("the laser always exists"),
            sprite: None,
            arm: Arm::default(),
            ink: Ink::new(3.0),
            particles: Particles::default(),
            width: 1440.0,
            height: 900.0,
            scale: 1.0,
            down_point: None,
            button_seen_down: false,
            held: false,
            captured_down: false,
            last_mouse: (0.0, 0.0),
            still_since: now,
            last_frame: now,
            clock: now,
            last_auto_tap: now,
            auto_pressed: false,
            polled_down: false,
        };
        scene.set_style(style_id, custom);
        scene
    }

    /// Switches style; a hand whose image doesn't load becomes the laser dot (ArmView).
    pub fn set_style(&mut self, id: &str, custom: &CustomHands) {
        let Some(style) = styles::find(id, custom.hands()) else { return };
        self.sprite = None;
        self.style = style;
        if self.style.kind == StyleKind::Laser {
            return;
        }
        let loaded = crate::assets::hand_image(id, custom)
            .ok_or_else(|| format!("no image for {id}"))
            .and_then(Sprite::load);
        match loaded {
            Ok(sprite) => self.sprite = Some(sprite),
            Err(e) => {
                log(&format!("{e}; using the laser dot"));
                self.style = styles::find(styles::LASER, &[]).expect("the laser always exists");
            }
        }
    }

    fn now(&self) -> f32 {
        self.clock.elapsed().as_secs_f32()
    }

    /// Something happened: run at the busy rate for a while.
    pub fn wake(&mut self) {
        self.still_since = Instant::now();
    }

    pub fn press(&mut self, at: (f32, f32), effect: TapEffect, size: f32) {
        self.arm.press_now();
        self.held = true;
        self.down_point = Some(at);
        self.button_seen_down = false;
        let unit = self.height / 900.0 * size / 0.13;
        self.particles.burst(effect, at.0, at.1, unit);
        self.wake();
    }

    pub fn drag(&mut self, to: (f32, f32), colour: u32, arrow: bool) {
        let Some(start) = self.down_point else { return };
        if !self.ink.is_drawing() {
            if (to.0 - start.0).hypot(to.1 - start.1) < 4.0 {
                return;
            }
            self.ink.begin(start, colour, ink::ink_width(self.height));
        }
        self.ink.set_arrow(arrow);
        self.ink.extend(to);
        self.wake();
    }

    pub fn release(&mut self) {
        self.button_seen_down = false;
        self.held = false;
        self.down_point = None;
        let now = self.now();
        self.ink.end(now);
    }

    /// Still moving: the frame after this one is needed at full rate.
    fn busy(&self) -> bool {
        let now = self.now();
        let fading = self.ink.strokes.iter().any(|s| {
            s.ended_at.is_some_and(|e| self.ink.fade_seconds > 0.0 && now - e > self.ink.fade_delay() - 0.05)
        });
        self.still_since.elapsed() < IDLE_AFTER
            || self.held
            || self.arm.is_settling()
            || self.ink.is_drawing()
            || fading
            || !self.particles.is_empty()
    }

    fn draws(&mut self, cursor: (f32, f32), dt: f32, show_shadow: bool, subtle: bool, pointer_hidden: bool, window: &mut Window) -> Vec<Draw> {
        let now = self.now();
        let mut draws: Vec<Draw> = self
            .ink
            .strokes
            .iter()
            .map(|s| Draw::Ink {
                path: s.path(),
                barbs: s.arrow.then(|| s.arrowhead()).flatten().map(|[(a, b), (c, d)]| [a, b, c, d]),
                colour: s.colour,
                width: s.width,
                opacity: self.ink.opacity(s, now),
            })
            .collect();

        let (w, h, p) = (self.width, self.height, self.arm.press);
        if self.style.kind == StyleKind::Laser || self.sprite.is_none() {
            self.arm.follow(cursor.0, w, dt);
            if !pointer_hidden {
                let r = (h * 0.0038 * self.arm.size / 0.13).max(4.0) * (1.0 + 0.2 * p);
                draws.push(Draw::Laser { x: cursor.0, y: cursor.1, r, glow: r * (2.2 + p) });
            }
        } else if let Some(sprite) = self.sprite.as_mut() {
            sprite.fit((h, self.scale, self.arm.size), window);
            let pose = self.arm.aim(&sprite.hand, cursor, (w, h), dt);
            if !pointer_hidden {
                let (key, rotated) = sprite.hand.rotated(pose.rotation);
                if sprite.images.len() > MAX_IMAGES {
                    sprite.drop_images(window);
                }
                let image = sprite.images.entry(key).or_insert_with(|| gpu(rotated.bgra.clone())).clone();
                let (bw, bh) = (pose.hand_size.0 * rotated.width_factor, pose.hand_size.1 * rotated.height_factor);
                let bounds = Bounds::new(
                    point(px(pose.hand_center.0 - bw / 2.0), px(pose.hand_center.1 - bh / 2.0)),
                    size(px(bw), px(bh)),
                );
                let pts = |q: [(f32, f32); 4]| q.map(|(x, y)| point(px(x), px(y)));
                if show_shadow {
                    // ArmView: lifted 0.06 H, less while pressed; offset right and down.
                    let lift = LIFT_FRAC * h * (1.0 - 0.8 * p);
                    let offset = (0.6 * lift, lift);
                    let rest_radius = (LIFT_FRAC * h * 0.35).max(1.0);
                    let sigma = rest_radius * self.scale / 2.0;
                    let (shadow, pad) = sprite
                        .shadows
                        .entry(key)
                        .or_insert_with(|| {
                            let (img, pad) = shadow_image(&rotated, sigma);
                            (gpu(img), pad)
                        })
                        .clone();
                    let pad_pts = pad * bw / rotated.bgra.width() as f32;
                    let shadow_bounds = Bounds::new(
                        point(px(f32::from(bounds.origin.x) - pad_pts + offset.0), px(f32::from(bounds.origin.y) - pad_pts + offset.1)),
                        size(px(bw + 2.0 * pad_pts), px(bh + 2.0 * pad_pts)),
                    );
                    draws.push(Draw::RodShadow(pts(pose.rod), offset, SHADOW_OPACITY));
                    draws.push(Draw::Image(shadow_bounds, shadow));
                }
                draws.push(Draw::Rod(pts(pose.rod), pts(pose.sheen), pose.rod_shade_angle));
                draws.push(Draw::Image(bounds, image));
            }
        }
        draws.extend(self.particles.live.iter().map(|p| Draw::Particle(*p, subtle)));
        draws
    }
}

impl Controller {
    /// One frame (`OverlayController.tick` plus `ArmView.aim`): read the mouse, follow it
    /// to another display, press/drag/release, update the cursor, and list what to paint.
    /// Returns the draws and whether the next frame is needed at full rate.
    pub fn frame(&mut self, window: &mut Window, cx: &mut Context<Self>) -> (Vec<Draw>, bool) {
        let now = Instant::now();
        let dt = now.duration_since(self.scene.last_frame).as_secs_f32().min(0.05);
        self.scene.last_frame = now;
        self.scene.scale = window.scale_factor();
        let viewport = window.viewport_size();
        if f32::from(viewport.height) > 0.0 {
            self.scene.width = f32::from(viewport.width);
            self.scene.height = f32::from(viewport.height);
        }
        self.scene.arm.size = self.settings.pointer_size;
        self.scene.ink.fade_seconds = self.settings.fade_seconds as f32;
        let mut global = None;

        let mut mouse = match self.input {
            Input::Capture => {
                let p = window.mouse_position();
                let m = window.modifiers();
                let mods = Mods { shift: m.shift, control: m.control, alt: m.alt, command: m.platform };
                if mods != self.mods {
                    self.modifiers_changed(mods, cx);
                }
                (f32::from(p.x), f32::from(p.y))
            }
            Input::Watch => {
                let fake = self.hooks.fake_mouse.is_some();
                let at = match (native::cursor_global(), self.display) {
                    (Some(g), Some(d)) if !fake => {
                        global = Some(g);
                        if let Some(under) = native::display_at(g.0, g.1) {
                            self.follow_display(under, cx);
                        }
                        ((g.0 - d.x) as f32, (g.1 - d.y) as f32)
                    }
                    _ => {
                        let m = native::mouse(self.scene.height, self.scene.scale);
                        (m.x, m.y)
                    }
                };
                // Without the tap, presses are only watched (they reach the app too).
                if !fake && !self.has_tap() {
                    let down = native::mouse(self.scene.height, self.scene.scale).down;
                    if down && !self.scene.polled_down {
                        self.press(at);
                    } else if !down && self.scene.polled_down {
                        self.scene.release();
                    }
                    self.scene.polled_down = down;
                }
                at
            }
        };
        if let Some(fake) = self.hooks.fake_mouse {
            mouse = fake;
        }
        if let Some(every) = self.hooks.tap_every {
            if self.scene.auto_pressed {
                self.scene.auto_pressed = false;
                self.scene.release();
            } else if now.duration_since(self.scene.last_auto_tap).as_secs_f32() > every {
                self.scene.last_auto_tap = now;
                self.scene.auto_pressed = true;
                self.press(mouse);
            }
        }
        if mouse != self.scene.last_mouse {
            self.scene.last_mouse = mouse;
            self.scene.wake();
        }
        if self.hooks.debug {
            eprintln!(
                "frame at {:.3}s mouse {:.0},{:.0} down={} window {:?}",
                self.scene.clock.elapsed().as_secs_f32(), mouse.0, mouse.1, self.scene.held, window.bounds()
            );
        }

        if self.input == Input::Watch && self.hooks.fake_mouse.is_none() {
            self.update_zones(global);
            self.check_lock(cx);
        }

        // Watch mode: a release can be missed, so the button state ends the press too.
        if self.scene.down_point.is_some() && self.input == Input::Watch && self.has_tap() {
            let down = native::input::left_button_down();
            if down {
                self.scene.button_seen_down = true;
            }
            if self.scene.button_seen_down && !down {
                self.scene.release();
            } else {
                self.drag(mouse);
            }
        } else if self.scene.down_point.is_some() && self.input == Input::Capture && self.scene.captured_down {
            self.drag(mouse);
        }
        if !self.scene.held {
            self.scene.arm.ease_press(dt);
        }
        self.scene.particles.step(dt);
        let t = self.scene.now();
        self.scene.ink.prune(t);

        let show_shadow = self.settings.show_shadow;
        let subtle = self.settings.tap_effect == TapEffect::FairyDustSubtle;
        let hidden = self.normal_cursor;
        let draws = self.scene.draws(mouse, dt, show_shadow, subtle, hidden, window);
        let busy = self.scene.busy() || self.cursor_settling();
        (draws, busy)
    }

    /// Capture mode: GPUI's own mouse events.
    fn captured_down(&mut self, at: (f32, f32)) {
        self.scene.captured_down = true;
        self.press(at);
    }

    fn captured_up(&mut self) {
        self.scene.captured_down = false;
        self.scene.release();
    }
}

pub struct OverlayView {
    controller: Entity<Controller>,
    focus: FocusHandle,
    idle_timer: Option<Task<()>>,
}

impl OverlayView {
    pub fn new(controller: Entity<Controller>, focus: FocusHandle) -> Self {
        OverlayView { controller, focus, idle_timer: None }
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (draws, busy) = self.controller.update(cx, |c, cx| {
            if c.active { c.frame(window, cx) } else { (Vec::new(), false) }
        });
        if busy {
            window.request_animation_frame();
        } else if self.idle_timer.is_none() {
            self.idle_timer = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(IDLE_INTERVAL).await;
                let _ = this.update(cx, |view, cx| {
                    view.idle_timer = None;
                    cx.notify();
                });
            }));
        }

        let painting = canvas(|_, _, _| {}, move |_, _, window, _| paint::paint(draws, window)).size_full();

        // Only reached in capture mode: in watch mode the window never gets the mouse.
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                let at = (f32::from(e.position.x), f32::from(e.position.y));
                this.controller.update(cx, |c, _| c.captured_down(at));
                // So Esc reaches the overlay where the system allows it.
                window.focus(&this.focus, cx);
                window.refresh();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| {
                this.controller.update(cx, |c, _| c.captured_up());
                window.refresh();
            }))
            .on_mouse_move(cx.listener(|_, _, window, _| window.refresh()))
            // The overlay takes every click, so there has to be a way out without the tray.
            .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                this.controller.update(cx, |c, cx| c.exit(cx));
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.controller.update(cx, |c, cx| c.exit(cx));
                }
            }))
            .child(painting)
    }
}
