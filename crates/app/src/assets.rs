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

/// The app icon for the welcome card: the 512 px PNG the Linux package installs, looked
/// for next to an installed binary, in a macOS bundle and in the repo.
pub fn app_icon() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(bin) = exe_dir() {
        candidates.push(bin.join("../share/icons/hicolor/512x512/apps/tinyhand.png"));
        candidates.push(bin.join("../Resources/tinyhand.png"));
        candidates.push(bin.join("tinyhand.png"));
    }
    candidates.push("/usr/share/icons/hicolor/512x512/apps/tinyhand.png".into());
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/linux/tinyhand.png"));
    candidates.into_iter().find(|p| p.is_file())
}

/// An RGBA image as GPUI wants it for drawing (BGRA pixels).
pub fn render_image(mut rgba: image::RgbaImage) -> std::sync::Arc<gpui::RenderImage> {
    for p in rgba.pixels_mut() {
        p.0.swap(0, 2);
    }
    std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(rgba)]))
}

/// Small icons drawn with `svg().path("icons/…")`, tinted with the text colour. Shapes
/// after the SF Symbols the Swift app uses.
pub struct Icons;

impl Icons {
    const FILES: &'static [(&'static str, &'static str)] = &[
        (
            "icons/speaker.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2.5 9.2c0-.7.5-1.2 1.2-1.2h2.9l4.3-3.7c.6-.5 1.6-.1 1.6.8v13.8c0 .9-1 1.3-1.6.8L6.6 16H3.7c-.7 0-1.2-.5-1.2-1.2z" fill="#000"/><path d="M15.6 8.6a4.8 4.8 0 0 1 0 6.8M18.6 5.8a8.8 8.8 0 0 1 0 12.4" stroke="#000" stroke-width="2" stroke-linecap="round" fill="none"/></svg>"##,
        ),
        (
            "icons/speaker-off.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4.5 9.2c0-.7.5-1.2 1.2-1.2h2.9l4.3-3.7c.6-.5 1.6-.1 1.6.8v13.8c0 .9-1 1.3-1.6.8L8.6 16H5.7c-.7 0-1.2-.5-1.2-1.2z" fill="#000"/><path d="M3.5 3.5l17 17" stroke="#000" stroke-width="4.2" stroke-linecap="round"/></svg>"##,
        ),
        (
            "icons/xmark.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18" stroke="#000" stroke-width="2.2" stroke-linecap="round" fill="none"/></svg>"##,
        ),
        (
            "icons/warning.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#000" fill-rule="evenodd" d="M10.3 3.2c.8-1.3 2.6-1.3 3.4 0l8.4 14.5c.8 1.3-.2 3-1.7 3H3.6c-1.5 0-2.5-1.7-1.7-3zM11 8.5h2l-.2 6.2h-1.6zM12 16.3a1.2 1.2 0 1 0 0 2.4a1.2 1.2 0 1 0 0-2.4z"/></svg>"##,
        ),
    ];
}

impl gpui::AssetSource for Icons {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Ok(Self::FILES.iter().find(|(p, _)| *p == path).map(|(_, svg)| std::borrow::Cow::Borrowed(svg.as_bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        Ok(Self::FILES.iter().filter(|(p, _)| p.starts_with(path)).map(|(p, _)| (*p).into()).collect())
    }
}
