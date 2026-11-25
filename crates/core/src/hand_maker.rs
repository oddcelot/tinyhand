//! Turns a user's generated picture into a TinyHand hand: the prompt to ask an image tool
//! with, checks that the picture is usable, the 1024x1536 hand-on-a-stick image TinyHand
//! draws, and the pen colour to match it. Ports HandMaker.swift.

use image::{ColorType, ImageFormat, Rgba, RgbaImage, imageops};
use std::collections::BTreeMap;
use std::fmt;

const PLACEHOLDER: &str = "[describe your hand, e.g. knitted wool in mustard yellow]";
const DEFAULT_IDEA: &str = "a soft vinyl toy in a bright colour";

/// The text to give an image tool. `[describe your hand, ...]` is where the idea goes.
pub const PROMPT: &str = "Make a pointer image for TinyHand, a Mac app that shows a toy hand on screen.

Draw one cartoon right hand in this style: [describe your hand, e.g. knitted wool in mustard yellow]

Rules:
- We see the BACK of the hand. The palm faces away from us. The knuckles and the smooth back of the hand face the viewer.
- The index finger points straight up. The other three fingers are folded down, so we only see their knuckles, never their fingertips or fingernails.
- The raised index finger sits at the far left of the knuckle row, right next to the thumb. Three folded knuckles sit to its right. It must never look like a middle finger.
- The thumb is tucked against the left side of the hand. Nothing crosses the front of the fist: no thumb, no fingertips, no fingernails, no palm.
- The fingertip is the highest point of the image. Nothing above or beside it.
- The hand ends at the bottom in a wrist cuff or a clean flat wrist, facing straight down, centred.
- Upright, not tilted. Whole hand visible, nothing cropped.
- Transparent background. No shadow, no floor, no outline glow, no text, no stick, no arm.
- Soft 3D toy look, bright and readable at small sizes.
- Portrait image, hand filling most of the height with a little empty space around it.";

/// The prompt with the user's idea filled in (or a default if it is blank).
pub fn prompt_for(idea: &str) -> String {
    let idea = idea.trim();
    let style = if idea.is_empty() { DEFAULT_IDEA } else { idea };
    PROMPT.replace(PLACEHOLDER, style)
}

/// Percent-encodes everything except letters, digits and `-._~`.
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The ChatGPT address that opens with the prompt already typed in.
pub fn chatgpt_url(idea: &str) -> String {
    format!("https://chatgpt.com/?q={}", percent_encode(&prompt_for(idea)))
}

/// Why a picture can't become a hand. `Display` gives the text to show the user.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MakeError {
    Unreadable,
    /// A readable image, but not a PNG (only PNG decoding is built in).
    NotPng,
    NoAlpha,
    TooSmall,
    Edge,
    MoreThanOne,
    Empty,
    NotUpright,
    HandTooSmall,
    NoFinger,
    NoWrist,
    SoftShadow,
}

impl fmt::Display for MakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MakeError::Unreadable => "That file isn't an image TinyHand can read.",
            MakeError::NotPng => "TinyHand can only read PNG images for now. Save the hand as a PNG with transparency.",
            MakeError::NoAlpha => "The image needs a transparent background. Save it as a PNG with transparency.",
            MakeError::TooSmall => "The image is too small. Use one at least 512 pixels tall.",
            MakeError::Edge => "The background isn't transparent, or the hand touches the edge of the image.",
            MakeError::MoreThanOne => "The image has more than one thing in it. Use a single hand.",
            MakeError::Empty => "The image is empty.",
            MakeError::NotUpright => "The hand should stand upright, taller than it is wide.",
            MakeError::HandTooSmall => "The hand is too small in the image. Crop it closer.",
            MakeError::NoFinger => "The top of the image should be a single finger pointing up.",
            MakeError::NoWrist => "The hand should end in a flat wrist or cuff at the bottom.",
            MakeError::SoftShadow => "The hand has a soft shadow or glow around it. Ask for no shadow.",
        })
    }
}

impl std::error::Error for MakeError {}

/// Decodes a PNG and refuses pictures with no alpha channel (an `RgbaImage` always has
/// one, so this is the only place that can tell). Other formats are refused with
/// `NotPng`, because only PNG decoding is built into the image crate here.
pub fn load_image(bytes: &[u8]) -> Result<RgbaImage, MakeError> {
    match image::guess_format(bytes) {
        Ok(ImageFormat::Png) => {}
        Ok(_) => return Err(MakeError::NotPng),
        Err(_) => return Err(MakeError::Unreadable),
    }
    let decoded = image::load_from_memory_with_format(bytes, ImageFormat::Png).map_err(|_| MakeError::Unreadable)?;
    if !ColorType::has_alpha(decoded.color()) {
        return Err(MakeError::NoAlpha);
    }
    Ok(decoded.into_rgba8())
}

fn alpha(img: &RgbaImage, x: u32, y: u32) -> u8 {
    img.get_pixel(x, y)[3]
}

/// Smallest box around the pixels with alpha > 128: (min_x, min_y, max_x, max_y).
fn bounds(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 128 {
            b = Some(match b {
                None => (x, y, x, y),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x), e.max(y)),
            });
        }
    }
    b
}

/// Width and centre column of the hand's bottom 4%.
fn bottom_span(img: &RgbaImage, b: (u32, u32, u32, u32)) -> (u32, f64) {
    let (min_x, min_y, max_x, max_y) = b;
    let bh = max_y - min_y + 1;
    let (mut lo, mut hi) = (img.width(), None::<u32>);
    for y in min_y.max(max_y.saturating_sub((bh * 4 / 100).max(2)))..=max_y {
        for x in min_x..=max_x {
            if alpha(img, x, y) > 128 {
                lo = lo.min(x);
                hi = Some(hi.map_or(x, |h| h.max(x)));
            }
        }
    }
    match hi {
        Some(hi) => (hi - lo + 1, (lo + hi) as f64 / 2.0),
        None => (0, (min_x + max_x) as f64 / 2.0),
    }
}

/// Runs every check, in order, and returns the first problem found.
pub fn validate(img: &RgbaImage) -> Result<(), MakeError> {
    let (w, h) = (img.width(), img.height());
    if w < 64 || h < 64 {
        return Err(MakeError::TooSmall);
    }

    // Edge pixels must be (almost) all transparent.
    let (mut edge, mut edge_solid) = (0u32, 0u32);
    for x in 0..w {
        edge += 2;
        edge_solid += (alpha(img, x, 0) > 32) as u32 + (alpha(img, x, h - 1) > 32) as u32;
    }
    for y in 0..h {
        edge += 2;
        edge_solid += (alpha(img, 0, y) > 32) as u32 + (alpha(img, w - 1, y) > 32) as u32;
    }
    if edge_solid as f64 / edge as f64 > 0.02 {
        return Err(MakeError::Edge);
    }

    // One connected piece (on a coarse grid) must hold at least 97% of the solid cells.
    let step = (w.min(h) / 256).max(1);
    let (gw, gh) = ((w / step) as usize, (h / step) as usize);
    let mut grid = vec![false; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            grid[gy * gw + gx] = alpha(img, gx as u32 * step, gy as u32 * step) > 128;
        }
    }
    let mut seen = vec![false; gw * gh];
    let mut sizes = Vec::new();
    for start in 0..grid.len() {
        if !grid[start] || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let mut n = 0usize;
        while let Some(i) = stack.pop() {
            n += 1;
            let (x, y) = ((i % gw) as i64, (i / gw) as i64);
            for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if nx >= 0 && ny >= 0 && (nx as usize) < gw && (ny as usize) < gh {
                    let j = ny as usize * gw + nx as usize;
                    if grid[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        sizes.push(n);
    }
    let Some(&biggest) = sizes.iter().max() else {
        return Err(MakeError::Empty);
    };
    if (biggest as f64) / (sizes.iter().sum::<usize>() as f64) < 0.97 {
        return Err(MakeError::MoreThanOne);
    }

    let b = bounds(img).ok_or(MakeError::Empty)?;
    let (min_x, min_y, max_x, max_y) = b;
    let (bw, bh) = (max_x - min_x + 1, max_y - min_y + 1);
    let aspect = bh as f64 / bw as f64;
    if !(1.05..=2.8).contains(&aspect) {
        return Err(MakeError::NotUpright);
    }
    if (bh as f64) / (h as f64) < 0.5 {
        return Err(MakeError::HandTooSmall);
    }

    // The top 8% must be one narrow finger.
    let top_band = (bh * 8 / 100).max(2);
    let (mut finger_min, mut finger_max) = (w, 0);
    for y in min_y..(min_y + top_band) {
        for x in min_x..=max_x {
            if alpha(img, x, y) > 128 {
                finger_min = finger_min.min(x);
                finger_max = finger_max.max(x);
            }
        }
    }
    if (finger_max as f64 - finger_min as f64 + 1.0) / bw as f64 > 0.4 {
        return Err(MakeError::NoFinger);
    }

    if (bottom_span(img, b).0 as f64) / (bw as f64) < 0.2 {
        return Err(MakeError::NoWrist);
    }

    // Soft edges (shadow, glow) show up as many half-transparent pixels.
    let (mut soft, mut any) = (0u32, 0u32);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let a = alpha(img, x, y);
            if a > 20 {
                any += 1;
                if a < 200 {
                    soft += 1;
                }
            }
        }
    }
    if soft as f64 / any.max(1) as f64 > 0.08 {
        return Err(MakeError::SoftShadow);
    }
    Ok(())
}

pub const CANVAS_W: u32 = 1024;
pub const CANVAS_H: u32 = 1536;

/// Puts the hand on a stick: scaled so the hand is 560 px tall (at most 700 wide) with its
/// top at y=150, the wrist centred, and a dark rod (a capsule with a five-stop horizontal
/// gradient) running from just inside the wrist down to y=1462. The hand is resized with
/// Catmull-Rom, on premultiplied pixels so edges stay clean.
pub fn compose(img: &RgbaImage) -> RgbaImage {
    let mut canvas = RgbaImage::new(CANVAS_W, CANVAS_H);
    let Some(b) = bounds(img) else { return canvas };
    let (min_x, min_y, max_x, max_y) = b;
    let bw = (max_x - min_x + 1) as f32;
    let bh = (max_y - min_y + 1) as f32;
    let (hand_top, hand_height) = (150.0f32, 560.0f32);
    let scale = (hand_height / bh).min(700.0 / bw);
    let draw_w = ((img.width() as f32 * scale).round() as u32).max(1);
    let draw_h = ((img.height() as f32 * scale).round() as u32).max(1);
    let wrist_x = (bottom_span(img, b).1 + 0.5) as f32 * scale;
    let origin_x = CANVAS_W as f32 / 2.0 - wrist_x;
    let origin_top = hand_top - min_y as f32 * scale;
    let hand_bottom = hand_top + bh * scale;

    // The rod.
    let rod_w = (bw * scale * 0.18).clamp(54.0, 76.0);
    let rod_top = hand_bottom - 30.0;
    let rod_bottom = 1462.0f32;
    let rod_left = CANVAS_W as f32 / 2.0 - rod_w / 2.0;
    let radius = rod_w / 2.0;
    let stops: [(f32, [f32; 3]); 5] = [
        (0.0, [0x00 as f32, 0x00 as f32, 0x00 as f32]),
        (0.22, [0x12 as f32, 0x12 as f32, 0x14 as f32]),
        (0.45, [0x3A as f32, 0x3A as f32, 0x3E as f32]),
        (0.75, [0x12 as f32, 0x12 as f32, 0x14 as f32]),
        (1.0, [0x00 as f32, 0x00 as f32, 0x00 as f32]),
    ];
    let gradient = |t: f32| -> [f32; 3] {
        let t = t.clamp(0.0, 1.0);
        for pair in stops.windows(2) {
            let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
            if t <= t1 {
                let k = (t - t0) / (t1 - t0);
                return [0, 1, 2].map(|i| c0[i] + (c1[i] - c0[i]) * k);
            }
        }
        stops[4].1
    };
    let (seg_top, seg_bottom) = (rod_top + radius, rod_bottom - radius);
    for y in (rod_top.floor().max(0.0) as u32)..(rod_bottom.ceil() as u32).min(CANVAS_H) {
        for x in (rod_left.floor().max(0.0) as u32)..((rod_left + rod_w).ceil() as u32).min(CANVAS_W) {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let cy = py.clamp(seg_top, seg_bottom.max(seg_top));
            let d = (px - CANVAS_W as f32 / 2.0).hypot(py - cy) - radius;
            let coverage = (0.5 - d).clamp(0.0, 1.0);
            if coverage > 0.0 {
                let c = gradient((px - rod_left) / rod_w);
                canvas.put_pixel(x, y, Rgba([c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, (coverage * 255.0).round() as u8]));
            }
        }
    }

    // The hand, over the rod.
    let mut pre = img.clone();
    for p in pre.pixels_mut() {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = ((p[c] as u32 * a + 127) / 255) as u8;
        }
    }
    let mut hand = imageops::resize(&pre, draw_w, draw_h, imageops::FilterType::CatmullRom);
    let (left, top) = (origin_x.round() as i64, origin_top.round() as i64);
    for (x, y, p) in hand.enumerate_pixels_mut() {
        let (cx, cy) = (left + x as i64, top + y as i64);
        if cx < 0 || cy < 0 || cx >= CANVAS_W as i64 || cy >= CANVAS_H as i64 || p[3] == 0 {
            continue;
        }
        // `p` is premultiplied; the canvas is straight.
        let sa = p[3] as f32 / 255.0;
        let dst = *canvas.get_pixel(cx as u32, cy as u32);
        let da = dst[3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        let mix = |s: u8, d: u8| ((s as f32 + d as f32 * da * (1.0 - sa)) / out_a).round().clamp(0.0, 255.0) as u8;
        canvas.put_pixel(cx as u32, cy as u32, Rgba([mix(p[0], dst[0]), mix(p[1], dst[1]), mix(p[2], dst[2]), (out_a * 255.0).round() as u8]));
    }
    canvas
}

/// The pen colour (0xRRGGBB) that goes with a hand: the most common saturated, mid-bright
/// colour. Samples every third pixel with alpha > 200 into 6x6x6 colour buckets, scores
/// each bucket as `count * (0.25 + saturation)` (x0.15 if too dark or light), falls back
/// to grey-blue if the winner is nearly grey, and caps the brightest channel at 0.82.
pub fn ink_colour(img: &RgbaImage) -> u32 {
    // Buckets in key order, so ties always go the same way.
    let mut buckets: BTreeMap<usize, (u32, f64, f64, f64)> = BTreeMap::new();
    for y in (0..img.height()).step_by(3) {
        for x in (0..img.width()).step_by(3) {
            let p = img.get_pixel(x, y).0;
            if p[3] <= 200 {
                continue;
            }
            let (r, g, b) = (p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0);
            let key = (r * 5.0) as usize * 36 + (g * 5.0) as usize * 6 + (b * 5.0) as usize;
            let e = buckets.entry(key).or_default();
            e.0 += 1;
            e.1 += r;
            e.2 += g;
            e.3 += b;
        }
    }
    let mut best = (-1.0f64, 0.53f64, 0.33f64, 0.95f64);
    for &(n, sr, sg, sb) in buckets.values() {
        let n = n as f64;
        let (r, g, b) = (sr / n, sg / n, sb / n);
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        let sat = if hi == 0.0 { 0.0 } else { (hi - lo) / hi };
        let lum = 0.3 * r + 0.59 * g + 0.11 * b;
        let score = n * (0.25 + sat) * if lum > 0.12 && lum < 0.85 { 1.0 } else { 0.15 };
        if score > best.0 {
            best = (score, r, g, b);
        }
    }
    let (_, mut r, mut g, mut b) = best;
    let hi = r.max(g).max(b);
    let lo = r.min(g).min(b);
    if hi - lo < 0.08 {
        (r, g, b) = (0.42, 0.45, 0.5);
    } else if hi > 0.82 {
        let k = 0.82 / hi;
        (r, g, b) = (r * k, g * k, b * k);
    }
    let ch = |v: f64| (v * 255.0).min(255.0) as u32;
    ch(r) << 16 | ch(g) << 8 | ch(b)
}

/// A name for a new hand from the picture's file name: the first two words, with `_` and
/// `-` as spaces and the first letter capitalised. `None` (use a default name) if that is
/// empty, over 16 characters, or looks like a ChatGPT download ("ChatGPT Image ...").
pub fn suggested_name(file_name: &str) -> Option<String> {
    let stem = std::path::Path::new(file_name).file_stem()?.to_string_lossy().replace(['_', '-'], " ");
    let words: Vec<&str> = stem.trim_matches([' ', '\t']).split(' ').filter(|w| !w.is_empty()).take(2).collect();
    let name = words.join(" ");
    if name.is_empty() || name.chars().count() > 16 || name.to_lowercase().starts_with("chatgpt image") {
        return None;
    }
    let mut chars = name.chars();
    let first: String = chars.next()?.to_uppercase().collect();
    Some(first + chars.as_str())
}

/// Reads, checks and composes a picture, and finds its pen colour.
pub fn make(bytes: &[u8]) -> Result<(RgbaImage, u32), MakeError> {
    let img = load_image(bytes)?;
    validate(&img)?;
    Ok((compose(&img), ink_colour(&img)))
}
