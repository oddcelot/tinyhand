//! Turning a frame's [`Draw`] list into GPUI paint calls.
//!
//! GPUI has no blur for paths or images, so the soft shadows of the Swift app are
//! approximated: the hand's shadow is a blurred silhouette made on the CPU (see
//! `overlay::shadow_image`), the stick's is a few offset, widening translucent copies,
//! the ink's an offset copy at the shadow opacity, and the laser's glow a stack of
//! translucent discs.

use std::sync::Arc;

use gpui::{
    BorderStyle, Bounds, Corners, Edges, Hsla, PathBuilder, Pixels, Point, Rgba, Window,
    linear_color_stop, linear_gradient, point, px, quad, size,
};
use gpui::{LineCap, LineJoin, PathStyle, StrokeOptions};

use tinyhand_core::ink::{self, Segment};

pub enum Draw {
    /// A finished or growing ink stroke.
    Ink { path: Vec<Segment>, barbs: Option<[ink::Point; 4]>, colour: u32, width: f32, opacity: f32 },
    /// The stick's shadow: its outline, the offset, and the opacity.
    RodShadow([Point<Pixels>; 4], (f32, f32), f32),
    /// Outline, highlight strip, and the angle (CSS degrees) across the stick for its shading.
    Rod([Point<Pixels>; 4], [Point<Pixels>; 4], f32),
    Image(Bounds<Pixels>, Arc<gpui::RenderImage>),
    /// The laser dot: centre, radius, glow radius.
    Laser { x: f32, y: f32, r: f32, glow: f32 },
}

pub fn rgba(hex: u32, alpha: f32) -> Hsla {
    let (r, g, b) = ink::rgb(hex);
    Rgba { r, g, b, a: alpha }.into()
}

fn white(alpha: f32) -> Hsla {
    Rgba { r: 1.0, g: 1.0, b: 1.0, a: alpha }.into()
}

fn black(alpha: f32) -> Hsla {
    Rgba { r: 0.0, g: 0.0, b: 0.0, a: alpha }.into()
}

fn pt(p: ink::Point) -> Point<Pixels> {
    point(px(p.0), px(p.1))
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

fn circle(cx: f32, cy: f32, d: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(cx - d / 2.0), px(cy - d / 2.0)), size(px(d), px(d)))
}

fn disc(window: &mut Window, cx: f32, cy: f32, r: f32, colour: Hsla) {
    window.paint_quad(quad(
        circle(cx, cy, 2.0 * r), Corners::all(px(r)), colour,
        Edges::all(px(0.0)), white(0.0), BorderStyle::Solid,
    ));
}

/// The stroke's path, shifted by `offset`: round caps and joins, like the Swift layer.
fn stroke_path(path: &[Segment], barbs: &Option<[ink::Point; 4]>, width: f32, offset: (f32, f32)) -> Option<gpui::Path> {
    let o = |p: ink::Point| pt((p.0 + offset.0, p.1 + offset.1));
    let mut b = PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(StrokeOptions {
        line_width: width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    }));
    // A zero-length stroke draws nothing; nudge it so the round caps make a dot.
    let mut last = None;
    for seg in path {
        match *seg {
            Segment::MoveTo(p) => {
                b.move_to(o(p));
                last = Some(p);
            }
            Segment::LineTo(p) => {
                let p = if last == Some(p) { (p.0 + 0.01, p.1) } else { p };
                b.line_to(o(p));
                last = Some(p);
            }
            Segment::QuadTo { ctrl, to } => {
                b.curve_to(o(to), o(ctrl));
                last = Some(to);
            }
        }
    }
    if let Some([a, b1, c, d]) = barbs {
        b.move_to(o(*a));
        b.line_to(o(*b1));
        b.move_to(o(*c));
        b.line_to(o(*d));
    }
    b.build().ok()
}

pub fn paint(draws: Vec<Draw>, window: &mut Window) {
    for draw in draws {
        match draw {
            Draw::Ink { path, barbs, colour, width, opacity } => {
                let (dx, dy) = ink::SHADOW_OFFSET;
                if let Some(shadow) = stroke_path(&path, &barbs, width + ink::SHADOW_RADIUS, (dx, dy)) {
                    window.paint_path(shadow, black(ink::SHADOW_OPACITY * opacity));
                }
                if let Some(line) = stroke_path(&path, &barbs, width, (0.0, 0.0)) {
                    window.paint_path(line, rgba(colour, opacity));
                }
            }
            Draw::RodShadow(rod, (dx, dy), opacity) => {
                // Three widening copies stand in for the blur.
                let centre = rod.iter().fold((0.0, 0.0), |a, p| (a.0 + f32::from(p.x) / 4.0, a.1 + f32::from(p.y) / 4.0));
                for (grow, share) in [(1.0, 0.45), (1.6, 0.3), (2.4, 0.25)] {
                    let pts = rod.map(|p| {
                        let (x, y) = (f32::from(p.x), f32::from(p.y));
                        // Widen across the stick only a little: scale about the centre.
                        let k = 1.0 + (grow - 1.0) * 0.08;
                        point(px(centre.0 + (x - centre.0) * k + dx), px(centre.1 + (y - centre.1) * k + dy))
                    });
                    if let Some(path) = polygon(&pts) {
                        window.paint_path(path, black(opacity * share));
                    }
                }
            }
            Draw::Rod(rod, sheen, across) => {
                let gray = |v: f32| Hsla::from(Rgba { r: v, g: v, b: v * 1.04, a: 1.0 });
                if let Some(path) = polygon(&rod) {
                    let body = linear_gradient(across, linear_color_stop(gray(0.24), 0.0), linear_color_stop(gray(0.02), 1.0));
                    window.paint_path(path, body);
                }
                if let Some(path) = polygon(&sheen) {
                    window.paint_path(path, white(0.22));
                }
            }
            Draw::Image(bounds, image) => {
                let _ = window.paint_image(bounds, bounds, Corners::default(), image, 0, false);
            }
            Draw::Laser { x, y, r, glow } => {
                // A glow fading out to `glow` around a red dot with a pale core (ArmView).
                let steps = 8;
                for i in 0..steps {
                    let t = i as f32 / steps as f32;
                    let radius = glow - (glow - r) * t;
                    disc(window, x, y, radius, rgba(ink::RED, 0.06 + 0.04 * t));
                }
                disc(window, x, y, r, rgba(ink::RED, 1.0));
                let core: Hsla = Rgba { r: 1.0, g: 0.86, b: 0.82, a: 0.95 }.into();
                disc(window, x, y, r * 0.45, core);
            }
        }
    }
}
