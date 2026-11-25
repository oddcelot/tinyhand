//! Linux: XFixes HideCursor on the root window when an X server is reachable (X11 or
//! XWayland); nothing on pure Wayland.

use x11rb::connection::Connection;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

#[derive(Default)]
pub struct Os {
    /// Kept open while hidden: the server shows the cursor again when it closes.
    x11: Option<(RustConnection, u32)>,
}

fn connect() -> Option<(RustConnection, u32)> {
    std::env::var_os("DISPLAY")?;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    // XFixes requests are only valid after the version handshake.
    conn.xfixes_query_version(5, 0).ok()?.reply().ok()?;
    Some((conn, root))
}

impl Os {
    pub fn hide(&mut self) {
        if self.x11.is_none() {
            self.x11 = connect();
        }
        if let Some((conn, root)) = &self.x11 {
            let _ = conn.xfixes_hide_cursor(*root);
            let _ = conn.flush();
        }
    }

    pub fn show(&mut self) {
        if let Some((conn, root)) = &self.x11 {
            let _ = conn.xfixes_show_cursor(*root);
            let _ = conn.flush();
        }
    }

    pub fn seed(&self) -> i32 {
        0
    }

    /// An XFixes hide holds until shown; nothing to do.
    pub fn reassert(&mut self) {}
}

pub fn enable_background_hiding() {}
