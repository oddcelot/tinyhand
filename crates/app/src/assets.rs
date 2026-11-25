//! Finding the files the app ships with: hand images, tap sounds and the tray icons.

use std::path::{Path, PathBuf};

use tinyhand_core::custom_hands::CustomHands;
use tinyhand_core::styles;
use tinyhand_platform::tray::TrayIcons;

/// The repo's `assets/` folder, for runs from a build folder.
fn repo_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

/// Folders that may hold `hand_<id>.png`, in the order they are tried: `TH_HANDS_DIR`,
/// next to an installed binary (`../share/tinyhand/hands`, as the .deb lays it out),
/// `/usr/share/tinyhand/hands`, a macOS bundle's `Resources`, next to the binary, and the
/// repo's assets folder.
pub fn hand_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("TH_HANDS_DIR") {
        dirs.push(dir.into());
    }
    if let Some(bin) = exe_dir() {
        dirs.push(bin.join("../share/tinyhand/hands"));
        dirs.push(bin.join("../Resources"));
        dirs.push(bin.clone());
        dirs.push(bin.join("assets"));
    }
    dirs.push("/usr/share/tinyhand/hands".into());
    dirs.push(repo_assets());
    dirs
}

/// Folders that may hold `tap_<id>.wav`; the sound player adds the places next to the binary.
pub fn sound_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = exe_dir() {
        dirs.push(bin.join("../share/tinyhand/sounds"));
        dirs.push(bin.join("../Resources"));
    }
    dirs.push("/usr/share/tinyhand/sounds".into());
    dirs.push(repo_assets().join("sounds"));
    dirs
}

/// The image for a hand style, or `None` for the laser dot or a missing file.
pub fn hand_image(id: &str, custom: &CustomHands) -> Option<PathBuf> {
    styles::hand_image_path(id, custom.dir(), &hand_dirs())
}

fn find_file(name: &str) -> Option<PathBuf> {
    let mut dirs = hand_dirs();
    dirs.push(repo_assets().join("icon"));
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

fn decode(name: &str) -> Result<image::RgbaImage, String> {
    let path = find_file(name).ok_or_else(|| format!("{name} not found"))?;
    image::open(&path).map(|i| i.into_rgba8()).map_err(|e| format!("{}: {e}", path.display()))
}

/// `menubar_on@2x.png` and `menubar_off@2x.png` (36×36, black template images). On
/// Windows and Linux, where the tray shows the pixels as they are, they are made white
/// so they show on the usual dark taskbars and panels.
pub fn tray_icons() -> Result<TrayIcons, String> {
    let on = decode("menubar_on@2x.png").or_else(|_| decode("menubar_on.png"))?;
    let off = decode("menubar_off@2x.png").or_else(|_| decode("menubar_off.png"))?;
    if on.dimensions() != off.dimensions() {
        return Err("the on and off tray icons differ in size".into());
    }
    let light = |img: image::RgbaImage| -> Vec<u8> {
        let raw = img.into_raw();
        if cfg!(target_os = "macos") {
            return raw;
        }
        raw.chunks_exact(4).flat_map(|p| [255, 255, 255, p[3]]).collect()
    };
    let (w, h) = on.dimensions();
    TrayIcons::from_rgba(w, h, light(on), light(off))
}
