use image::{Rgba, RgbaImage};
use std::collections::HashSet;
use std::path::PathBuf;
use tinyhand_core::custom_hands::*;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tinyhand-hands-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn png() -> RgbaImage {
    RgbaImage::from_pixel(4, 6, Rgba([10, 20, 30, 255]))
}

#[test]
fn add_writes_png_and_list() {
    let d = temp("add");
    let mut h = CustomHands::load(&d);
    assert!(h.hands().is_empty());
    let a = h.add(&png(), "  Floppy  ", 0x123456).unwrap();
    assert_eq!(a.name, "Floppy");
    assert_eq!(a.ink, 0x123456);
    assert!(a.id.starts_with("custom-"));
    let hex = &a.id["custom-".len()..];
    assert_eq!(hex.len(), 8);
    assert!(hex.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    assert_eq!(image::open(h.png_path(&a.id)).unwrap().to_rgba8(), png());

    // hands.json is a plain array with decimal ink.
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join("hands.json")).unwrap()).unwrap();
    assert_eq!(json[0]["id"], a.id.as_str());
    assert_eq!(json[0]["name"], "Floppy");
    assert_eq!(json[0]["ink"], 0x123456);

    // A fresh load sees it.
    let again = CustomHands::load(&d);
    assert_eq!(again.hands(), &[a]);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn names_default_and_cap() {
    let d = temp("names");
    let mut h = CustomHands::load(&d);
    assert_eq!(h.add(&png(), "", 1).unwrap().name, "My hand 1");
    assert_eq!(h.add(&png(), "   ", 1).unwrap().name, "My hand 2");
    let long = h.add(&png(), "A very long hand name indeed", 1).unwrap();
    assert_eq!(long.name, "A very long hand");
    assert_eq!(long.name.chars().count(), 16);
    let wide = h.add(&png(), "ééééééééééééééééééé", 1).unwrap();
    assert_eq!(wide.name.chars().count(), 16);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn ids_are_unique() {
    let d = temp("ids");
    let mut h = CustomHands::load(&d);
    let img = RgbaImage::new(1, 1);
    let ids: HashSet<String> = (0..40).map(|_| h.add(&img, "x", 0).unwrap().id).collect();
    assert_eq!(ids.len(), 40);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn delete_removes_file_and_entry() {
    let d = temp("delete");
    let mut h = CustomHands::load(&d);
    let a = h.add(&png(), "One", 1).unwrap();
    let b = h.add(&png(), "Two", 2).unwrap();
    assert!(h.delete(&a.id).unwrap());
    assert!(!h.png_path(&a.id).exists());
    assert!(h.png_path(&b.id).exists());
    assert!(!h.delete(&a.id).unwrap());
    assert_eq!(CustomHands::load(&d).hands(), &[b]);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn rename_trims_caps_and_persists() {
    let d = temp("rename");
    let mut h = CustomHands::load(&d);
    let a = h.add(&png(), "One", 1).unwrap();
    assert!(h.rename(&a.id, "  Renamed hand that is long ").unwrap());
    assert_eq!(h.get(&a.id).unwrap().name, "Renamed hand tha");
    assert!(!h.rename(&a.id, "   ").unwrap());
    assert!(!h.rename("custom-nothere", "x").unwrap());
    assert_eq!(CustomHands::load(&d).get(&a.id).unwrap().name, "Renamed hand tha");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn reads_swift_written_list() {
    let d = temp("swift");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("hands.json"), r#"[{"id":"custom-0a1b2c3d","name":"Old","ink":9131250}]"#).unwrap();
    let h = CustomHands::load(&d);
    assert_eq!(h.hands()[0], CustomHand { id: "custom-0a1b2c3d".into(), name: "Old".into(), ink: 9131250 });
    assert_eq!(h.default_name(), "My hand 2");
    std::fs::write(d.join("hands.json"), "junk").unwrap();
    assert!(CustomHands::load(&d).hands().is_empty());
    std::fs::remove_dir_all(&d).unwrap();
}
