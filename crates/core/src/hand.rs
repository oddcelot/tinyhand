//! Loads a TinyHand hand image and measures it the way `HandAsset.measure` does in the
//! Swift app, then hands out copies of the hand rotated to any angle.
//!
//! GPUI can rotate SVG icons but not raster images, so the rotation happens here on the
//! CPU. Each angle is made once (in half-degree steps) and kept.

use image::{RgbaImage, imageops};
use std::collections::HashMap;
use std::sync::Arc;

/// A rotated hand ready to draw: pixels in BGRA order (as GPUI wants them) and the size
/// of its bounding box as a multiple of the unrotated hand's width and height.
pub struct Rotated {
    pub bgra: RgbaImage,
    pub width_factor: f32,
    pub height_factor: f32,
}

pub struct Hand {
    /// The hand only (from just above the fingertip down to the wrist), scaled to the
    /// size it is drawn at, in device pixels.
    crop: RgbaImage,
    /// Fractions of the full source image, as in the Swift `HandAsset`.
    pub aspect: f32,
    pub hand_top: f32,
    pub tip: f32,
    pub wrist: f32,
    pub tip_x: f32,
    pub rod_x: f32,
    pub rod_width: f32,
    rotated: HashMap<i32, Arc<Rotated>>,
}

impl Hand {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let image = image::open(path)
            .map_err(|e| format!("can't read {}: {e}", path.display()))?
            .into_rgba8();
        Hand::measure(&image)
    }

    /// Measures a hand image (`HandAsset.measure`).
    pub fn measure(image: &RgbaImage) -> Result<Self, String> {
        let (w, h) = (image.width() as usize, image.height() as usize);
        let solid = |x: usize, y: usize| image.get_pixel(x as u32, y as u32)[3] > 128;
        let counts: Vec<usize> = (0..h).map(|y| (0..w).filter(|&x| solid(x, y)).count()).collect();
        let center = |y: usize| {
            let xs: Vec<usize> = (0..w).filter(|&x| solid(x, y)).collect();
            match (xs.first(), xs.last()) {
                (Some(lo), Some(hi)) => (lo + hi) as f32 / 2.0 / w as f32,
                _ => 0.5,
            }
        };

        let top = counts.iter().position(|&n| n > 0).ok_or("image is empty")?;
        let search_end = h.min(top + (0.45 * h as f32) as usize);
        let max_w = *counts[top..search_end].iter().max().unwrap_or(&0);
        let fist = (top..h).find(|&y| counts[y] as f32 > 0.6 * max_w as f32).ok_or("no hand found")?;
        let narrowing = (fist..h).find(|&y| (counts[y] as f32) < 0.35 * max_w as f32).ok_or("no wrist found")?;
        let last_row = counts.iter().rposition(|&n| n > 0).unwrap_or(h - 1);
        let mut below: Vec<usize> = counts[narrowing..=last_row].to_vec();
        below.sort_unstable();
        let stick_w = below[below.len() / 2];
        let wrist = (narrowing..=last_row)
            .find(|&y| counts[y] as f32 <= 1.25 * stick_w as f32)
            .unwrap_or(narrowing);
        let rod_row = (wrist + h / 50).min(h - 1);
        let from = (wrist + h / 33).min(h - 1);
        let rod_end = (from..h).find(|&y| counts[y] as f32 > 1.8 * counts[rod_row] as f32).unwrap_or(last_row);

        let fh = h as f32;
        let hand_top_px = (top as f32 - 0.004 * fh).max(0.0);
        let crop = imageops::crop_imm(image, 0, hand_top_px as u32, w as u32, (wrist as f32 - hand_top_px) as u32).to_image();

        Ok(Hand {
            crop,
            aspect: w as f32 / fh,
            hand_top: hand_top_px / fh,
            tip: top as f32 / fh,
            wrist: wrist as f32 / fh,
            tip_x: center((top + h / 200).min(h - 1)),
            rod_x: center((wrist + rod_end) / 2),
            rod_width: counts[rod_row] as f32 / w as f32,
            rotated: HashMap::new(),
        })
    }

    /// Shrinks the hand once to the size it will be drawn at, so rotations are cheap and sharp.
    pub fn fit(&mut self, device_height: u32) {
        let height = device_height.max(16);
        let width = (self.crop.width() as f32 * height as f32 / self.crop.height() as f32).round() as u32;
        self.crop = imageops::resize(&self.crop, width, height, imageops::FilterType::Triangle);
        self.rotated.clear();
    }

    /// The hand turned anticlockwise (as seen on screen) by `radians`, to the nearest half
    /// degree. Returns a key that is the same for every call that gives the same image, so
    /// callers can keep their own GPU copies.
    pub fn rotated(&mut self, radians: f32) -> (i32, Arc<Rotated>) {
        let key = (radians.to_degrees() * 2.0).round() as i32;
        let crop = &self.crop;
        let entry = self.rotated.entry(key).or_insert_with(|| {
            let (bgra, width_factor, height_factor) = rotate(crop, (key as f32 / 2.0).to_radians());
            Arc::new(Rotated { bgra, width_factor, height_factor })
        });
        (key, entry.clone())
    }

    /// Size of the fitted hand image in device pixels.
    pub fn fitted_size(&self) -> (u32, u32) {
        (self.crop.width(), self.crop.height())
    }
}

/// Bilinear rotation into a new image just big enough to hold the result. Returns the
/// image in GPUI's BGRA order and its size relative to the source.
fn rotate(src: &RgbaImage, angle: f32) -> (RgbaImage, f32, f32) {
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    let (sin, cos) = angle.sin_cos();
    let ow = (sw * cos.abs() + sh * sin.abs()).ceil();
    let oh = (sw * sin.abs() + sh * cos.abs()).ceil();
    let mut out = RgbaImage::new(ow as u32, oh as u32);
    let (scx, scy, ocx, ocy) = (sw / 2.0, sh / 2.0, ow / 2.0, oh / 2.0);

    let sample = |x: i64, y: i64| -> [f32; 4] {
        if x < 0 || y < 0 || x >= sw as i64 || y >= sh as i64 {
            return [0.0; 4];
        }
        let p = src.get_pixel(x as u32, y as u32).0;
        let a = p[3] as f32;
        // Premultiply while blending so transparent edges don't bleed dark fringes.
        [p[0] as f32 * a / 255.0, p[1] as f32 * a / 255.0, p[2] as f32 * a / 255.0, a]
    };

    for oy in 0..oh as u32 {
        for ox in 0..ow as u32 {
            // Screen y points down, so turning anticlockwise on screen maps an output
            // offset back to the source like this.
            let dx = ox as f32 + 0.5 - ocx;
            let dy = oy as f32 + 0.5 - ocy;
            let sx = dx * cos - dy * sin + scx - 0.5;
            let sy = dx * sin + dy * cos + scy - 0.5;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (a, b, c, d) = (sample(x0, y0), sample(x0 + 1, y0), sample(x0, y0 + 1), sample(x0 + 1, y0 + 1));
            let mut px = [0.0f32; 4];
            for i in 0..4 {
                let top = a[i] + (b[i] - a[i]) * fx;
                let bottom = c[i] + (d[i] - c[i]) * fx;
                px[i] = top + (bottom - top) * fy;
            }
            let alpha = px[3];
            if alpha <= 0.5 {
                continue;
            }
            let un = |v: f32| (v * 255.0 / alpha).round().clamp(0.0, 255.0) as u8;
            // BGRA, as GPUI's own image loader produces.
            out.put_pixel(ox, oy, image::Rgba([un(px[2]), un(px[1]), un(px[0]), alpha.round() as u8]));
        }
    }
    (out, ow / sw, oh / sh)
}

/// The picture of a hand for buttons (`HandAsset.thumbnail`): a square around the hand
/// only (fingertip to wrist), 1.06 times its larger side, centred on it and scaled to
/// `side` × `side` pixels. RGBA, straight alpha.
pub fn thumbnail(path: &std::path::Path, side: u32) -> Result<RgbaImage, String> {
    let image = image::open(path)
        .map_err(|e| format!("can't read {}: {e}", path.display()))?
        .into_rgba8();
    thumbnail_of(&image, side)
}

/// [`thumbnail`] of an image already in memory.
pub fn thumbnail_of(image: &RgbaImage, side: u32) -> Result<RgbaImage, String> {
    let hand = Hand::measure(image)?;
    let (w, h) = (image.width() as f32, image.height() as f32);
    let (top, wrist) = ((hand.tip * h) as u32, (hand.wrist * h) as u32);
    let (mut min_x, mut max_x) = (image.width(), 0);
    for y in top..wrist.min(image.height()) {
        for x in 0..image.width() {
            if image.get_pixel(x, y)[3] > 128 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
    }
    if min_x > max_x {
        return Err("no hand found".into());
    }
    let hand_w = (max_x - min_x) as f32;
    let hand_h = (hand.wrist - hand.hand_top) * h;
    let square = hand_w.max(hand_h) * 1.06;
    let (cx, cy) = ((min_x + max_x) as f32 / 2.0, (hand.hand_top + hand.wrist) / 2.0 * h);
    // Clipped to the image, as CGImage.cropping(to:) does; the clipped square is then
    // stretched to fill, as the Swift thumbnail draws it.
    let x0 = (cx - square / 2.0).max(0.0);
    let y0 = (cy - square / 2.0).max(0.0);
    let x1 = (cx + square / 2.0).min(w);
    let y1 = (cy + square / 2.0).min(h);
    let crop = imageops::crop_imm(image, x0 as u32, y0 as u32, ((x1 - x0) as u32).max(1), ((y1 - y0) as u32).max(1)).to_image();
    Ok(imageops::resize(&crop, side, side, imageops::FilterType::Triangle))
}
