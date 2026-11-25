use std::path::PathBuf;
use tinyhand_core::custom_hands::CustomHand;
use tinyhand_core::styles::*;

#[test]
fn catalogue_order_and_size() {
    let all = all_styles(&[]);
    assert_eq!(all.len(), 40);
    assert_eq!(all[0].id, "lavender");
    assert_eq!(all[38].id, "butterfly3d");
    assert_eq!(all.last().unwrap().id, "laser");
    assert_eq!(all.last().unwrap().kind, StyleKind::Laser);
    let mut ids: Vec<_> = all.iter().map(|s| s.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 40);
}

#[test]
fn titles_and_inks_match_swift() {
    let s = find("lavender", &[]).unwrap();
    assert_eq!((s.title.as_str(), s.ink), ("Lavender toy", 0x8754F2));
    assert_eq!(find("coral", &[]).unwrap().ink, 0xFF291F);
    assert_eq!(find("cookie", &[]).unwrap().title, "Gingerbread");
    assert_eq!(find("lowpoly", &[]).unwrap().title, "Low poly");
    assert_eq!(find("black", &[]).unwrap().title, "Midnight");
    assert_eq!(find("pentacode", &[]).unwrap().ink, 0x0CA8D1);
    assert_eq!(find("laser", &[]).unwrap().title, "Laser dot");
    assert_eq!(find("laser", &[]).unwrap().ink, 0xFF291F);
    assert!(find("nope", &[]).is_none());
}

#[test]
fn customs_sit_between_built_ins_and_laser() {
    let c = vec![CustomHand { id: "custom-abcd1234".into(), name: "Mine".into(), ink: 0x112233 }];
    let all = all_styles(&c);
    assert_eq!(all.len(), 41);
    assert_eq!(all[39].id, "custom-abcd1234");
    assert_eq!(all[39].kind, StyleKind::Custom);
    assert_eq!(all[40].id, "laser");
    let f = find("custom-abcd1234", &c).unwrap();
    assert_eq!((f.title.as_str(), f.ink), ("Mine", 0x112233));
    assert!(find("custom-abcd1234", &[]).is_none());
}

#[test]
fn pinned_defaults() {
    assert_eq!(MAX_PINNED, 8);
    assert_eq!(DEFAULT_PINNED.len(), 7);
    for id in DEFAULT_PINNED {
        assert!(find(id, &[]).is_some());
    }
}

#[test]
fn asset_names() {
    assert_eq!(asset_file_name("lavender"), "hand_lavender.png");
    assert_eq!(asset_file_name("custom-1234abcd"), "custom-1234abcd.png");
}

#[test]
fn every_built_in_has_a_bundled_image() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    for s in all_styles(&[]).iter().filter(|s| s.kind == StyleKind::Hand) {
        let p = hand_image_path(&s.id, &PathBuf::from("/nonexistent"), &[assets.clone()]);
        assert!(p.is_some(), "no image for {}", s.id);
    }
}

#[test]
fn image_lookup_prefers_the_custom_folder() {
    let root = std::env::temp_dir().join(format!("tinyhand-styles-{}", std::process::id()));
    let (custom, a, b) = (root.join("c"), root.join("a"), root.join("b"));
    for d in [&custom, &a, &b] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(b.join("hand_pink.png"), b"x").unwrap();
    std::fs::write(a.join("hand_pink.png"), b"x").unwrap();
    std::fs::write(custom.join("custom-00000001.png"), b"x").unwrap();
    let dirs = vec![a.clone(), b.clone()];
    assert_eq!(hand_image_path("pink", &custom, &dirs), Some(a.join("hand_pink.png")));
    assert_eq!(hand_image_path("pink", &custom, &[b.clone()]), Some(b.join("hand_pink.png")));
    assert_eq!(hand_image_path("custom-00000001", &custom, &dirs), Some(custom.join("custom-00000001.png")));
    assert_eq!(hand_image_path("lemon", &custom, &dirs), None);
    assert_eq!(hand_image_path("laser", &custom, &dirs), None);
    std::fs::remove_dir_all(&root).unwrap();
}
