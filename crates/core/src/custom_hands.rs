//! Hands the user made, stored as `<id>.png` plus a `hands.json` list (HandMaker.swift).

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Longest name a hand can have, in characters.
pub const MAX_NAME: usize = 16;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CustomHand {
    pub id: String,
    pub name: String,
    /// Ink colour, 0xRRGGBB, stored as a decimal number.
    pub ink: u32,
}

/// Where custom hands live: `TH_HANDS_DIR` if set, else next to the settings file
/// (`Hands` on macOS, `hands` elsewhere).
pub fn hands_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("TH_HANDS_DIR") {
        return PathBuf::from(dir);
    }
    let name = if cfg!(target_os = "macos") { "Hands" } else { "hands" };
    crate::settings::config_dir().join(name)
}

pub struct CustomHands {
    dir: PathBuf,
    hands: Vec<CustomHand>,
}

impl CustomHands {
    /// Reads `hands.json` from `dir`. A missing or broken file gives an empty list.
    pub fn load(dir: &Path) -> CustomHands {
        let hands = std::fs::read(dir.join("hands.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        CustomHands {
            dir: dir.to_path_buf(),
            hands,
        }
    }

    pub fn load_default() -> CustomHands {
        CustomHands::load(&hands_dir())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn hands(&self) -> &[CustomHand] {
        &self.hands
    }

    pub fn get(&self, id: &str) -> Option<&CustomHand> {
        self.hands.iter().find(|h| h.id == id)
    }

    pub fn png_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.png"))
    }

    /// Saves the image and adds the hand. An empty name becomes "My hand N".
    pub fn add(&mut self, png: &image::RgbaImage, name: &str, ink: u32) -> io::Result<CustomHand> {
        std::fs::create_dir_all(&self.dir)?;
        let id = self.new_id();
        png.save_with_format(self.png_path(&id), image::ImageFormat::Png)
            .map_err(io::Error::other)?;
        let name = self.clean_name(name);
        let hand = CustomHand { id, name, ink };
        self.hands.push(hand.clone());
        if let Err(e) = self.save() {
            self.hands.pop();
            let _ = std::fs::remove_file(self.png_path(&hand.id));
            return Err(e);
        }
        Ok(hand)
    }

    /// Removes the hand and its image. False if there was no such hand.
    pub fn delete(&mut self, id: &str) -> io::Result<bool> {
        let Some(at) = self.hands.iter().position(|h| h.id == id) else {
            return Ok(false);
        };
        let _ = std::fs::remove_file(self.png_path(id));
        self.hands.remove(at);
        self.save()?;
        Ok(true)
    }

    /// Renames a hand (trimmed, cut to 16 characters). False if there is no such hand or
    /// the new name is empty.
    pub fn rename(&mut self, id: &str, name: &str) -> io::Result<bool> {
        let name = trim_name(name);
        let Some(hand) = self.hands.iter_mut().find(|h| h.id == id) else {
            return Ok(false);
        };
        if name.is_empty() {
            return Ok(false);
        }
        hand.name = name;
        self.save()?;
        Ok(true)
    }

    /// "My hand N" for the next hand to be added.
    pub fn default_name(&self) -> String {
        format!("My hand {}", self.hands.len() + 1)
    }

    fn clean_name(&self, name: &str) -> String {
        let name = trim_name(name);
        if name.is_empty() {
            self.default_name()
        } else {
            name
        }
    }

    fn new_id(&self) -> String {
        loop {
            let id = format!("custom-{:08x}", random_u32());
            if self.get(&id).is_none() && !self.png_path(&id).exists() {
                return id;
            }
        }
    }

    fn save(&self) -> io::Result<()> {
        let json = serde_json::to_vec(&self.hands).map_err(io::Error::other)?;
        crate::settings::write_atomic(&self.dir.join("hands.json"), &json)
    }
}

fn trim_name(name: &str) -> String {
    name.trim().chars().take(MAX_NAME).collect()
}

/// Time plus a counter, mixed (splitmix64); no random crate needed for an id.
fn random_u32() -> u32 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut z = nanos
        .wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 16) as u32
}
