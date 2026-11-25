//! Pointer thumbnails: the hand cropped to a square around it, as `HandAsset.thumbnail`
//! in PointerStyle.swift, made off the main thread once and kept for the app's life.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{AppContext, Context, RenderImage};
use image::{RgbaImage, imageops};
use tinyhand_core::hand::Hand;

use super::SettingsWindow;

/// Thumbnail side in pixels (shown at up to 42 points, so this is sharp at 2x and 3x).
const SIDE: u32 = 128;

thread_local! {
    static CACHE: RefCell<HashMap<String, Arc<RenderImage>>> = RefCell::new(HashMap::new());
    static PENDING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Whether a thumbnail still has to be made (not cached and not tried yet).
pub fn wanted(id: &str) -> bool {
    get(id).is_none() && PENDING.with(|p| !p.borrow().contains(id))
}

pub fn get(id: &str) -> Option<Arc<RenderImage>> {
    CACHE.with(|c| c.borrow().get(id).cloned())
}

/// Drops a thumbnail (a custom hand that was deleted).
pub fn forget(id: &str) {
    CACHE.with(|c| c.borrow_mut().remove(id));
    PENDING.with(|p| p.borrow_mut().remove(id));
}

/// Starts making thumbnails (ids from `wanted`, with their image files); the window
/// re-renders when they are ready. A hand with no file or a bad one isn't tried again.
pub fn request(wanted: Vec<(String, Option<PathBuf>)>, cx: &mut Context<SettingsWindow>) {
    let missing: Vec<(String, PathBuf)> = wanted
        .into_iter()
        .filter(|(id, _)| PENDING.with(|p| p.borrow_mut().insert(id.clone())))
        .filter_map(|(id, path)| Some((id, path?)))
        .collect();
    if missing.is_empty() {
        return;
    }
    let work = cx.background_spawn(async move {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
        let chunk = missing.len().div_ceil(threads);
        std::thread::scope(|s| {
            let handles: Vec<_> = missing
                .chunks(chunk.max(1))
                .map(|part| s.spawn(move || part.iter().map(|(id, path)| (id.clone(), make(path))).collect::<Vec<_>>()))
                .collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect::<Vec<_>>()
        })
    });
    cx.spawn(async move |this, cx| {
        let made = work.await;
        let _ = this.update(cx, |_, cx| {
            for (id, image) in made {
                if let Some(image) = image {
                    CACHE.with(|c| c.borrow_mut().insert(id, Arc::new(RenderImage::new(vec![image::Frame::new(image)]))));
                }
            }
            cx.notify();
        });
    })
    .detach();
}

/// The square around the hand (fingertip to wrist), 1.06 times its larger side, scaled
/// to `SIDE` pixels, in GPUI's BGRA order.
fn make(path: &Path) -> Option<RgbaImage> {
    let hand = Hand::load(path).ok()?;
    let img = image::open(path).ok()?.into_rgba8();
    let (w, h) = (img.width() as f32, img.height() as f32);
    let top = (hand.tip * h) as u32;
    let wrist = ((hand.wrist * h) as u32).min(img.height());
    let (mut min_x, mut max_x) = (img.width(), 0u32);
    for y in top..wrist {
        for x in 0..img.width() {
            if img.get_pixel(x, y)[3] > 128 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
    }
    if max_x < min_x {
        return None;
    }
    let hand_w = (max_x - min_x) as f32;
    let hand_h = (hand.wrist - hand.hand_top) * h;
    let side = hand_w.max(hand_h) * 1.06;
    let cx = (min_x + max_x) as f32 / 2.0;
    let cy = (hand.hand_top + hand.wrist) / 2.0 * h;
    let (x0, y0) = (cx - side / 2.0, cy - side / 2.0);

    // Copy the square onto a transparent canvas (parts outside the image stay empty),
    // premultiplied so the scaling doesn't darken the edges.
    let s = side.ceil() as u32;
    let mut square = RgbaImage::new(s, s);
    for sy in 0..s {
        let iy = (y0 + sy as f32).floor();
        if iy < 0.0 || iy >= h {
            continue;
        }
        for sx in 0..s {
            let ix = (x0 + sx as f32).floor();
            if ix < 0.0 || ix >= w {
                continue;
            }
            square.put_pixel(sx, sy, *img.get_pixel(ix as u32, iy as u32));
        }
    }
    premultiply(&mut square);
    Some(scaled_bgra(square, SIDE, SIDE))
}

/// Scales a premultiplied RGBA image and returns it straight (not premultiplied) in
/// GPUI's BGRA order.
fn scaled_bgra(premultiplied: RgbaImage, w: u32, h: u32) -> RgbaImage {
    let mut small = imageops::resize(&premultiplied, w, h, imageops::FilterType::CatmullRom);
    for p in small.pixels_mut() {
        let a = p[3] as u32;
        let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a / 2) / a).min(255) as u8 };
        *p = image::Rgba([un(p[2]), un(p[1]), un(p[0]), p[3]]);
    }
    small
}

fn premultiply(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let a = p[3] as u16;
        let pm = |c: u8| ((c as u16 * a + 127) / 255) as u8;
        *p = image::Rgba([pm(p[0]), pm(p[1]), pm(p[2]), p[3]]);
    }
}

/// A part of an RGBA image scaled to `w`x`h`, ready to draw (the hand maker's preview).
pub fn render_crop(img: &RgbaImage, x: u32, y: u32, cw: u32, ch: u32, w: u32, h: u32) -> Arc<RenderImage> {
    let mut part = imageops::crop_imm(img, x, y, cw, ch).to_image();
    premultiply(&mut part);
    Arc::new(RenderImage::new(vec![image::Frame::new(scaled_bgra(part, w, h))]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_are_square_and_not_empty() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/hand_lavender.png");
        let thumb = make(&path).expect("thumbnail");
        assert_eq!(thumb.dimensions(), (SIDE, SIDE));
        let solid = thumb.pixels().filter(|p| p[3] > 200).count();
        assert!(solid > (SIDE * SIDE / 5) as usize, "hand fills the square: {solid}");
    }
}
