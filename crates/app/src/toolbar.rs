//! The toolbar at the bottom of the screen while the pointer is on (Toolbar.swift), its
//! ink tray, and the hint pill above it (first-run and click-through hints from
//! OverlayController.swift).
//!
//! All three live in one transparent window that sits at the bottom centre of the
//! overlay's display: the bar along its bottom edge, and a strip above it for the ink
//! tray, the hint and tooltips. The window is open only while something in it shows.
//!
//! The window must still let clicks reach what is under its empty parts:
//! - macOS and Windows: it ignores the mouse except while the mouse is over the bar or
//!   the open tray, and those rects pass through the input tap.
//! - Linux (the overlay takes the mouse): the window sits above the overlay; presses and
//!   moves on its empty parts are handed to the overlay as if they had landed there.
//!
//! The state lives in [`Toolbar`] inside the [`Controller`]; [`ToolbarView`] only draws
//! it, like the overlay.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, BoxShadow, Bounds, Context, Corners, Edges, Entity, FontWeight, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, PathBuilder, Pixels, Point, RenderImage, Task, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, canvas, div, hsla, img, point, prelude::*, px, rgba, size,
    svg,
};

use tinyhand_core::settings::{InkChoice, InteractKey};
use tinyhand_core::styles::{self, StyleKind};
use tinyhand_platform as native;
use tinyhand_platform::input::Rect;

use crate::assets;
use crate::controller::{Controller, Input, log};

const BUTTON: f32 = 40.0;
const GAP: f32 = 6.0;
const PAD_X: f32 = 10.0;
pub const BAR_H: f32 = 56.0;
const PILL_W: f32 = 168.0;
/// Room above the bar for the ink tray, the hint and tooltips.
const STRIP: f32 = 64.0;
/// Room below and beside the bar for its shadow.
const PAD: f32 = 10.0;
/// The window is at least this wide so the longest hint fits.
const MIN_WINDOW_W: f32 = 760.0;
/// Gap between the bottom of the usable screen and the bar (`visibleFrame.minY + 14`).
const BOTTOM_GAP: f32 = 14.0;
const SWATCH: f32 = 30.0;
const TRAY_W: f32 = 9.0 * SWATCH + 8.0 * 2.0 + 12.0;
const TRAY_H: f32 = SWATCH + 10.0;
const HINT_H: f32 = 36.0;

const FADE_IN: f32 = 0.15;
const FADE_OUT: f32 = 0.35;
const TRAY_FADE: f32 = 0.12;
const HINT_IN: f32 = 0.2;
const HINT_OUT: f32 = 0.3;
const TOOLTIP_DELAY: Duration = Duration::from_millis(600);

/// A rectangle in overlay points (y down, origin at the top left of the overlay).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl LRect {
    fn new(x: f32, y: f32, w: f32, h: f32) -> LRect {
        LRect { x, y, w, h }
    }

    fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }

    fn offset(&self, dx: f32, dy: f32) -> LRect {
        LRect::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// The smallest rect holding both, like `CGRect.union`.
    fn union(&self, o: &LRect) -> LRect {
        let (x0, y0) = (self.x.min(o.x), self.y.min(o.y));
        let (x1, y1) = ((self.x + self.w).max(o.x + o.w), (self.y + self.h).max(o.y + o.h));
        LRect::new(x0, y0, x1 - x0, y1 - y0)
    }

    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(point(px(self.x), px(self.y)), size(px(self.w), px(self.h)))
    }
}

/// The OverlayController reveal zone around the toolbar (and its open tray): 80 pt to
/// each side, 90 pt above, 10 pt more above and below.
pub fn reveal_zone(f: &LRect) -> LRect {
    let wide = LRect::new(f.x - 80.0, f.y, f.w + 160.0, f.h);
    let up = f.offset(0.0, -90.0);
    let u = wide.union(&up);
    LRect::new(u.x, u.y - 10.0, u.w, u.h + 20.0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Style(String),
    Divider,
    Ink,
    Sound,
    Exit,
    Permission,
}

/// Where everything goes, worked out from the settings and the display.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// The window, in overlay points.
    pub window: LRect,
    /// The bar, the ink tray and the hint, in window points.
    pub bar: LRect,
    pub tray: LRect,
    pub items: Vec<(Item, LRect)>,
}


pub struct Hint {
    parts: Vec<(String, String)>,
    shown_at: Instant,
    until: Instant,
    hiding_since: Option<Instant>,
}

/// A hover waiting to become a tooltip.
#[derive(Clone, PartialEq)]
struct Hover {
    id: String,
    tip: String,
    /// The hovered thing in window points.
    rect: LRect,
    /// Below the thing instead of above (the swatches, which sit at the top).
    below: bool,
}

#[derive(Default)]
pub struct Toolbar {
    window: Option<WindowHandle<ToolbarView>>,
    opening: bool,
    opened_with: Option<LRect>,
    pub revealed: bool,
    /// When `revealed` last changed, and the bar's opacity then.
    changed: Option<(Instant, f32)>,
    pub reveal_until: Option<Instant>,
    pub ink_open: bool,
    ink_changed: Option<(Instant, f32)>,
    hint: Option<Hint>,
    thumbs: HashMap<String, Arc<RenderImage>>,
    loading: HashSet<String>,
    hover: Option<(Hover, Instant)>,
    /// Linux: the mouse in overlay points while it is over the toolbar window.
    pub pointer: Option<(f32, f32)>,
    ignoring: Option<bool>,
    pub layout: Option<Layout>,
    pub(crate) dirty: bool,
}

fn ramp(changed: Option<(Instant, f32)>, on: bool, fade_in: f32, fade_out: f32) -> f32 {
    let Some((at, from)) = changed else { return if on { 1.0 } else { 0.0 } };
    let t = at.elapsed().as_secs_f32();
    if on { (from + t / fade_in).min(1.0) } else { (from - t / fade_out).max(0.0) }
}

impl Toolbar {
    pub fn bar_opacity(&self) -> f32 {
        ramp(self.changed, self.revealed, FADE_IN, FADE_OUT)
    }

    fn tray_opacity(&self) -> f32 {
        ramp(self.ink_changed, self.ink_open, TRAY_FADE, TRAY_FADE)
    }

    fn hint_opacity(&self) -> f32 {
        let Some(h) = &self.hint else { return 0.0 };
        let shown = (h.shown_at.elapsed().as_secs_f32() / HINT_IN).min(1.0);
        match h.hiding_since {
            Some(at) => (shown - at.elapsed().as_secs_f32() / HINT_OUT).max(0.0),
            None => shown,
        }
    }

    fn set_revealed(&mut self, on: bool) {
        if on == self.revealed {
            return;
        }
        let now = self.bar_opacity();
        self.revealed = on;
        self.changed = Some((Instant::now(), now));
        if !on {
            self.set_ink_open(false);
            self.hover = None;
        }
        self.dirty = true;
    }

    fn set_ink_open(&mut self, open: bool) {
        if open == self.ink_open {
            return;
        }
        let now = self.tray_opacity();
        self.ink_open = open;
        self.ink_changed = Some((Instant::now(), now));
        self.dirty = true;
    }

    /// Fades the hint out (`Toolbar.hideHint`).
    pub fn hide_hint(&mut self) {
        if let Some(h) = self.hint.as_mut().filter(|h| h.hiding_since.is_none()) {
            h.hiding_since = Some(Instant::now());
            self.dirty = true;
        }
    }

    fn animating(&self) -> bool {
        let bar = self.bar_opacity();
        let tray = self.tray_opacity();
        let hint = self.hint.as_ref().is_some_and(|h| h.shown_at.elapsed().as_secs_f32() < HINT_IN || h.hiding_since.is_some());
        (bar > 0.0 && bar < 1.0) || (tray > 0.0 && tray < 1.0) || hint
    }

    /// Something in the window shows (or is fading).
    fn showing(&self) -> bool {
        self.revealed || self.bar_opacity() > 0.0 || self.hint.is_some()
    }
}

impl Controller {
    /// The pointer turned on: show the bar for 3 s, and the first-run hint once.
    pub fn toolbar_activated(&mut self) {
        let t = &mut self.toolbar;
        t.reveal_until = Some(Instant::now() + Duration::from_secs(3));
        t.revealed = false;
        t.changed = None;
        t.ink_open = false;
        t.ink_changed = None;
        t.hint = None;
        t.pointer = None;
        let forced = self.hooks.show.as_deref() == Some("hint");
        // The other TH_SHOW screenshots want the bar alone.
        let other_shot = self.hooks.show.as_deref().is_some_and(|s| s != "hint");
        if self.needs_permission() || other_shot || (self.settings.first_run_hint_shown && !forced) {
            return;
        }
        if !forced {
            self.settings.first_run_hint_shown = true;
            self.save_settings();
        }
        let mac = cfg!(target_os = "macos");
        let mut parts: Vec<(String, String)> = vec![("Click".into(), "to tap".into()), ("Drag".into(), "to draw".into())];
        if self.input == Input::Watch {
            if self.settings.interact_key != InteractKey::Off {
                parts.push((format!("Hold {}", interact_key_name(self.settings.interact_key, mac)), "to click for real".into()));
            }
            if self.settings.double_shift_click_through {
                parts.push((double_shift(mac).into(), "to click through".into()));
            }
        }
        parts.push(("Esc".into(), "to exit".into()));
        self.show_hint(parts, Duration::from_secs(10));
        self.toolbar.reveal_until = Some(Instant::now() + Duration::from_secs(10));
    }

    /// Double-⇧ toggled click-through: say so for 1.5 s.
    pub fn click_through_hint(&mut self, on: bool) {
        let shift = double_shift(cfg!(target_os = "macos"));
        let parts = if on {
            vec![("Clicks go through".into(), "the hand stays".into()), (shift.into(), "to tap again".into())]
        } else {
            vec![("Taps".into(), "clicks don't reach apps".into()), (shift.into(), "to click through".into())]
        };
        self.show_hint(parts, Duration::from_millis(1500));
    }

    fn show_hint(&mut self, parts: Vec<(String, String)>, for_how_long: Duration) {
        let now = Instant::now();
        self.toolbar.hint = Some(Hint { parts, shown_at: now, until: now + for_how_long, hiding_since: None });
        self.toolbar.dirty = true;
    }

    /// The pointer turned off: close everything.
    pub fn toolbar_deactivated(&mut self, cx: &mut Context<Self>) {
        let t = &mut self.toolbar;
        t.revealed = false;
        t.changed = None;
        t.ink_open = false;
        t.ink_changed = None;
        t.hint = None;
        t.hover = None;
        t.pointer = None;
        t.reveal_until = None;
        self.close_toolbar(cx);
    }

    /// The tap isn't running though it should (Swift: `!inputTap.isInstalled`).
    pub fn needs_permission(&self) -> bool {
        if self.hooks.show.as_deref() == Some("toolbar-untrusted") {
            return true;
        }
        self.active && self.taps_input() && !self.has_tap()
    }

    /// The styles on the bar: the pinned ones, plus the current one if it isn't pinned.
    fn toolbar_styles(&self) -> Vec<String> {
        let mut ids = self.settings.pinned_existing(self.custom.hands());
        let current = &self.scene.style.id;
        if !ids.contains(current) {
            ids.push(current.clone());
        }
        ids
    }

    /// The display's size and the bottom of its usable part, in overlay points.
    fn screen_area(&self) -> (f32, f32, f32) {
        if let Some(d) = self.display {
            let bottom = native::menus::visible_frame(&d).map(|v| (v.y + v.h - d.y) as f32).unwrap_or(d.h as f32);
            return (d.w as f32, d.h as f32, bottom);
        }
        (self.scene.width, self.scene.height, self.scene.height)
    }

    pub fn toolbar_layout(&self) -> Layout {
        let mut kinds: Vec<Item> = self.toolbar_styles().into_iter().map(Item::Style).collect();
        kinds.push(Item::Divider);
        if self.settings.show_ink_button {
            kinds.push(Item::Ink);
        }
        kinds.push(Item::Sound);
        kinds.push(Item::Exit);
        if self.needs_permission() {
            kinds.push(Item::Permission);
        }
        let width_of = |i: &Item| match i {
            Item::Divider => 1.0,
            Item::Permission => PILL_W,
            _ => BUTTON,
        };
        let bar_w = 2.0 * PAD_X + kinds.iter().map(width_of).sum::<f32>() + GAP * (kinds.len() as f32 - 1.0);

        let (w, _h, bottom) = self.screen_area();
        let win_w = (bar_w + 2.0 * PAD).max(MIN_WINDOW_W).min(w.max(bar_w));
        let win_h = STRIP + BAR_H + PAD;
        let window = LRect::new(((w - win_w) / 2.0).round(), bottom - BOTTOM_GAP - BAR_H - STRIP, win_w, win_h);
        let bar = LRect::new(((win_w - bar_w) / 2.0).round(), STRIP, bar_w, BAR_H);

        let mut items = Vec::new();
        let mut x = bar.x + PAD_X;
        for k in kinds {
            let iw = width_of(&k);
            let ih = match k {
                Item::Divider => 24.0,
                Item::Permission => 32.0,
                _ => BUTTON,
            };
            items.push((k, LRect::new(x, bar.y + (BAR_H - ih) / 2.0, iw, ih)));
            x += iw + GAP;
        }
        let ink_cx = items.iter().find(|(i, _)| *i == Item::Ink).map(|(_, r)| r.x + r.w / 2.0).unwrap_or(bar.x + bar.w / 2.0);
        let tray_x = (ink_cx - TRAY_W / 2.0).clamp(0.0, (win_w - TRAY_W).max(0.0)).round();
        let tray = LRect::new(tray_x, bar.y - 8.0 - TRAY_H, TRAY_W, TRAY_H);
        Layout { window, bar, tray, items }
    }

    /// The bar (and the open tray) in overlay points, for the reveal zone and the tap.
    fn toolbar_frames(&self, layout: &Layout) -> Vec<LRect> {
        let o = (layout.window.x, layout.window.y);
        let mut v = vec![layout.bar.offset(o.0, o.1)];
        if self.toolbar.ink_open {
            v.push(layout.tray.offset(o.0, o.1));
        }
        v
    }

    /// Global rects the input tap must let through: the bar and tray while shown.
    pub fn toolbar_pass_through(&self) -> Vec<Rect> {
        let (Some(layout), true) = (self.toolbar.layout.as_ref(), self.toolbar.revealed) else { return Vec::new() };
        let (dx, dy) = self.display.map(|d| (d.x, d.y)).unwrap_or((0.0, 0.0));
        self.toolbar_frames(layout)
            .into_iter()
            .map(|r| Rect::new(r.x as f64 + dx, r.y as f64 + dy, r.w as f64, r.h as f64))
            .collect()
    }

    /// Once a frame (`OverlayController.updateZones`, the toolbar half): reveal or hide,
    /// open or close the window, and let the mouse through where nothing is.
    pub fn update_toolbar(&mut self, mouse: (f32, f32), cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        let now = Instant::now();
        let layout = self.toolbar_layout();
        let frames = self.toolbar_frames(&layout);
        let zone = frames.iter().skip(1).fold(frames[0], |a, b| a.union(b));
        let near = reveal_zone(&zone).contains(mouse);
        let drawing = self.scene.ink.is_drawing();
        let until = self.toolbar.reveal_until.is_some_and(|u| now < u);
        let forced = matches!(self.hooks.show.as_deref(), Some("toolbar" | "toolbar-untrusted" | "ink" | "tooltip"));
        let want = self.needs_permission() || forced || (self.settings.show_toolbar && !drawing && (near || until));
        self.toolbar.set_revealed(want);
        if !self.settings.show_ink_button {
            self.toolbar.set_ink_open(false);
        }
        if forced && self.hooks.show.as_deref() == Some("ink") && self.toolbar.bar_opacity() >= 1.0 {
            self.toolbar.set_ink_open(true);
        }
        let exit = layout.items.iter().find(|(i, _)| *i == Item::Exit).map(|(_, r)| *r);
        if let (Some("tooltip"), None, Some(rect)) = (self.hooks.show.as_deref(), &self.toolbar.hover, exit) {
            let hover = Hover { id: "exit".into(), tip: "Exit  (Esc)".into(), rect, below: false };
            self.toolbar.hover = Some((hover, now));
        }

        if let Some(h) = self.toolbar.hint.as_mut() {
            if now >= h.until && h.hiding_since.is_none() {
                h.hiding_since = Some(now);
            }
            if h.hiding_since.is_some_and(|at| at.elapsed().as_secs_f32() > HINT_OUT) {
                self.toolbar.hint = None;
                self.toolbar.dirty = true;
            }
        }
        if self.toolbar.layout.as_ref() != Some(&layout) {
            self.toolbar.dirty = true;
        }
        self.toolbar.layout = Some(layout.clone());

        let showing = self.toolbar.showing();
        match (&self.toolbar.window, showing) {
            (None, true) if !self.toolbar.opening => self.open_toolbar(&layout, cx),
            (Some(_), true) if self.toolbar.opened_with != Some(layout.window) => {
                self.close_toolbar(cx);
                self.open_toolbar(&layout, cx);
            }
            (Some(_), false) => self.close_toolbar(cx),
            _ => {}
        }

        // Let clicks through the window's empty parts (macOS, Windows).
        if let (Input::Watch, Some(handle)) = (self.input, self.toolbar.window) {
            let local = (mouse.0 - layout.window.x, mouse.1 - layout.window.y);
            let over = self.toolbar.revealed
                && (layout.bar.contains(local) || (self.toolbar.ink_open && layout.tray.contains(local)));
            if self.toolbar.ignoring != Some(!over) {
                self.toolbar.ignoring = Some(!over);
                let ignore = !over;
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| native::set_ignores_mouse(window, ignore));
                });
            }
        }

        if std::mem::take(&mut self.toolbar.dirty) || self.toolbar.animating() {
            self.refresh_toolbar(cx);
        }
    }

    fn refresh_toolbar(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.toolbar.window {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.refresh());
            });
        }
    }

    fn close_toolbar(&mut self, cx: &mut Context<Self>) {
        self.toolbar.opened_with = None;
        self.toolbar.pointer = None;
        self.toolbar.hover = None;
        self.toolbar.ignoring = None;
        if let Some(handle) = self.toolbar.window.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    fn open_toolbar(&mut self, layout: &Layout, cx: &mut Context<Self>) {
        self.toolbar.opening = true;
        self.toolbar.opened_with = Some(layout.window);
        self.load_thumbnails(cx);
        let this = cx.entity();
        let r = layout.window;
        let display = self.display;
        let kind = self.panel_kind(r);
        cx.defer(move |cx: &mut App| {
            let display_id = display.map(|d| gpui::DisplayId::from(d.id)).or_else(|| cx.primary_display().map(|d| d.id()));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(r.bounds())),
                display_id,
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                focus: false,
                show: true,
                kind,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            };
            let view_of = this.clone();
            let opened = cx.open_window(options, move |window, cx| {
                if let Err(e) = native::make_panel(window) {
                    log(&format!("toolbar setup: {e}"));
                }
                if let Some(d) = display {
                    let _ = native::place_window(window, d.x + r.x as f64, d.y + r.y as f64, r.w as f64, r.h as f64);
                }
                cx.new(|_| ToolbarView { controller: view_of, tooltip_timer: None })
            });
            this.update(cx, |c, cx| {
                c.toolbar.opening = false;
                match opened {
                    Ok(handle) if !c.active || c.toolbar.opened_with != Some(r) => {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    Ok(handle) => {
                        if let Some(d) = display {
                            let _ = handle.update(cx, |_, window, _| {
                                let _ = native::make_panel(window);
                                native::place_window(window, d.x + r.x as f64, d.y + r.y as f64, r.w as f64, r.h as f64)
                            });
                        }
                        c.toolbar.window = Some(handle);
                        c.toolbar.ignoring = None;
                    }
                    Err(e) => log(&format!("can't open the toolbar: {e}")),
                }
            });
        });
    }

    /// The kind of window for the toolbar: a pop-up, or on a layer-shell compositor a
    /// layer surface anchored to the bottom, above the overlay.
    #[allow(unused_variables)]
    fn panel_kind(&self, r: LRect) -> WindowKind {
        #[cfg(target_os = "linux")]
        if let WindowKind::LayerShell(_) = (self.window_kind)() {
            use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
            let bottom = (self.scene.height - r.y - r.h).max(0.0);
            return WindowKind::LayerShell(LayerShellOptions {
                namespace: "tinyhand-toolbar".into(),
                layer: Layer::Overlay,
                anchor: Anchor::BOTTOM,
                exclusive_zone: Some(px(-1.0)),
                margin: Some((px(0.0), px(0.0), px(bottom), px(0.0))),
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            });
        }
        WindowKind::PopUp
    }

    /// Makes the button pictures of the bar's hands, off the main thread.
    fn load_thumbnails(&mut self, cx: &mut Context<Self>) {
        for id in self.toolbar_styles() {
            if self.toolbar.thumbs.contains_key(&id) || self.toolbar.loading.contains(&id) {
                continue;
            }
            let Some(path) = assets::hand_image(&id, &self.custom) else { continue };
            self.toolbar.loading.insert(id.clone());
            cx.spawn(async move |this, cx| {
                let made = cx.background_executor().spawn(async move { tinyhand_core::hand::thumbnail(&path, 128) }).await;
                let _ = this.update(cx, |c, cx| {
                    c.toolbar.loading.remove(&id);
                    match made {
                        Ok(image) => {
                            c.toolbar.thumbs.insert(id, assets::render_image(image));
                            c.toolbar.dirty = true;
                            c.refresh_toolbar(cx);
                        }
                        Err(e) => log(&format!("no thumbnail for {id}: {e}")),
                    }
                });
            })
            .detach();
        }
    }

    /// After custom hands changed: their pictures may have too.
    pub fn forget_thumbnails(&mut self) {
        self.toolbar.thumbs.clear();
    }

    /// What the toolbar window draws this frame.
    fn toolbar_snapshot(&self) -> Option<Snapshot> {
        let layout = self.toolbar.layout.clone()?;
        let t = &self.toolbar;
        let all = styles::all_styles(self.custom.hands());
        let styles = layout
            .items
            .iter()
            .filter_map(|(i, _)| match i {
                Item::Style(id) => Some(id.clone()),
                _ => None,
            })
            .map(|id| {
                let style = all.iter().find(|s| s.id == id);
                StyleButton {
                    title: style.map(|s| s.title.clone()).unwrap_or_else(|| id.clone()),
                    laser: style.is_some_and(|s| s.kind == StyleKind::Laser),
                    thumb: t.thumbs.get(&id).cloned(),
                    selected: id == self.scene.style.id,
                    id,
                }
            })
            .collect();
        let tooltip = t.hover.as_ref().filter(|(_, at)| at.elapsed() >= TOOLTIP_DELAY).map(|(h, _)| h.clone());
        Some(Snapshot {
            bar_opacity: t.bar_opacity(),
            tray_opacity: t.tray_opacity(),
            hint_opacity: t.hint_opacity(),
            revealed: t.revealed,
            styles,
            ink: self.settings.ink_choice.resolved(self.scene.style.ink),
            ink_choice: self.settings.ink_choice,
            ink_open: t.ink_open,
            sound_on: self.sound_on(),
            hint: t.hint.as_ref().map(|h| h.parts.clone()),
            tooltip,
            tooltip_pending: t.hover.as_ref().and_then(|(_, at)| TOOLTIP_DELAY.checked_sub(at.elapsed())),
            animating: t.animating(),
            capture: self.input == Input::Capture,
            layout,
        })
    }

    fn toolbar_hover(&mut self, hover: Hover, on: bool) {
        let same = self.toolbar.hover.as_ref().is_some_and(|(h, _)| h.id == hover.id);
        if on && !same {
            self.toolbar.hover = Some((hover, Instant::now()));
        } else if !on && same {
            self.toolbar.hover = None;
        }
    }

    fn toolbar_action(&mut self, item: &Item, cx: &mut Context<Self>) {
        log(&format!("toolbar: {item:?}"));
        self.toolbar.hover = None;
        match item {
            Item::Style(id) => {
                self.pick(id, cx);
            }
            Item::Ink => {
                let open = !self.toolbar.ink_open;
                self.toolbar.set_ink_open(open);
            }
            Item::Sound => {
                let on = !self.sound_on();
                self.set_sound_on(on);
            }
            Item::Exit => self.exit(cx),
            Item::Permission => {
                self.exit(cx);
                self.show_welcome(cx);
            }
            Item::Divider => {}
        }
        self.toolbar.dirty = true;
        self.wake(cx);
    }

    fn pick_ink(&mut self, choice: InkChoice, cx: &mut Context<Self>) {
        self.toolbar.hover = None;
        self.update_settings(cx, |s| s.ink_choice = choice);
        self.toolbar.set_ink_open(false);
    }
}

/// How the interact key is written in a hint.
fn interact_key_name(key: InteractKey, mac: bool) -> &'static str {
    if mac {
        return key.symbol();
    }
    match key {
        InteractKey::Option => "Alt",
        InteractKey::Command => "Win",
        InteractKey::Control => "Ctrl",
        InteractKey::Off => "",
    }
}

fn double_shift(mac: bool) -> &'static str {
    if mac { "Double-tap ⇧" } else { "Double-tap Shift" }
}

struct StyleButton {
    id: String,
    title: String,
    laser: bool,
    thumb: Option<Arc<RenderImage>>,
    selected: bool,
}

struct Snapshot {
    layout: Layout,
    bar_opacity: f32,
    tray_opacity: f32,
    hint_opacity: f32,
    revealed: bool,
    styles: Vec<StyleButton>,
    ink: u32,
    ink_choice: InkChoice,
    ink_open: bool,
    sound_on: bool,
    hint: Option<Vec<(String, String)>>,
    tooltip: Option<Hover>,
    tooltip_pending: Option<Duration>,
    animating: bool,
    capture: bool,
}

// ---------------------------------------------------------------- drawing

fn white(a: f32) -> Hsla {
    hsla(0.0, 0.0, 1.0, a)
}

fn rgb(c: u32, a: f32) -> Hsla {
    let mut c: Hsla = gpui::rgb(c).into();
    c.a = a;
    c
}

/// The dark glass of `NSVisualEffectView.darkGlass`: GPUI can't blur what is behind a
/// window, so a darker translucent fill stands in for the HUD material, with the same
/// tint and hairline border on top.
pub fn glass(radius: f32) -> gpui::Div {
    div()
        .rounded(px(radius))
        .bg(rgba(0x242428e6))
        .border_1()
        .border_color(white(0.12))
        .shadow(vec![BoxShadow {
            color: hsla(0.0, 0.0, 0.0, 0.35),
            offset: point(px(0.0), px(3.0)),
            blur_radius: px(8.0),
            spread_radius: px(0.0),
            inset: false,
        }])
}

fn disc(window: &mut Window, c: Point<Pixels>, r: f32, colour: Hsla) {
    let b = Bounds::new(point(c.x - px(r), c.y - px(r)), size(px(2.0 * r), px(2.0 * r)));
    window.paint_quad(gpui::quad(b, Corners::all(px(r)), colour, Edges::all(px(0.0)), white(0.0), gpui::BorderStyle::Solid));
}

fn ellipse(window: &mut Window, b: Bounds<Pixels>, colour: Hsla) {
    let r = b.size.width.min(b.size.height) / 2.0;
    window.paint_quad(gpui::quad(b, Corners::all(r), colour, Edges::all(px(0.0)), white(0.0), gpui::BorderStyle::Solid));
}

/// `Toolbar.inkDot`: a glossy dot with a soft shadow, `unit` = button side / 64.
fn ink_dot(window: &mut Window, c: Point<Pixels>, unit: f32, colour: Option<u32>) {
    let r = 13.0 * unit;
    disc(window, point(c.x, c.y + px(unit)), r + 1.5 * unit, hsla(0.0, 0.0, 0.0, 0.35));
    match colour {
        Some(colour) => {
            disc(window, c, r, rgb(colour, 1.0));
            let hl = Bounds::new(point(c.x - px(0.55 * r), c.y - px(0.65 * r)), size(px(0.7 * r), px(0.5 * r)));
            ellipse(window, hl, white(0.35));
        }
        None => {
            // `Toolbar.rainbowDot`: six slices from the top, anticlockwise.
            let colours = [0xFF3B30, 0xFF9500, 0xFFCC00, 0x34C759, 0x007AFF, 0xAF52DE];
            for (i, colour) in colours.iter().enumerate() {
                let mut b = PathBuilder::fill();
                b.move_to(c);
                let steps = 10;
                for s in 0..=steps {
                    let deg = 90.0 + 60.0 * (i as f32 + s as f32 / steps as f32);
                    let a = deg.to_radians();
                    b.line_to(point(c.x + px(r * a.cos()), c.y - px(r * a.sin())));
                }
                b.close();
                if let Ok(path) = b.build() {
                    window.paint_path(path, rgb(*colour, 1.0));
                }
            }
        }
    }
}

/// `Toolbar.icon(for: .laser)`: a red dot in a soft red glow.
fn laser_icon(window: &mut Window, c: Point<Pixels>, unit: f32) {
    let red = styles::INK_RED;
    let glow = 18.0 * unit;
    let rings = 16;
    for k in 0..rings {
        let r = glow * (1.0 - k as f32 / rings as f32);
        disc(window, c, r, rgb(red, 0.07));
    }
    disc(window, c, 7.0 * unit, rgb(red, 1.0));
    disc(window, c, 3.0 * unit, gpui::Rgba { r: 1.0, g: 0.86, b: 0.82, a: 0.95 }.into());
}

fn painted(f: impl Fn(Point<Pixels>, &mut Window) + 'static) -> impl IntoElement {
    canvas(|_, _, _| {}, move |bounds, _, window, _| f(bounds.center(), window)).size_full()
}

pub struct ToolbarView {
    controller: Entity<Controller>,
    tooltip_timer: Option<Task<()>>,
}

impl ToolbarView {
    /// A 40×40 button with a highlight when selected (white 0.18) or open.
    fn button(&self, key: &str, item: Item, rect: LRect, tip: String, lit: bool, enabled: bool) -> gpui::Stateful<gpui::Div> {
        let c = self.controller.clone();
        let hover_c = self.controller.clone();
        let hover = Hover { id: key.to_string(), tip, rect, below: false };
        let mut d = div()
            .id(gpui::SharedString::from(key.to_string()))
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .h(px(rect.h))
            .rounded(px(10.0))
            .flex()
            .items_center()
            .justify_center();
        if lit {
            d = d.bg(white(0.18));
        } else if enabled {
            d = d.hover(|s| s.bg(white(0.08)));
        }
        if enabled {
            d = d
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| {
                    let item = item.clone();
                    c.update(cx, |c, cx| c.toolbar_action(&item, cx));
                })
                .on_hover(move |on, _, cx| {
                    let (hover, on) = (hover.clone(), *on);
                    hover_c.update(cx, |c, cx| {
                        c.toolbar_hover(hover, on);
                        c.toolbar.dirty = true;
                        c.refresh_toolbar(cx);
                    });
                });
        }
        d
    }

    fn bar(&self, s: &Snapshot) -> impl IntoElement {
        let l = &s.layout;
        let enabled = s.revealed;
        let mut bar = div().absolute().size_full().opacity(s.bar_opacity).child(
            glass(16.0).absolute().left(px(l.bar.x)).top(px(l.bar.y)).w(px(l.bar.w)).h(px(l.bar.h)),
        );
        for (item, rect) in &l.items {
            let el: gpui::AnyElement = match item {
                Item::Style(id) => {
                    let Some(b) = s.styles.iter().find(|b| &b.id == id) else { continue };
                    let inner: gpui::AnyElement = if b.laser {
                        painted(|c, w| laser_icon(w, c, BUTTON / 64.0)).into_any_element()
                    } else if let Some(t) = &b.thumb {
                        img(t.clone()).size(px(BUTTON)).into_any_element()
                    } else {
                        div().into_any_element()
                    };
                    self.button(&format!("style-{id}"), item.clone(), *rect, b.title.clone(), b.selected, enabled)
                        .child(inner)
                        .into_any_element()
                }
                Item::Divider => div()
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w))
                    .h(px(rect.h))
                    .bg(white(0.18))
                    .into_any_element(),
                Item::Ink => {
                    let colour = s.ink;
                    self.button("ink", Item::Ink, *rect, "Ink colour".into(), s.ink_open, enabled)
                        .child(painted(move |c, w| ink_dot(w, c, BUTTON / 64.0, Some(colour))))
                        .into_any_element()
                }
                Item::Sound => {
                    let (icon, tip, alpha) = if s.sound_on {
                        ("icons/speaker.svg", "Tap sound on — click to mute", 0.85)
                    } else {
                        ("icons/speaker-off.svg", "Tap sound off — click to unmute", 0.45)
                    };
                    self.button("sound", Item::Sound, *rect, tip.into(), false, enabled)
                        .child(svg().path(icon).size(px(19.0)).text_color(white(alpha)))
                        .into_any_element()
                }
                Item::Exit => self
                    .button("exit", Item::Exit, *rect, "Exit  (Esc)".into(), false, enabled)
                    .child(svg().path("icons/xmark.svg").size(px(17.0)).text_color(white(0.85)))
                    .into_any_element(),
                Item::Permission => self
                    .button(
                        "permission",
                        Item::Permission,
                        *rect,
                        "TinyHand needs Accessibility permission to intercept clicks and draw".into(),
                        false,
                        enabled,
                    )
                    .bg(rgba(0xff9500e6))
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(gpui::black())
                    .child(svg().path("icons/warning.svg").size(px(14.0)).text_color(gpui::black()))
                    .child("Allow clicks & drawing")
                    .into_any_element(),
            };
            bar = bar.child(el);
        }
        bar
    }

    fn tray(&self, s: &Snapshot) -> impl IntoElement {
        let t = s.layout.tray;
        let mut row = div().flex().flex_row().gap(px(2.0)).px(px(6.0)).py(px(5.0));
        for (i, choice) in InkChoice::ALL.into_iter().enumerate() {
            let c = self.controller.clone();
            let hover_c = self.controller.clone();
            let rect = LRect::new(t.x + 6.0 + i as f32 * (SWATCH + 2.0), t.y + 5.0, SWATCH, SWATCH);
            let hover = Hover { id: format!("ink-{i}"), tip: choice.title().into(), rect, below: true };
            let colour = choice.colour();
            let mut sw = div()
                .id(("swatch", i))
                .size(px(SWATCH))
                .rounded_full()
                .child(painted(move |c, w| ink_dot(w, c, SWATCH / 64.0, colour)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| c.update(cx, |c, cx| c.pick_ink(choice, cx)))
                .on_hover(move |on, _, cx| {
                    let (hover, on) = (hover.clone(), *on);
                    hover_c.update(cx, |c, cx| {
                        c.toolbar_hover(hover, on);
                        c.toolbar.dirty = true;
                        c.refresh_toolbar(cx);
                    });
                });
            sw = if choice == s.ink_choice { sw.bg(white(0.2)) } else { sw.hover(|st| st.bg(white(0.08))) };
            row = row.child(sw);
        }
        glass(14.0)
            .absolute()
            .left(px(t.x))
            .top(px(t.y))
            .w(px(t.w))
            .h(px(t.h))
            .opacity(s.tray_opacity)
            .child(row)
    }

    fn hint(&self, s: &Snapshot, parts: &[(String, String)]) -> impl IntoElement {
        let mut row = div().flex().flex_row().gap(px(14.0)).text_size(px(12.5)).whitespace_nowrap();
        for (key, text) in parts {
            row = row.child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(3.5))
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_color(white(1.0)).child(key.clone()))
                    .child(div().text_color(white(0.7)).child(text.clone())),
            );
        }
        let bar = s.layout.bar;
        // Centred over the bar, its bottom 12 pt above it.
        div()
            .absolute()
            .left(px(0.0))
            .w_full()
            .top(px(bar.y - 12.0 - HINT_H))
            .h(px(HINT_H))
            .flex()
            .justify_center()
            .opacity(s.hint_opacity)
            .child(glass(12.0).h(px(HINT_H)).px(px(16.0)).flex().items_center().child(row))
    }

    fn tooltip(&self, s: &Snapshot, h: &Hover) -> impl IntoElement {
        let w = s.layout.window.w;
        let width = (h.tip.chars().count() as f32 * 6.4 + 16.0).min(w);
        let x = (h.rect.x + h.rect.w / 2.0 - width / 2.0).clamp(0.0, (w - width).max(0.0));
        let y = if h.below { h.rect.y + h.rect.h + 4.0 } else { h.rect.y - 26.0 };
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .h(px(22.0))
            .flex()
            .justify_center()
            .child(
                div()
                    .h(px(22.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .bg(rgba(0x2a2a2ef0))
                    .border_1()
                    .border_color(white(0.14))
                    .text_size(px(11.5))
                    .text_color(white(0.9))
                    .whitespace_nowrap()
                    .child(h.tip.clone()),
            )
    }
}

impl Render for ToolbarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(s) = self.controller.read(cx).toolbar_snapshot() else { return div() };
        if s.animating {
            window.request_animation_frame();
        }
        if let (Some(wait), None) = (s.tooltip_pending.filter(|d| !d.is_zero()), &self.tooltip_timer) {
            self.tooltip_timer = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(wait + Duration::from_millis(10)).await;
                let _ = this.update(cx, |v, cx| {
                    v.tooltip_timer = None;
                    cx.notify();
                });
            }));
        }

        let mut root = div().size_full().relative().font_family(".SystemUIFont");
        if s.capture {
            // Linux: the overlay is underneath and can't see the mouse here; hand it the
            // moves, presses and releases that land on the empty parts.
            let origin = (s.layout.window.x, s.layout.window.y);
            let (c1, c2, c3) = (self.controller.clone(), self.controller.clone(), self.controller.clone());
            root = root
                .on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
                    let at = (origin.0 + f32::from(e.position.x), origin.1 + f32::from(e.position.y));
                    c1.update(cx, |c, cx| {
                        c.toolbar.pointer = Some(at);
                        c.wake(cx);
                    });
                })
                .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _, cx| {
                    let at = (origin.0 + f32::from(e.position.x), origin.1 + f32::from(e.position.y));
                    c2.update(cx, |c, cx| {
                        c.captured_down(at);
                        c.wake(cx);
                    });
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    c3.update(cx, |c, cx| {
                        c.captured_up();
                        c.wake(cx);
                    });
                });
        }
        if s.bar_opacity > 0.0 {
            root = root.child(self.bar(&s));
        }
        if s.tray_opacity > 0.0 {
            root = root.child(self.tray(&s));
        }
        if let Some(parts) = &s.hint {
            root = root.child(self.hint(&s, parts));
        }
        if let Some(h) = &s.tooltip {
            root = root.child(self.tooltip(&s, h));
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_zone_matches_the_swift_formula() {
        // A 400×56 bar at (500, 800): 80 pt each side, 90 pt above, 10 pt more above and below.
        let z = reveal_zone(&LRect::new(500.0, 800.0, 400.0, 56.0));
        assert_eq!(z, LRect::new(420.0, 700.0, 560.0, 166.0));
        assert!(z.contains((430.0, 705.0)));
        assert!(!z.contains((410.0, 820.0)));
        assert!(!z.contains((600.0, 867.0)));
    }

    #[test]
    fn fades_ramp_from_where_they_were() {
        let past = Instant::now() - Duration::from_secs(1);
        assert_eq!(ramp(None, true, FADE_IN, FADE_OUT), 1.0);
        assert_eq!(ramp(Some((past, 0.0)), true, FADE_IN, FADE_OUT), 1.0);
        assert_eq!(ramp(Some((past, 1.0)), false, FADE_IN, FADE_OUT), 0.0);
        assert!(ramp(Some((Instant::now(), 0.5)), false, FADE_IN, FADE_OUT) > 0.45);
    }
}
