//! Hiding the system cursor while the hand is shown, ported from `CursorHider.swift`.
//!
//! - **macOS**: `CGDisplayHideCursor`, plus the private `CGSSetConnectionProperty
//!   ("SetsCursorInBackground")` so an accessory app may hide it, and `CGSCurrentCursorSeed`
//!   to notice another app showing it again ([`CursorHider::reassert`]). Also [`DockArea`].
//! - **Windows**: every system cursor (`OCR_*`) is swapped for a blank one with
//!   `SetSystemCursor`, and `SystemParametersInfoW(SPI_SETCURSORS)` puts the user's cursors
//!   back. This is session-wide: if the process dies without [`CursorHider::show`] or `Drop`
//!   running, the cursor stays blank until the next sign-in or a cursor-scheme change.
//! - **Linux X11** (`DISPLAY` set, including XWayland): XFixes `HideCursor` on the root
//!   window. The X server shows it again when our connection closes, so a crash can't leave
//!   it hidden. Under XWayland this hides it over X windows only.
//! - **Wayland** without `DISPLAY`: nothing; Wayland has no way to hide the cursor over other
//!   apps' surfaces.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod imp {
    #[derive(Default)]
    pub struct Os;
    impl Os {
        pub fn hide(&mut self) {}
        pub fn show(&mut self) {}
        pub fn seed(&self) -> i32 {
            0
        }
        pub fn reassert(&mut self) {}
    }
    pub fn enable_background_hiding() {}
}

#[cfg(target_os = "macos")]
pub use macos::DockArea;

use std::time::{Duration, Instant};

/// Hides the system cursor. `show` (and dropping it) undoes every hide.
#[derive(Default)]
pub struct CursorHider {
    hidden: bool,
    hidden_at: Option<Instant>,
    last_seed: i32,
    last_reassert: Option<Instant>,
    os: imp::Os,
}

impl CursorHider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hide(&mut self) {
        if self.hidden {
            return;
        }
        self.hidden = true;
        self.hidden_at = Some(Instant::now());
        self.os.hide();
    }

    /// Shows the cursor, balancing every hide and reassert since `hide`.
    pub fn show(&mut self) {
        if !self.hidden {
            return;
        }
        self.hidden = false;
        self.os.show();
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// True for the first second after `hide`.
    pub fn is_settling(&self) -> bool {
        self.hidden && self.since_hidden() < Duration::from_secs(1)
    }

    fn since_hidden(&self) -> Duration {
        self.hidden_at.map_or(Duration::MAX, |t| t.elapsed())
    }

    /// Hides again if another app may have shown the cursor: when the cursor seed changed
    /// (macOS), or after an interval of 0 s if `urgent`, 30 ms if `fighting` (over the Dock),
    /// 50 ms in the first second, else 1 s. Only macOS needs it; elsewhere a hide holds.
    pub fn reassert(&mut self, fighting: bool, urgent: bool) {
        if !self.hidden {
            return;
        }
        let seed = self.os.seed();
        let interval = if urgent {
            0.0
        } else if fighting {
            0.03
        } else if self.since_hidden() < Duration::from_secs(1) {
            0.05
        } else {
            1.0
        };
        let since = self.last_reassert.map_or(f64::INFINITY, |t| t.elapsed().as_secs_f64());
        if seed == self.last_seed && since <= interval {
            return;
        }
        self.last_seed = seed;
        self.last_reassert = Some(Instant::now());
        self.os.reassert();
    }
}

impl Drop for CursorHider {
    fn drop(&mut self) {
        self.show();
    }
}

/// macOS: lets this (accessory) app hide the cursor while another app is frontmost. Call
/// once at launch; [`CursorHider::reassert`] calls it again. Does nothing elsewhere.
pub fn enable_background_hiding() {
    imp::enable_background_hiding()
}

/// Whether (`x`, `y`) in global top-left points is over the Dock, with an 8 pt margin.
/// macOS only (false elsewhere). Reads the Dock's frame at most once a second, in the
/// background, so the answer can be a second old.
pub fn dock_contains(x: f64, y: f64) -> bool {
    #[cfg(target_os = "macos")]
    {
        static DOCK: std::sync::OnceLock<DockArea> = std::sync::OnceLock::new();
        DOCK.get_or_init(DockArea::new).contains(x, y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (x, y);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookkeeping_without_hiding() {
        // Never calls hide, so the real cursor is left alone.
        let mut c = CursorHider::new();
        assert!(!c.is_settling());
        c.reassert(true, true);
        c.show();
        assert!(!c.is_hidden());
        let _ = dock_contains(0.0, 0.0);
    }

    /// Hides the real cursor for half a second. Only with TINYHAND_CURSOR_TEST=1.
    #[test]
    #[ignore]
    fn live_hide_show() {
        if std::env::var("TINYHAND_CURSOR_TEST").as_deref() != Ok("1") {
            return;
        }
        enable_background_hiding();
        let mut c = CursorHider::new();
        c.hide();
        c.hide();
        assert!(c.is_settling());
        c.reassert(false, true);
        std::thread::sleep(Duration::from_millis(500));
        c.show();
        assert!(!c.is_hidden());
    }
}
