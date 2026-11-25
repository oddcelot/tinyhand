//! Every bundled hand must measure sensibly, and the arm must put its fingertip on the
//! cursor.

use std::path::PathBuf;
use tinyhand_core::{Arm, Hand};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn bundled() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(assets())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("hand_") && p.extension().is_some_and(|e| e == "png"))
        .collect();
    v.sort();
    v
}

#[test]
fn every_hand_measures() {
    let hands = bundled();
    assert!(hands.len() >= 30, "found {} hands", hands.len());
    for path in hands {
        let h = Hand::load(&path).unwrap_or_else(|e| panic!("{e}"));
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(h.tip < h.wrist, "{name}: tip below wrist");
        assert!(h.hand_top <= h.tip, "{name}");
        assert!((0.05..0.6).contains(&(h.wrist - h.hand_top)), "{name}: hand is {} of the image", h.wrist - h.hand_top);
        assert!(h.rod_width > 0.0 && h.rod_width < 0.3, "{name}: rod width {}", h.rod_width);
        assert!((0.0..=1.0).contains(&h.tip_x) && (0.0..=1.0).contains(&h.rod_x), "{name}");
    }
}

#[test]
fn fingertip_lands_on_cursor() {
    let mut hand = Hand::load(&assets().join("hand_butterfly3d.png")).unwrap();
    hand.fit(Arm::hand_pixels(900.0, 2.0));
    let mut arm = Arm::default();
    for &(x, y) in &[(700.0, 450.0), (100.0, 80.0), (1300.0, 850.0), (720.0, 20.0)] {
        // Let the grip settle.
        let mut pose = arm.aim(&hand, (x, y), (1440.0, 900.0), 1.0 / 120.0);
        for _ in 0..600 {
            pose = arm.aim(&hand, (x, y), (1440.0, 900.0), 1.0 / 120.0);
        }
        let (tx, ty) = Arm::fingertip(&hand, &pose);
        assert!((tx - x).abs() < 1.5 && (ty - y).abs() < 1.5, "cursor {x},{y} tip {tx},{ty}");
    }
}
