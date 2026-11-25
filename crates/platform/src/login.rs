//! Open at login, and whether this copy of the app looks installed.

use std::path::{Path, PathBuf};

use auto_launch::{AutoLaunch, AutoLaunchBuilder, MacOSLaunchMode};

/// The `X.app` folder if `exe` is inside `X.app/Contents/MacOS/`.
fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension().is_some_and(|e| e == "app");
    is_bundle.then(|| bundle.to_path_buf())
}

/// Where the app lives: the .app bundle if there is one, else the executable.
fn install_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(app_bundle(&exe).unwrap_or(exe))
}

/// On macOS this is a LaunchAgent plist in `~/Library/LaunchAgents/TinyHand.plist`. Its
/// `ProgramArguments` must be an executable (launchd can't run a .app folder), so it
/// points at the binary inside the bundle, which launchd starts like any app launch.
fn auto_launch() -> Result<AutoLaunch, String> {
    let path = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = path.to_str().ok_or("app path is not valid UTF-8")?;
    AutoLaunchBuilder::new()
        .set_app_name("TinyHand")
        .set_app_path(path)
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .build()
        .map_err(|e| e.to_string())
}

/// Whether TinyHand is set to open at login.
pub fn launch_at_login() -> bool {
    auto_launch().and_then(|a| a.is_enabled().map_err(|e| e.to_string())).unwrap_or(false)
}

/// Turn open at login on or off.
pub fn set_launch_at_login(on: bool) -> Result<(), String> {
    let auto = auto_launch()?;
    let result = if on { auto.enable() } else { auto.disable() };
    result.map_err(|e| e.to_string())
}

/// Whether this looks like a real install, where it's fine to turn on open at login the
/// first time the app runs (and not a build folder).
pub fn should_offer_first_run_setup() -> bool {
    let Ok(path) = install_path() else { return false };
    installed(&path, std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

fn installed(path: &Path, home: Option<&Path>) -> bool {
    if cfg!(target_os = "macos") {
        path.starts_with("/Applications") || home.is_some_and(|h| path.starts_with(h.join("Applications")))
    } else {
        let parts: Vec<_> = path.components().collect();
        !parts.windows(2).any(|w| {
            w[0].as_os_str() == "target" && (w[1].as_os_str() == "debug" || w[1].as_os_str() == "release")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_app_bundle() {
        let exe = Path::new("/Applications/TinyHand.app/Contents/MacOS/tinyhand");
        assert_eq!(app_bundle(exe), Some(PathBuf::from("/Applications/TinyHand.app")));
        assert_eq!(app_bundle(Path::new("/repo/target/release/tinyhand")), None);
        assert_eq!(app_bundle(Path::new("/x/Foo/Contents/MacOS/tinyhand")), None);
    }

    #[test]
    fn install_check() {
        let home = Some(Path::new("/Users/me"));
        if cfg!(target_os = "macos") {
            assert!(installed(Path::new("/Applications/TinyHand.app"), home));
            assert!(installed(Path::new("/Users/me/Applications/TinyHand.app"), home));
            assert!(!installed(Path::new("/repo/target/debug/tinyhand"), home));
        } else {
            assert!(installed(Path::new("/usr/bin/tinyhand"), home));
            assert!(!installed(Path::new("/repo/target/debug/tinyhand"), home));
        }
    }
}
