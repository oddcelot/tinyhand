//! A model of drawing on screen, free of any UI: strokes, their smoothed paths, arrowheads
//! and fading, as in Ink.swift. Coordinates are screen points with y pointing down.

use std::f32::consts::PI;

/// The default pen colour, 0xRRGGBB (Swift sRGB 0.53, 0.33, 0.95).
pub const PURPLE: u32 = 0x8754F2;
/// The other pen colour, 0xRRGGBB (Swift sRGB 1.0, 0.16, 0.12).
pub const RED: u32 = 0xFF291F;

/// A stroke's shadow is black at this opacity, blurred by `SHADOW_RADIUS`.
pub const SHADOW_OPACITY: f32 = 0.18;
pub const SHADOW_RADIUS: f32 = 1.5;
/// Swift (y up) offsets the shadow by (1, -1.5), meaning right and down. In our y-down
/// coordinates that is (1, +1.5).
pub const SHADOW_OFFSET: (f32, f32) = (1.0, 1.5);

/// Seconds a finished stroke takes to fade out, after its wait.
pub const FADE_DURATION: f32 = 0.6;
/// Shortest wait before a stroke starts to fade.
pub const MIN_FADE_DELAY: f32 = 0.5;
/// A new point must be at least this far from the last one.
pub const MIN_POINT_GAP: f32 = 1.5;

pub type Point = (f32, f32);

/// Turns 0xRRGGBB into red, green, blue in 0..1.
pub fn rgb(hex: u32) -> (f32, f32, f32) {
    (
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    )
}

/// The pen width for a screen of this height (from ArmView).
pub fn ink_width(screen_height: f32) -> f32 {
    (screen_height * 0.0055).max(3.0)
}

/// One step of a drawn path.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Segment {
    MoveTo(Point),
    QuadTo { ctrl: Point, to: Point },
    LineTo(Point),
}

#[derive(Clone, Debug)]
pub struct Stroke {
    pub points: Vec<Point>,
    pub colour: u32,
    pub width: f32,
    pub arrow: bool,
    /// When the pen was lifted, in seconds on the caller's clock. `None` while drawing.
    pub ended_at: Option<f32>,
}

fn dist(a: Point, b: Point) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

impl Stroke {
    pub fn new(point: Point, colour: u32, width: f32) -> Self {
        Stroke { points: vec![point], colour, width, arrow: false, ended_at: None }
    }

    /// Adds a point unless it is within 1.5 pt of the last one.
    pub fn add_point(&mut self, point: Point) {
        if let Some(&last) = self.points.last() {
            if dist(point, last) < MIN_POINT_GAP {
                return;
            }
        }
        self.points.push(point);
    }

    /// The smoothed path: quadratic curves through the midpoints of the points, then a
    /// line to the last point. One point gives a zero-length line (a dot).
    pub fn path(&self) -> Vec<Segment> {
        let Some(&first) = self.points.first() else {
            return Vec::new();
        };
        let mut path = vec![Segment::MoveTo(first)];
        if self.points.len() == 1 {
            path.push(Segment::LineTo(first));
        }
        for i in 1..self.points.len() {
            let (prev, p) = (self.points[i - 1], self.points[i]);
            let mid = ((prev.0 + p.0) / 2.0, (prev.1 + p.1) / 2.0);
            path.push(Segment::QuadTo { ctrl: prev, to: mid });
        }
        if self.points.len() > 1 {
            path.push(Segment::LineTo(*self.points.last().unwrap()));
        }
        path
    }

    /// Size of the arrowhead's barbs.
    pub fn arrow_size(&self) -> f32 {
        (self.width * 3.2).max(14.0)
    }

    /// The two barbs as (start, tip) lines, both starting at the stroke's end. Aimed along
    /// the last stretch of the stroke so a wobble at the very end does not twist the head.
    /// `None` if the stroke is too short to aim.
    pub fn arrowhead(&self) -> Option<[(Point, Point); 2]> {
        let end = *self.points.last()?;
        let size = self.arrow_size();
        let back = self
            .points
            .iter()
            .rev()
            .find(|&&p| dist(end, p) >= size * 1.2)
            .copied()
            .unwrap_or(self.points[0]);
        let length = dist(end, back);
        if length <= 1.0 {
            return None;
        }
        let head = size.min(length * 0.8);
        let angle = (end.1 - back.1).atan2(end.0 - back.0);
        let spread = PI / 7.0;
        let barb = |side: f32| {
            let a = angle + PI + side * spread;
            (end, (end.0 + a.cos() * head, end.1 + a.sin() * head))
        };
        Some([barb(-1.0), barb(1.0)])
    }
}

/// Solves CAMediaTimingFunction easeIn, the cubic bezier (0.42, 0, 1, 1): finds the curve
/// parameter whose x is `t` by bisection, and returns its y. Exact to about 1e-6.
pub fn ease_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) as f64;
    let bez = |a: f64, b: f64, s: f64| 3.0 * a * s * (1.0 - s).powi(2) + 3.0 * b * s * s * (1.0 - s) + s.powi(3);
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if bez(0.42, 1.0, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(0.0, 1.0, (lo + hi) / 2.0) as f32
}

#[derive(Clone, Debug, Default)]
pub struct Ink {
    pub strokes: Vec<Stroke>,
    /// Seconds before finished strokes fade. 0 or less keeps them forever.
    pub fade_seconds: f32,
}

impl Ink {
    pub fn new(fade_seconds: f32) -> Self {
        Ink { strokes: Vec::new(), fade_seconds }
    }

    /// The stroke being drawn, if the pen is down.
    pub fn current(&mut self) -> Option<&mut Stroke> {
        self.strokes.last_mut().filter(|s| s.ended_at.is_none())
    }

    pub fn is_drawing(&self) -> bool {
        self.strokes.last().is_some_and(|s| s.ended_at.is_none())
    }

    /// Starts a new stroke (without an arrow).
    pub fn begin(&mut self, point: Point, colour: u32, width: f32) {
        self.strokes.push(Stroke::new(point, colour, width));
    }

    pub fn extend(&mut self, point: Point) {
        if let Some(s) = self.current() {
            s.add_point(point);
        }
    }

    /// Turns the arrowhead on or off for the stroke being drawn.
    pub fn set_arrow(&mut self, arrow: bool) {
        if let Some(s) = self.current() {
            s.arrow = arrow;
        }
    }

    /// Lifts the pen. The stroke keeps its arrow, as in Swift.
    pub fn end(&mut self, now: f32) {
        if let Some(s) = self.current() {
            s.ended_at = Some(now);
        }
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
    }

    /// Seconds a finished stroke waits before it starts to fade.
    pub fn fade_delay(&self) -> f32 {
        self.fade_seconds.max(MIN_FADE_DELAY)
    }

    /// How opaque a stroke is at `now`: 1 while drawing or waiting, then fading to 0 along
    /// the ease-in curve over 0.6 s. Always 1 if `fade_seconds` is 0 or less.
    pub fn opacity(&self, stroke: &Stroke, now: f32) -> f32 {
        let Some(ended) = stroke.ended_at else { return 1.0 };
        if self.fade_seconds <= 0.0 {
            return 1.0;
        }
        let t = (now - ended - self.fade_delay()) / FADE_DURATION;
        1.0 - ease_in(t)
    }

    /// Removes strokes that have faded out completely.
    pub fn prune(&mut self, now: f32) {
        if self.fade_seconds <= 0.0 {
            return;
        }
        let gone = self.fade_delay() + FADE_DURATION;
        self.strokes.retain(|s| s.ended_at.is_none_or(|e| now - e < gone));
    }
}
