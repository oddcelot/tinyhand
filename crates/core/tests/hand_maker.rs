//! HandMaker: prompt, validation, compose, ink colour, names.

use image::{ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;
use tinyhand_core::Hand;
use tinyhand_core::hand_maker::*;

const MUSTARD: [u8; 3] = [230, 170, 30];

fn fill(img: &mut RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 4]) {
    for y in y0..y1 {
        for x in x0..x1 {
            img.put_pixel(x, y, Rgba(c));
        }
    }
}

/// 300x600: a finger on top, a fist, a flat wrist. Solid, no soft edges.
fn hand_with(colour: [u8; 3]) -> RgbaImage {
    let c = [colour[0], colour[1], colour[2], 255];
    let mut img = RgbaImage::new(300, 600);
    fill(&mut img, 130, 60, 170, 180, c);
    fill(&mut img, 50, 180, 250, 480, c);
    fill(&mut img, 100, 480, 200, 540, c);
    img
}

fn hand() -> RgbaImage {
    hand_with(MUSTARD)
}

fn png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).unwrap();
    out
}

#[test]
fn prompt_replaces_idea_or_default() {
    assert!(PROMPT.contains("[describe your hand, e.g. knitted wool in mustard yellow]"));
    assert!(PROMPT.starts_with("Make a pointer image for TinyHand"));
    assert!(PROMPT.ends_with("a little empty space around it."));
    let p = prompt_for("  felt mitten \n");
    assert!(p.contains("in this style: felt mitten\n"));
    assert!(!p.contains("[describe"));
    assert!(prompt_for("   ").contains("style: a soft vinyl toy in a bright colour\n"));
}

#[test]
fn chatgpt_url_is_percent_encoded() {
    let u = chatgpt_url("a&b c");
    assert!(u.starts_with("https://chatgpt.com/?q=Make%20a%20pointer%20image"));
    assert!(u.contains("a%26b%20c"));
    assert!(u.contains("%0A"));
    assert!(!u.contains(' ') && !u.contains('&'));
    assert!(u.contains("%2C")); // the comma in "Rules" lines / prompt text
}

#[test]
fn good_hand_passes() {
    assert_eq!(validate(&hand()), Ok(()));
    assert!(make(&png(&hand())).is_ok());
}

#[test]
fn load_image_rejections() {
    let rgb = image::RgbImage::from_pixel(100, 100, image::Rgb([1, 2, 3]));
    let mut out = Vec::new();
    rgb.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).unwrap();
    assert_eq!(load_image(&out), Err(MakeError::NoAlpha));
    assert_eq!(
        MakeError::NoAlpha.to_string(),
        "The image needs a transparent background. Save it as a PNG with transparency."
    );
    assert_eq!(load_image(b"hello, not an image"), Err(MakeError::Unreadable));
    assert_eq!(load_image(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0]), Err(MakeError::NotPng));
    assert!(load_image(&png(&hand())).is_ok());
}

#[test]
fn too_small() {
    let err = validate(&RgbaImage::new(63, 200)).unwrap_err();
    assert_eq!(err, MakeError::TooSmall);
    assert_eq!(err.to_string(), "The image is too small. Use one at least 512 pixels tall.");
}

#[test]
fn touching_the_edge() {
    let mut img = hand();
    fill(&mut img, 0, 0, 300, 20, [0, 0, 0, 255]);
    let err = validate(&img).unwrap_err();
    assert_eq!(err, MakeError::Edge);
    assert_eq!(err.to_string(), "The background isn't transparent, or the hand touches the edge of the image.");
}

#[test]
fn empty_image() {
    let err = validate(&RgbaImage::new(200, 300)).unwrap_err();
    assert_eq!(err, MakeError::Empty);
    assert_eq!(err.to_string(), "The image is empty.");
}

#[test]
fn two_things() {
    let mut img = hand();
    fill(&mut img, 5, 300, 45, 400, [10, 10, 10, 255]);
    let err = validate(&img).unwrap_err();
    assert_eq!(err, MakeError::MoreThanOne);
    assert_eq!(err.to_string(), "The image has more than one thing in it. Use a single hand.");
}

#[test]
fn not_upright() {
    let mut img = RgbaImage::new(600, 300);
    fill(&mut img, 100, 50, 500, 250, [200, 50, 50, 255]);
    let err = validate(&img).unwrap_err();
    assert_eq!(err, MakeError::NotUpright);
    assert_eq!(err.to_string(), "The hand should stand upright, taller than it is wide.");
}

#[test]
fn too_small_in_image() {
    let mut img = RgbaImage::new(600, 1200);
    fill(&mut img, 200, 100, 400, 400, [200, 50, 50, 255]);
    let err = validate(&img).unwrap_err();
    assert_eq!(err, MakeError::HandTooSmall);
    assert_eq!(err.to_string(), "The hand is too small in the image. Crop it closer.");
}

#[test]
fn top_is_not_a_finger() {
    let mut img = RgbaImage::new(300, 600);
    fill(&mut img, 50, 100, 250, 500, [200, 50, 50, 255]);
    fill(&mut img, 100, 500, 200, 540, [200, 50, 50, 255]);
    let err = validate(&img).unwrap_err();
    assert_eq!(err, MakeError::NoFinger);
    assert_eq!(err.to_string(), "The top of the image should be a single finger pointing up.");
}

#[test]
fn no_flat_wrist() {
    let mut img = RgbaImage::new(300, 600);
    fill(&mut img, 130, 60, 170, 180, [200, 50, 50, 255]);
    fill(&mut img, 50, 180, 250, 480, [200, 50, 50, 255]);
    fill(&mut img, 145, 480, 155, 540, [200, 50, 50, 255]); // 10 of 200 wide
    let err = validate(&img).unwrap_err();
    assert_eq!(err, MakeError::NoWrist);
    assert_eq!(err.to_string(), "The hand should end in a flat wrist or cuff at the bottom.");
}

#[test]
fn soft_shadow() {
    let mut img = hand();
    // A faint halo on the left and right of the fist, far from the edges.
    fill(&mut img, 20, 180, 50, 480, [0, 0, 0, 100]);
    fill(&mut img, 250, 180, 280, 480, [0, 0, 0, 100]);
    let err = validate(&img).unwrap_err();
    assert_eq!(err, MakeError::SoftShadow);
    assert_eq!(err.to_string(), "The hand has a soft shadow or glow around it. Ask for no shadow.");
}

#[test]
fn compose_size_and_placement() {
    let out = compose(&hand());
    assert_eq!((out.width(), out.height()), (1024, 1536));
    // scale = min(560/480, 700/200) = 1.1667; the fingertip lands on y=150, the wrist is centred.
    let first_solid = (0..1536).find(|&y| (0..1024).any(|x| out.get_pixel(x, y)[3] > 128)).unwrap();
    assert!((149..=151).contains(&first_solid), "tip at {first_solid}");
    let xs: Vec<u32> = (0..1024).filter(|&x| out.get_pixel(x, 150 + 3)[3] > 128).collect();
    let mid = (xs[0] + xs[xs.len() - 1]) as f32 / 2.0;
    // The finger is centred at 150/300 of the source, the wrist too: so x = 512.
    assert!((mid - 512.0).abs() < 3.0, "finger centre {mid}");
    // Rod: dark, centred, runs to y=1462.
    assert!(out.get_pixel(512, 1400)[3] > 250);
    assert!(out.get_pixel(512, 1400)[0] < 0x40);
    assert_eq!(out.get_pixel(512, 1500)[3], 0);
    assert!(out.get_pixel(512, 1455)[3] > 200);
    assert_eq!(out.get_pixel(100, 1400)[3], 0);
}

#[test]
fn composed_hand_loads_and_measures() {
    let out = compose(&hand());
    let path = std::env::temp_dir().join(format!("tinyhand_hand_maker_test_{}.png", std::process::id()));
    out.save(&path).unwrap();
    let measured = Hand::load(&path);
    let _ = std::fs::remove_file(&path);
    let h = measured.unwrap();
    assert!(h.tip < h.wrist);
    assert!((h.tip_x - 0.5).abs() < 0.01, "tip_x {}", h.tip_x);
    assert!((h.rod_x - 0.5).abs() < 0.01, "rod_x {}", h.rod_x);
    assert!((h.aspect - 1024.0 / 1536.0).abs() < 1e-4);
    assert!((0.05..0.6).contains(&(h.wrist - h.hand_top)));
}

#[test]
fn ink_colour_of_solid_hands() {
    // Mustard (230,170,30): brightest channel 0.902 is scaled down to 0.82.
    assert_eq!(ink_colour(&hand()), 0xD19A1B);
    // A mid colour is left alone (float truncation may cost one step per channel).
    let near = |got: u32, want: u32| (0..3).all(|i| ((got >> (8 * i)) & 0xFF).abs_diff((want >> (8 * i)) & 0xFF) <= 1);
    assert!(near(ink_colour(&hand_with([100, 150, 200])), 0x6496C8));
    // Grey falls back to (0.42, 0.45, 0.5).
    assert!(near(ink_colour(&hand_with([128, 128, 128])), 0x6B727F));
    // Nothing to sample gives the purple default, capped like any bright colour.
    assert!(near(ink_colour(&RgbaImage::new(30, 30)), 0x7448D1));
}

#[test]
fn names() {
    assert_eq!(suggested_name("knitted_wool-hand.png").as_deref(), Some("Knitted wool"));
    assert_eq!(suggested_name("mitten.png").as_deref(), Some("Mitten"));
    assert_eq!(suggested_name("one two three.png").as_deref(), Some("One two"));
    assert_eq!(suggested_name("ChatGPT Image Nov 25, 2026.png"), None);
    assert_eq!(suggested_name("extraordinarily_long.png"), None);
    assert_eq!(suggested_name("  ").as_deref(), None);
}
