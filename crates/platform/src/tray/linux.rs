//! A StatusNotifierItem over D-Bus (KDE, most wlroots bars, GNOME with the AppIndicator
//! extension) via `ksni`'s blocking API, which runs the D-Bus service on its own thread.
//!
//! The tray host draws the menu, so there is no `MenuOpened` / `MenuClosed` (ksni reports
//! "about to show" but not "closed"; sending only one would leave the app thinking the
//! menu is still open). Activate (usually a left click) sends `Clicked`; the host decides
//! which click opens the menu. Attention swaps in an orange-tinted icon.

use super::{ATTENTION, MenuEntry, Rect, TrayEvent, TrayIcons, tinted};
use ksni::blocking::{Handle, TrayMethods};

struct Sni {
    on_event: Box<dyn Fn(TrayEvent) + Send>,
    /// on, off, tinted on, tinted off; ARGB32 in network byte order.
    icons: [ksni::Icon; 4],
    active: bool,
    attention: bool,
    tooltip: String,
    entries: Vec<MenuEntry>,
}

impl Sni {
    fn emit(&self, event: TrayEvent) {
        (self.on_event)(event);
    }
}

impl ksni::Tray for Sni {
    fn id(&self) -> String {
        "tinyhand".into()
    }

    fn title(&self) -> String {
        "TinyHand".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let index = usize::from(!self.active) + if self.attention { 2 } else { 0 };
        vec![self.icons[index].clone()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: self.tooltip.clone(), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.emit(TrayEvent::Clicked);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, StandardItem};
        self.entries
            .iter()
            .map(|entry| match entry {
                MenuEntry::Separator => ksni::MenuItem::Separator,
                MenuEntry::Item(m) => {
                    let id = m.id.clone();
                    let activate = Box::new(move |this: &mut Sni| this.emit(TrayEvent::MenuItem(id.clone())));
                    // A checkmark item always shows a box, so only checked items get one,
                    // like the macOS menu which shows nothing when off.
                    if m.checked {
                        CheckmarkItem {
                            label: m.label("    "),
                            enabled: m.enabled,
                            visible: m.visible,
                            checked: true,
                            activate,
                            ..Default::default()
                        }
                        .into()
                    } else {
                        StandardItem { label: m.label("    "), enabled: m.enabled, visible: m.visible, activate, ..Default::default() }
                            .into()
                    }
                }
            })
            .collect()
    }
}

pub struct Tray {
    handle: Handle<Sni>,
}

impl Tray {
    pub fn new(icons: TrayIcons, on_event: Box<dyn Fn(TrayEvent) + Send>) -> Result<Tray, String> {
        let (w, h) = (icons.width, icons.height);
        let sni = Sni {
            on_event,
            icons: [
                argb(&icons.on_rgba, w, h),
                argb(&icons.off_rgba, w, h),
                argb(&tinted(&icons.on_rgba, ATTENTION), w, h),
                argb(&tinted(&icons.off_rgba, ATTENTION), w, h),
            ],
            active: false,
            attention: false,
            tooltip: "TinyHand".into(),
            entries: Vec::new(),
        };
        // The tray host may start after us at login; keep waiting for it instead of failing.
        let handle = sni.assume_sni_available(true).spawn().map_err(|e| format!("tray: {e}"))?;
        Ok(Tray { handle })
    }

    pub fn set_active(&mut self, active: bool) {
        self.handle.update(|t| t.active = active);
    }

    pub fn set_attention(&mut self, attention: bool) {
        self.handle.update(|t| t.attention = attention);
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        let tooltip = tooltip.to_string();
        self.handle.update(|t| t.tooltip = tooltip);
    }

    pub fn set_menu(&mut self, entries: Vec<MenuEntry>) {
        self.handle.update(|t| t.entries = entries);
    }

    pub fn frame(&self) -> Option<Rect> {
        None
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let _ = self.handle.shutdown();
    }
}

fn argb(rgba: &[u8], width: u32, height: u32) -> ksni::Icon {
    let data = rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
    ksni::Icon { width: width as i32, height: height as i32, data }
}
