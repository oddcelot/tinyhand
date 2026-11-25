//! The pointer catalogue from PointerStyle.swift: built-in hands, custom hands, the laser dot.

use crate::custom_hands::CustomHand;
use std::path::{Path, PathBuf};

/// Ink.purple (sRGB 0.53, 0.33, 0.95) as 0xRRGGBB.
pub const INK_PURPLE: u32 = 0x8754F2;
/// Ink.red (sRGB 1.0, 0.16, 0.12) as 0xRRGGBB.
pub const INK_RED: u32 = 0xFF291F;

pub const MAX_PINNED: usize = 8;
pub const LASER: &str = "laser";
/// The style used when nothing valid is saved.
pub const DEFAULT_STYLE: &str = "butterfly3d";
pub const DEFAULT_PINNED: [&str; 7] = [
    "butterfly3d",
    "lavender",
    "glove",
    "coral",
    "plush",
    "robot",
    "laser",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StyleKind {
    Hand,
    Custom,
    Laser,
}

impl StyleKind {
    /// The word used in `state.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            StyleKind::Hand => "hand",
            StyleKind::Custom => "custom",
            StyleKind::Laser => "laser",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Style {
    pub id: String,
    pub title: String,
    /// Ink colour, 0xRRGGBB.
    pub ink: u32,
    pub kind: StyleKind,
}

/// (id, title, ink) in catalogue order.
pub const BUILT_IN: [(&str, &str, u32); 39] = [
    ("lavender", "Lavender toy", INK_PURPLE),
    ("glove", "Cursor glove", INK_PURPLE),
    ("coral", "Coral vinyl", INK_RED),
    ("plush", "Plush", 0x2F86F0),
    ("robot", "Robot", 0x16A87A),
    ("pink", "Bubblegum", 0xEE4F98),
    ("blue", "Sky", 0x2F86F0),
    ("mint", "Mint", 0x16A87A),
    ("lemon", "Lemon", 0xF2B705),
    ("black", "Midnight", 0x1D1D21),
    ("chrome", "Chrome", 0x6B7380),
    ("holo", "Holo", INK_PURPLE),
    ("fuzzy", "Fuzzy", INK_PURPLE),
    ("wood", "Wood", 0x9A5E2C),
    ("marble", "Marble", 0x6B7380),
    ("terrazzo", "Terrazzo", 0xF0561D),
    ("glass", "Glass", 0x1597E5),
    ("jelly", "Jelly", 0xA33FE6),
    ("pastel", "Pastel", 0xEE4F98),
    ("pixel", "Pixel", 0x1D1D21),
    ("lowpoly", "Low poly", INK_PURPLE),
    ("knit", "Knit", INK_PURPLE),
    ("skeleton", "Skeleton", 0x9A5E2C),
    ("astro", "Astronaut", 0x2F86F0),
    ("space", "Space", INK_RED),
    ("monster", "Monster", 0x3E9E2E),
    ("water", "Water", 0x1597E5),
    ("lava", "Lava", 0xF0561D),
    ("grass", "Grass", 0x3E9E2E),
    ("cactus", "Cactus", 0x3E9E2E),
    ("choco", "Chocolate", 0x9A5E2C),
    ("cloud", "Cloud", 0x2F86F0),
    ("mech", "Mech", 0xF0561D),
    ("neon", "Neon", 0xE0309A),
    ("leaf", "Leaf", 0x3E9E2E),
    ("cookie", "Gingerbread", 0x9A5E2C),
    ("pentacode", "Pentacode", 0x0CA8D1),
    ("butterfly", "Butterfly", 0x1991C7),
    ("butterfly3d", "Butterfly 3D", 0x1991C7),
];

fn laser() -> Style {
    Style {
        id: LASER.into(),
        title: "Laser dot".into(),
        ink: INK_RED,
        kind: StyleKind::Laser,
    }
}

/// Built-in hands, then custom hands, then the laser dot.
pub fn all_styles(custom: &[CustomHand]) -> Vec<Style> {
    let mut v: Vec<Style> = BUILT_IN
        .iter()
        .map(|&(id, title, ink)| Style {
            id: id.into(),
            title: title.into(),
            ink,
            kind: StyleKind::Hand,
        })
        .collect();
    v.extend(custom.iter().map(|c| Style {
        id: c.id.clone(),
        title: c.name.clone(),
        ink: c.ink,
        kind: StyleKind::Custom,
    }));
    v.push(laser());
    v
}

pub fn is_built_in(id: &str) -> bool {
    BUILT_IN.iter().any(|b| b.0 == id)
}

/// The style with this id, or `None` if it isn't built in, custom or the laser.
pub fn find(id: &str, custom: &[CustomHand]) -> Option<Style> {
    if id == LASER {
        return Some(laser());
    }
    if let Some(&(id, title, ink)) = BUILT_IN.iter().find(|b| b.0 == id) {
        return Some(Style {
            id: id.into(),
            title: title.into(),
            ink,
            kind: StyleKind::Hand,
        });
    }
    custom.iter().find(|c| c.id == id).map(|c| Style {
        id: c.id.clone(),
        title: c.name.clone(),
        ink: c.ink,
        kind: StyleKind::Custom,
    })
}

/// The image file for a style: `hand_<id>.png` for built-ins, `<id>.png` for custom hands.
pub fn asset_file_name(id: &str) -> String {
    if id.starts_with("custom-") {
        format!("{id}.png")
    } else {
        format!("hand_{id}.png")
    }
}

/// Where the hand's image is: the custom hands folder first, then each search folder in
/// order. The laser dot has no image.
pub fn hand_image_path(id: &str, custom_dir: &Path, search_dirs: &[PathBuf]) -> Option<PathBuf> {
    if id == LASER {
        return None;
    }
    let name = asset_file_name(id);
    std::iter::once(custom_dir)
        .chain(search_dirs.iter().map(PathBuf::as_path))
        .map(|d| d.join(&name))
        .find(|p| p.is_file())
}
