//! The settings window: SettingsWindow.swift and SettingsView.swift. A sidebar with six
//! sections and a scrolling page; each page is in its own file. Every change goes
//! through the controller (`update_settings`, `set_shortcut`, ...), which saves it and
//! applies it, and the window re-renders whenever the controller changes.
//!
//! For screenshots, `open_for_test(controller, page, cx)` opens straight onto a page:
//! `general`, `pointers`, `appearance`, `drawing`, `sound`, `about`, `picker`, `menu` or `make` (the
//! hand maker). TH_DARK=1 forces the dark look, TH_MAKE_HAND=<idea> fills the maker's
//! description, TH_DRAFT=<png> loads a picture into it. main.rs can call it for
//! `TH_SHOW=settings:<page>`.

mod about;
mod appearance;
mod drawing;
mod general;
mod icons;
mod maker;
mod pointers;
mod recorder;
mod sound;
mod text_input;
mod thumbs;
mod ui;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, ClickEvent, Context, Entity, FocusHandle, FontWeight, Image, ImageFormat, InteractiveElement,
    IntoElement, MouseButton, MouseMoveEvent, ParentElement, Pixels, Render, ScrollHandle, StatefulInteractiveElement, Styled,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div, img, linear_color_stop,
    linear_gradient, point, prelude::FluentBuilder, px, relative, size,
};
use tinyhand_core::custom_hands::CustomHand;
use tinyhand_core::settings::{POINTER_SIZE_RANGE, Settings};

use crate::controller::Controller;
use ui::{Theme, text};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    General,
    Pointers,
    Appearance,
    Drawing,
    Sound,
    About,
}

impl Section {
    const ALL: [Section; 6] = [Section::General, Section::Pointers, Section::Appearance, Section::Drawing, Section::Sound, Section::About];

    fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Pointers => "Pointers",
            Section::Appearance => "Appearance",
            Section::Drawing => "Drawing",
            Section::Sound => "Sound",
            Section::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::General => icons::GEAR,
            Section::Pointers => icons::HAND,
            Section::Appearance => icons::SPARKLES,
            Section::Drawing => icons::PENCIL_TIP,
            Section::Sound => icons::SPEAKER_WAVE,
            Section::About => icons::INFO,
        }
    }

    /// The icon tile colour, as SettingsSection.tint.
    fn tint(self) -> (f32, f32, f32) {
        match self {
            Section::General => (0.55, 0.55, 0.55),
            Section::Pointers => (0.55, 0.42, 0.95),
            Section::Appearance => (0.95, 0.45, 0.6),
            Section::Drawing => (0.96, 0.58, 0.2),
            Section::Sound => (0.2, 0.55, 0.98),
            Section::About => (0.45, 0.45, 0.45),
        }
    }

    fn from_name(name: &str) -> Option<Section> {
        Section::ALL.into_iter().find(|s| s.title().eq_ignore_ascii_case(name))
    }
}

/// The app icon, shown in the sidebar and on the About page.
const APP_ICON: &[u8] = include_bytes!("../../../../assets/settings/app_icon.png");

pub(crate) fn mac() -> bool {
    cfg!(target_os = "macos")
}

pub struct SettingsWindow {
    controller: Entity<Controller>,
    section: Section,
    making_hand: bool,
    scroll: ScrollHandle,
    focus: FocusHandle,
    shortcut_error: Option<String>,
    pin_message: Option<String>,
    hand_message: Option<String>,
    open_menu: Option<&'static str>,
    menu_closed: Option<(&'static str, Instant)>,
    recorder: recorder::Recorder,
    drag: Option<pointers::Drag>,
    drag_removing: Rc<Cell<Option<bool>>>,
    picker_open: bool,
    swap_for: Option<String>,
    popover_closed: Option<Instant>,
    login: bool,
    slider_dragging: bool,
    slider_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    maker: maker::Maker,
    _subscriptions: Vec<Subscription>,
}

thread_local! {
    static OPEN: RefCell<Option<WindowHandle<SettingsWindow>>> = const { RefCell::new(None) };
}

/// Opens the settings window, or brings it forward if it is already open.
pub fn open(controller: Entity<Controller>, cx: &mut App) {
    open_at(controller, None, cx);
}

/// Opens the window on one page, for screenshots and tests. `page` is a section name
/// (`general` ... `about`), `make` for the hand maker, `picker` (Pointers with the add
/// picker open) or `menu` (General with a pop-up menu open).
#[allow(dead_code)]
pub fn open_for_test(controller: Entity<Controller>, page: &str, cx: &mut App) {
    open_at(controller, Some(page.to_string()), cx);
}

fn open_at(controller: Entity<Controller>, page: Option<String>, cx: &mut App) {
    text_input::bind_keys(cx);
    if let Some(handle) = OPEN.with(|o| *o.borrow()) {
        let shown = handle.update(cx, |this, window, cx| {
            this.on_show(page.as_deref(), window, cx);
            window.activate_window();
        });
        if shown.is_ok() {
            cx.activate(true);
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(780.), px(560.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("TinyHand Settings".into()),
            appears_transparent: mac(),
            traffic_light_position: mac().then(|| point(px(14.), px(14.))),
        }),
        window_min_size: Some(size(px(760.), px(540.))),
        focus: true,
        show: true,
        is_resizable: true,
        app_id: Some("tinyhand".into()),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            let mut this = SettingsWindow::new(controller, window, cx);
            this.on_show(page.as_deref(), window, cx);
            this
        })
    }) {
        Ok(handle) => {
            OPEN.with(|o| *o.borrow_mut() = Some(handle));
            cx.activate(true);
        }
        Err(e) => crate::controller::log(&format!("can't open the settings window: {e}")),
    }
}

impl SettingsWindow {
    fn new(controller: Entity<Controller>, window: &mut Window, cx: &mut Context<Self>) -> SettingsWindow {
        let subscriptions = vec![
            cx.observe(&controller, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
            cx.on_release(|this: &mut SettingsWindow, cx: &mut App| {
                if this.recorder.recording.is_some() {
                    this.controller.update(cx, |c, _| c.suspend_hotkeys(false));
                }
                OPEN.with(|o| *o.borrow_mut() = None);
            }),
        ];
        SettingsWindow {
            controller,
            section: Section::General,
            making_hand: false,
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            shortcut_error: None,
            pin_message: None,
            hand_message: None,
            open_menu: None,
            menu_closed: None,
            recorder: recorder::Recorder::new(cx),
            drag: None,
            drag_removing: Rc::new(Cell::new(None)),
            picker_open: false,
            swap_for: None,
            popover_closed: None,
            login: tinyhand_platform::login::launch_at_login(),
            slider_dragging: false,
            slider_bounds: Rc::new(Cell::new(None)),
            maker: maker::Maker::new(cx),
            _subscriptions: subscriptions,
        }
    }

    /// SettingsWindow.show: picks the section and clears the old messages.
    fn on_show(&mut self, page: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(page) = page {
            if page.eq_ignore_ascii_case("make") {
                self.section = Section::Pointers;
                self.start_making_hand(cx);
                if let Ok(idea) = std::env::var("TH_MAKE_HAND") {
                    self.maker.idea.update(cx, |i, cx| i.set_text(&idea, cx));
                }
                if let Some(path) = std::env::var_os("TH_DRAFT") {
                    self.load_draft(std::path::PathBuf::from(path), cx);
                }
            } else if page.eq_ignore_ascii_case("picker") {
                self.section = Section::Pointers;
                self.picker_open = true;
            } else if page.eq_ignore_ascii_case("menu") {
                self.section = Section::General;
                self.open_menu = Some("interact-key");
            } else if let Some(section) = Section::from_name(page) {
                self.section = section;
            }
        }
        self.shortcut_error = None;
        self.pin_message = None;
        self.login = tinyhand_platform::login::launch_at_login();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn settings<'a>(&self, cx: &'a App) -> &'a Settings {
        &self.controller.read(cx).settings
    }

    fn custom_hands(&self, cx: &App) -> Vec<CustomHand> {
        self.controller.read(cx).custom.hands().to_vec()
    }

    /// Changes settings through the controller, which saves and applies them.
    fn change(&self, cx: &mut Context<Self>, f: impl FnOnce(&mut Settings)) {
        self.controller.update(cx, |c, cx| c.update_settings(cx, f));
    }

    fn select(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.open_menu = None;
        self.picker_open = false;
        self.swap_for = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    /// Clicking a pop-up's own button while its menu is open closes the menu (the
    /// mouse-down outside the menu) and must not open it again straight away.
    fn menu_just_closed(&mut self, id: &'static str) -> bool {
        matches!(self.menu_closed.take(), Some((closed, at)) if closed == id && at.elapsed() < Duration::from_millis(400))
    }

    fn close_menu(&mut self, id: &'static str) {
        if self.open_menu == Some(id) {
            self.open_menu = None;
            self.menu_closed = Some((id, Instant::now()));
        }
    }

    fn popover_just_closed(&mut self) -> bool {
        matches!(self.popover_closed.take(), Some(at) if at.elapsed() < Duration::from_millis(400))
    }

    fn close_popovers(&mut self) {
        if self.picker_open || self.swap_for.is_some() {
            self.picker_open = false;
            self.swap_for = None;
            self.popover_closed = Some(Instant::now());
        }
    }

    fn slider_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(b) = self.slider_bounds.get() else { return };
        let knob = 20.;
        let f = ((f32::from(x - b.left()) - knob / 2.) / (f32::from(b.size.width) - knob)).clamp(0., 1.);
        let (lo, hi) = (*POINTER_SIZE_RANGE.start(), *POINTER_SIZE_RANGE.end());
        let value = lo + (hi - lo) * f;
        if (self.settings(cx).pointer_size - value).abs() > 0.0005 {
            self.change(cx, |s| s.pointer_size = value);
        }
    }

    fn render_sidebar(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = Section::ALL.into_iter().map(|s| {
            let selected = self.section == s;
            let (r, g, b) = s.tint();
            let tint = gpui::Rgba { r, g, b, a: 1.0 };
            let top = gpui::Rgba { r: r * 0.85 + 0.15, g: g * 0.85 + 0.15, b: b * 0.85 + 0.15, a: 1.0 };
            let hover = t.fg(if selected { 0.1 } else { 0.05 });
            div()
                .id(s.title())
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(8.))
                .py(px(6.))
                .rounded(px(8.))
                .when(selected, |d| d.bg(t.fg(0.1)))
                .hover(move |st| st.bg(hover))
                .cursor_pointer()
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(24.))
                        .rounded(px(6.))
                        .bg(linear_gradient(180., linear_color_stop(top, 0.), linear_color_stop(tint, 1.)))
                        .child(icons::icon(s.icon(), 13., gpui::white())),
                )
                .child(text(s.title(), 13., t.label()).when(selected, |d| d.font_weight(FontWeight::SEMIBOLD)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(s, cx)))
        });
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(210.))
            .h_full()
            .pt(px(if mac() { 52. } else { 16. }))
            .px(px(10.))
            .bg(t.sidebar_bg())
            .child(div().flex().flex_col().gap(px(2.)).children(rows))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .pb(px(16.))
                    .child(img(app_icon()).size(px(22.)))
                    .child(text("TinyHand", 12., t.label()).font_weight(FontWeight::SEMIBOLD)),
            )
    }

    fn render_page(&mut self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let making = self.section == Section::Pointers && self.making_hand;
        let title = if making { "Make your own hand" } else { self.section.title() };
        let content = match self.section {
            Section::General => general::render(self, t, cx),
            Section::Pointers if making => maker::render(self, t, cx),
            Section::Pointers => pointers::render(self, t, window, cx),
            Section::Appearance => appearance::render(self, t, cx),
            Section::Drawing => drawing::render(self, t, cx),
            Section::Sound => sound::render(self, t, window, cx),
            Section::About => about::render(self, t, cx),
        };
        let secondary = t.secondary();
        let hover = t.label();
        div()
            .id("page")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .bg(t.window_bg())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .px(px(28.))
                    .pt(px(if mac() { 48. } else { 24. }))
                    .pb(px(28.))
                    .when(making, |d| {
                        d.child(
                            div()
                                .id("back")
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .mb(px(-14.))
                                .text_color(secondary)
                                .hover(move |s| s.text_color(hover))
                                .cursor_pointer()
                                .child(icons::icon(icons::CHEVRON_LEFT, 11., secondary))
                                .child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).child("Pointers"))
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.making_hand = false;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(text(title, 22., t.label()).font_weight(FontWeight::BOLD).pt(px(6.)))
                    .child(content),
            )
    }
}

pub(crate) fn app_icon() -> std::sync::Arc<Image> {
    std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, APP_ICON.to_vec()))
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::of(window);
        if self.drag.is_some() && !cx.has_active_drag() {
            self.drag = None;
        }
        pointers::request_thumbnails(self, cx);
        div()
            .size_full()
            .flex()
            .track_focus(&self.focus)
            .font_family(".SystemUIFont")
            .line_height(relative(1.22))
            .text_color(t.label())
            .bg(t.window_bg())
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.slider_dragging {
                    if e.pressed_button == Some(MouseButton::Left) {
                        this.slider_to(e.position.x, cx);
                    } else {
                        this.slider_dragging = false;
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.slider_dragging = false;
                }),
            )
            .on_drop(cx.listener(|this, d: &pointers::SlotDrag, _, cx| pointers::drop(this, d, cx)))
            .child(self.render_sidebar(&t, cx))
            .child(self.render_page(&t, window, cx))
    }
}
