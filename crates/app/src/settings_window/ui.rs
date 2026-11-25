//! Colours and the small widgets the pages share: cards, rows, switches, buttons, pop-up
//! menus, a slider and tooltips. Zed's `ui` crate isn't a dependency, so they live here.

use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext, BoxShadow, ClickEvent, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, WindowAppearance, anchored,
    canvas, deferred, div, hsla, prelude::FluentBuilder, px, relative, rgb,
};

use super::SettingsWindow;
use super::icons;

pub type Cx<'a> = Context<'a, SettingsWindow>;
pub type Handler = Rc<dyn Fn(&mut SettingsWindow, &mut Window, &mut Cx<'_>)>;

/// The colours for light or dark, picked like the system's (NSColor) ones.
#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
}

pub fn rgb_hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

impl Theme {
    pub fn of(window: &Window) -> Theme {
        if let Some(v) = std::env::var_os("TH_DARK") {
            return Theme { dark: v != "0" };
        }
        Theme {
            dark: matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
        }
    }

    /// `Color.primary.opacity(a)`: black in light mode, white in dark mode.
    pub fn fg(&self, a: f32) -> Hsla {
        if self.dark { hsla(0., 0., 1., a) } else { hsla(0., 0., 0., a) }
    }

    pub fn label(&self) -> Hsla {
        self.fg(0.85)
    }

    pub fn secondary(&self) -> Hsla {
        if self.dark { hsla(0., 0., 1., 0.55) } else { hsla(0., 0., 0., 0.5) }
    }

    pub fn window_bg(&self) -> Hsla {
        rgb_hsla(if self.dark { 0x323232 } else { 0xECECEC })
    }

    pub fn sidebar_bg(&self) -> Hsla {
        rgb_hsla(if self.dark { 0x49484E } else { 0xE3E2E8 })
    }

    /// The menu and popover background.
    pub fn popover_bg(&self) -> Hsla {
        rgb_hsla(if self.dark { 0x2C2C2E } else { 0xF6F6F6 })
    }

    pub fn accent(&self) -> Hsla {
        rgb_hsla(if self.dark { 0x0A84FF } else { 0x007AFF })
    }

    pub fn red(&self) -> Hsla {
        rgb_hsla(if self.dark { 0xFF453A } else { 0xFF3B30 })
    }

    pub fn orange(&self) -> Hsla {
        rgb_hsla(if self.dark { 0xFF9F0A } else { 0xFF9500 })
    }
}

pub fn text(s: impl Into<SharedString>, size: f32, colour: Hsla) -> Div {
    div().text_size(px(size)).text_color(colour).child(s.into())
}

/// The grey section heading above a card or a block.
pub fn heading(t: &Theme, title: impl Into<SharedString>) -> Div {
    text(title, 12., t.secondary()).font_weight(FontWeight::SEMIBOLD)
}

/// A rounded group of rows with thin lines between them and an optional heading.
pub fn card(t: &Theme, title: Option<&str>, rows: Vec<AnyElement>) -> Div {
    let n = rows.len();
    let mut body = div().flex().flex_col().rounded(px(12.)).bg(t.fg(0.045));
    for (i, row) in rows.into_iter().enumerate() {
        body = body.child(row);
        if i + 1 < n {
            body = body.child(div().ml(px(14.)).h(px(1.)).bg(t.fg(0.07)));
        }
    }
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .when_some(title, |d, title| d.child(heading(t, title.to_string()).pl(px(4.))))
        .child(body)
}

/// A settings row: title and detail on the left, a control on the right.
pub fn row(t: &Theme, title: impl Into<SharedString>, detail: Option<String>, trailing: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(10.))
        .min_h(px(44.))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(text(title, 13., t.label()))
                .when_some(detail, |d, detail| d.child(text(detail, 11., t.secondary()))),
        )
        .child(div().flex_none().child(trailing))
        .into_any_element()
}

/// The macOS-style switch.
pub fn switch(
    t: &Theme,
    id: impl Into<ElementId>,
    on: bool,
    enabled: bool,
    cx: &mut Cx<'_>,
    toggle: impl Fn(&mut SettingsWindow, bool, &mut Window, &mut Cx<'_>) + 'static,
) -> impl IntoElement {
    let track = if on { t.accent() } else { t.fg(if t.dark { 0.12 } else { 0.1 }) };
    let knob = div()
        .size(px(20.))
        .rounded_full()
        .bg(rgb_hsla(if t.dark && !on { 0xCACACA } else { 0xFFFFFF }))
        .shadow(vec![
            BoxShadow::new(px(0.), px(0.5), hsla(0., 0., 0., 0.25)).blur_radius(px(1.5)),
            BoxShadow::new(px(0.), px(0.), hsla(0., 0., 0., 0.08)).spread_radius(px(0.5)),
        ]);
    div()
        .id(id.into())
        .flex()
        .items_center()
        .w(px(38.))
        .h(px(22.))
        .p(px(1.))
        .rounded_full()
        .bg(track)
        .when(on, |d| d.justify_end())
        .when(!enabled, |d| d.opacity(0.5))
        .child(knob)
        .when(enabled, |d| {
            d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, window, cx| toggle(this, !on, window, cx)))
        })
}

/// Small grey text link ("Reset", "Make your own…").
pub fn text_button(t: &Theme, id: impl Into<ElementId>, title: impl Into<SharedString>, cx: &mut Cx<'_>, action: impl Fn(&mut SettingsWindow, &mut Window, &mut Cx<'_>) + 'static) -> impl IntoElement {
    let hover = t.label();
    div()
        .id(id.into())
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.secondary())
        .hover(move |s| s.text_color(hover))
        .cursor_pointer()
        .child(title.into())
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| action(this, window, cx)))
}

/// Capsule button ("Open Settings").
pub fn pill_button(t: &Theme, id: impl Into<ElementId>, title: impl Into<SharedString>, cx: &mut Cx<'_>, action: impl Fn(&mut SettingsWindow, &mut Window, &mut Cx<'_>) + 'static) -> impl IntoElement {
    let hover = t.fg(0.14);
    div()
        .id(id.into())
        .px(px(12.))
        .py(px(5.))
        .rounded_full()
        .bg(t.fg(0.09))
        .hover(move |s| s.bg(hover))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.label())
        .cursor_pointer()
        .child(title.into())
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| action(this, window, cx)))
}

/// The filled (accent) or grey capsule button of the hand maker.
pub fn primary_button(
    t: &Theme,
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    symbol: Option<&'static str>,
    primary: bool,
    cx: &mut Cx<'_>,
    action: impl Fn(&mut SettingsWindow, &mut Window, &mut Cx<'_>) + 'static,
) -> impl IntoElement {
    let fg = if primary { gpui::white() } else { t.label() };
    let (bg, hover) = if primary { (t.accent(), t.accent().opacity(0.88)) } else { (t.fg(0.09), t.fg(0.14)) };
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(14.))
        .h(px(30.))
        .rounded_full()
        .bg(bg)
        .hover(move |s| s.bg(hover))
        .cursor_pointer()
        .when_some(symbol, |d, s| d.child(icons::icon(s, 12., fg)))
        .child(text(title, 12., fg).font_weight(FontWeight::SEMIBOLD))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| action(this, window, cx)))
}

pub struct MenuChoice {
    pub label: String,
    pub selected: bool,
    pub action: Handler,
}

impl MenuChoice {
    pub fn new(label: impl Into<String>, selected: bool, action: impl Fn(&mut SettingsWindow, &mut Window, &mut Cx<'_>) + 'static) -> MenuChoice {
        MenuChoice { label: label.into(), selected, action: Rc::new(action) }
    }
}

/// A pop-up button (SwiftUI's `.pickerStyle(.menu)`): shows the chosen value, opens a menu.
pub fn popup(t: &Theme, this: &SettingsWindow, id: &'static str, choices: Vec<MenuChoice>, cx: &mut Cx<'_>) -> impl IntoElement {
    let open = this.open_menu == Some(id);
    let current = choices.iter().find(|c| c.selected).map(|c| c.label.clone()).unwrap_or_default();
    let (bg, border) = if t.dark { (t.fg(0.16), t.fg(0.06)) } else { (gpui::white(), t.fg(0.1)) };
    let button = div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(22.))
        .pl(px(9.))
        .pr(px(4.))
        .rounded(px(6.))
        .bg(bg)
        .border(px(0.5))
        .border_color(border)
        .shadow(vec![BoxShadow::new(px(0.), px(0.5), hsla(0., 0., 0., if t.dark { 0.3 } else { 0.12 })).blur_radius(px(1.))])
        .cursor_pointer()
        .child(text(current, 13., t.label()))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(16.))
                .rounded(px(4.))
                .child(icons::icon(icons::CHEVRONS_UP_DOWN, 11., t.label())),
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            if this.menu_just_closed(id) {
                return;
            }
            this.open_menu = if this.open_menu == Some(id) { None } else { Some(id) };
            cx.notify();
        }));

    let menu = open.then(|| {
        let items: Vec<AnyElement> = choices
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let action = c.action.clone();
                let accent = t.accent();
                div()
                    .id(ElementId::Name(format!("{id}-{i}").into()))
                    .group("menu-item")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .h(px(24.))
                    .pl(px(6.))
                    .pr(px(14.))
                    .rounded(px(5.))
                    .text_size(px(13.))
                    .text_color(t.label())
                    .hover(move |s| s.bg(accent).text_color(gpui::white()))
                    .cursor_pointer()
                    .child(div().w(px(14.)).flex().justify_center().when(c.selected, |d| d.child("✓")))
                    .child(c.label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_menu = None;
                        action(this, window, cx);
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        floating(
            26.,
                div()
                    .id(ElementId::Name(format!("{id}-menu").into()))
                    .flex()
                    .flex_col()
                    .min_w(px(160.))
                    .p(px(5.))
                    .rounded(px(8.))
                    .bg(t.popover_bg())
                    .border(px(0.5))
                    .border_color(t.fg(if t.dark { 0.2 } else { 0.12 }))
                    .shadow(vec![BoxShadow::new(px(0.), px(6.), hsla(0., 0., 0., 0.22)).blur_radius(px(18.))])
                    .children(items)
                    .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                        this.close_menu(id);
                        cx.notify();
                    })),
        )
    });
    div().relative().child(button).children(menu)
}

/// A horizontal slider; `value` and the result are in 0...1.
pub fn slider(t: &Theme, this: &SettingsWindow, width: f32, value: f32, cx: &mut Cx<'_>) -> impl IntoElement {
    let bounds = this.slider_bounds.clone();
    let knob = 20.;
    let travel = width - knob;
    let x = travel * value.clamp(0., 1.);
    div()
        .id("slider")
        .relative()
        .w(px(width))
        .h(px(22.))
        .cursor_pointer()
        .child(
            canvas(
                move |b, _, _| bounds.set(Some(b)),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .child(div().absolute().left(px(knob / 2.)).right(px(knob / 2.)).top(px(9.)).h(px(4.)).rounded_full().bg(t.fg(0.12)))
        .child(div().absolute().left(px(knob / 2.)).w(px(x)).top(px(9.)).h(px(4.)).rounded_full().bg(t.accent()))
        .child(
            div()
                .absolute()
                .left(px(x))
                .top(px(1.))
                .size(px(knob))
                .rounded_full()
                .bg(rgb_hsla(if t.dark { 0xDADADA } else { 0xFFFFFF }))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(0.5), hsla(0., 0., 0., 0.3)).blur_radius(px(2.)),
                    BoxShadow::new(px(0.), px(0.), hsla(0., 0., 0., 0.1)).spread_radius(px(0.5)),
                ]),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                this.slider_dragging = true;
                this.slider_to(e.position.x, cx);
            }),
        )
}

/// Tooltip view (SwiftUI's `.help`).
pub struct Tip {
    text: SharedString,
}

impl Render for Tip {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::of(window);
        div().pl(px(10.)).pt(px(16.)).child(
            div()
                .px(px(7.))
                .py(px(3.))
                .rounded(px(5.))
                .bg(t.popover_bg())
                .border(px(0.5))
                .border_color(t.fg(0.15))
                .shadow(vec![BoxShadow::new(px(0.), px(2.), hsla(0., 0., 0., 0.18)).blur_radius(px(6.))])
                .text_size(px(11.))
                .text_color(t.label())
                .line_height(relative(1.3))
                .font_family(".SystemUIFont")
                .child(self.text.clone()),
        )
    }
}

pub fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip { text: text.clone() }).into()
}

/// A dropped floating panel (popover) placed just below its trigger.
pub fn popover(t: &Theme, id: impl Into<ElementId>, content: impl IntoElement) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .rounded(px(10.))
        .bg(t.popover_bg())
        .border(px(0.5))
        .border_color(t.fg(if t.dark { 0.2 } else { 0.12 }))
        .shadow(vec![BoxShadow::new(px(0.), px(8.), hsla(0., 0., 0., 0.25)).blur_radius(px(24.))])
        .child(content)
}

/// Shows `content` in a layer above the page, `top` points below the top-left corner of
/// the element it is a child of (which must be `relative`), moved in to fit the window.
pub fn floating(top: f32, content: impl IntoElement) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .top(px(top))
        .child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(content)).priority(1))
}
