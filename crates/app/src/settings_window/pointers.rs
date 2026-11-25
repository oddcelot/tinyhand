//! Pointers: the toolbar's pinned pointers (drag to reorder, drag out or ✕ to remove, +
//! to add from a picker), the toolbar switches, and the hands the user made.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, AppContext, ClickEvent, Context, DragMoveEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div, hsla, img, prelude::FluentBuilder, px,
};
use tinyhand_core::custom_hands::{CustomHand, CustomHands};
use tinyhand_core::settings::Settings;
use tinyhand_core::styles::{self, BUILT_IN, DEFAULT_PINNED, LASER, MAX_PINNED};

use super::ui::{Theme, card, floating, heading, popover, row, switch, text, text_button, tip};
use super::{SettingsWindow, icons, mac, thumbs};

/// What is dragged out of a toolbar slot.
pub struct SlotDrag {
    id: String,
    removing: Rc<Cell<Option<bool>>>,
}

/// A drag in progress: the order before it, and the order shown now.
pub struct Drag {
    id: String,
    base: Vec<String>,
    order: Vec<String>,
    outside: bool,
}

/// The pointer that follows the mouse while dragging (FloatingPointer).
struct DragView {
    id: String,
    removing: Rc<Cell<Option<bool>>>,
}

impl Render for DragView {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::of(window);
        let removing = self.removing.get() == Some(true);
        div()
            .relative()
            .size(px(60.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(10.))
            .bg(t.window_bg().opacity(0.95))
            .shadow(vec![gpui::BoxShadow::new(px(0.), px(5.), hsla(0., 0., 0., 0.25)).blur_radius(px(12.))])
            .child(pointer_icon(&self.id, 42.))
            .when(removing, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(-7.))
                        .right(px(-7.))
                        .size(px(18.))
                        .rounded_full()
                        .bg(t.red())
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icons::icon(icons::MINUS, 11., gpui::white())),
                )
            })
    }
}

/// A style's thumbnail (or the drawn laser dot); empty until the thumbnail is ready.
pub fn pointer_icon(id: &str, size: f32) -> AnyElement {
    if id == LASER {
        return img(icons::colour_svg(icons::laser_svg())).size(px(size)).into_any_element();
    }
    match thumbs::get(id) {
        Some(image) => img(image).size(px(size)).into_any_element(),
        None => div().size(px(size)).into_any_element(),
    }
}

/// Makes the thumbnails of every style that hasn't got one yet.
pub fn request_thumbnails(this: &SettingsWindow, cx: &mut Context<SettingsWindow>) {
    if this.section != super::Section::Pointers {
        return;
    }
    let custom = &this.controller.read(cx).custom;
    let wanted: Vec<(String, Option<std::path::PathBuf>)> = BUILT_IN
        .iter()
        .map(|b| b.0.to_string())
        .chain(custom.hands().iter().map(|h| h.id.clone()))
        .filter(|id| thumbs::wanted(id))
        .map(|id| {
            let path = crate::assets::hand_image(&id, custom);
            (id, path)
        })
        .collect();
    if !wanted.is_empty() {
        thumbs::request(wanted, cx);
    }
}

fn title(id: &str, custom: &[CustomHand]) -> String {
    styles::find(id, custom).map(|s| s.title).unwrap_or_else(|| id.to_string())
}

fn pinned(s: &Settings, custom: &[CustomHand]) -> Vec<String> {
    s.pinned_existing(custom)
}

impl SettingsWindow {
    fn set_pinned(&mut self, pinned: Vec<String>, cx: &mut Context<Self>) {
        self.change(cx, |s| s.pinned_styles = pinned);
    }

    /// SettingsModel.pin: adds a pointer, or removes it if it is already there.
    fn pin(&mut self, id: &str, cx: &mut Context<Self>) {
        let custom = self.custom_hands(cx);
        let mut p = pinned(self.settings(cx), &custom);
        if p.iter().any(|x| x == id) {
            return self.unpin(id, cx);
        }
        if p.len() >= MAX_PINNED {
            self.pin_message = Some(format!("The toolbar is full. Remove one above to add {}.", title(id, &custom)));
            cx.notify();
            return;
        }
        p.push(id.to_string());
        self.pin_message = None;
        self.set_pinned(p, cx);
    }

    fn unpin(&mut self, id: &str, cx: &mut Context<Self>) {
        let custom = self.custom_hands(cx);
        let mut p = pinned(self.settings(cx), &custom);
        let Some(i) = p.iter().position(|x| x == id) else { return };
        if p.len() == 1 {
            self.pin_message = Some("Keep at least one pointer in the toolbar.".into());
            cx.notify();
            return;
        }
        p.remove(i);
        self.pin_message = None;
        self.set_pinned(p, cx);
    }

    fn swap(&mut self, old: &str, new: &str, cx: &mut Context<Self>) {
        let custom = self.custom_hands(cx);
        let mut p = pinned(self.settings(cx), &custom);
        if p.iter().any(|x| x == new) {
            return;
        }
        let Some(i) = p.iter().position(|x| x == old) else { return };
        p[i] = new.to_string();
        self.pin_message = None;
        self.set_pinned(p, cx);
    }

    fn reset_toolbar(&mut self, cx: &mut Context<Self>) {
        self.pin_message = None;
        self.set_pinned(DEFAULT_PINNED.iter().map(|s| s.to_string()).collect(), cx);
    }

    /// SettingsModel.removeHand: out of the toolbar, off as the current pointer, deleted.
    fn remove_hand(&mut self, id: &str, cx: &mut Context<Self>) {
        let custom = self.custom_hands(cx);
        let mut p: Vec<String> = pinned(self.settings(cx), &custom).into_iter().filter(|x| x != id).collect();
        if p.is_empty() {
            p = DEFAULT_PINNED.iter().map(|s| s.to_string()).collect();
        }
        let current = self.settings(cx).pointer_style == id;
        self.change(cx, |s| {
            s.pinned_styles = p;
            if current {
                s.pointer_style = "lavender".into();
            }
        });
        let mut hands = CustomHands::load_default();
        if let Err(e) = hands.delete(id) {
            crate::controller::log(&format!("can't delete {id}: {e}"));
        }
        thumbs::forget(id);
        self.controller.update(cx, |c, cx| c.reload_custom_hands(cx));
        self.hand_message = None;
        self.swap_for = None;
        cx.notify();
    }

    fn drag_moved(&mut self, e: &DragMoveEvent<SlotDrag>, cx: &mut Context<Self>) {
        let id = e.drag(cx).id.clone();
        if self.drag.as_ref().is_none_or(|d| d.id != id) {
            let base = pinned(self.settings(cx), &self.custom_hands(cx));
            self.drag = Some(Drag { id: id.clone(), order: base.clone(), base, outside: false });
            self.pin_message = None;
            self.close_popovers();
        }
        let Some(drag) = self.drag.as_mut() else { return };
        let (b, p) = (e.bounds, e.event.position);
        let f = |v: Pixels| f32::from(v);
        let inside = f(p.x) >= f(b.left()) - 16.
            && f(p.x) <= f(b.right()) + 16.
            && f(p.y) >= f(b.top()) - 28.
            && f(p.y) <= f(b.bottom()) + 28.;
        let mut order: Vec<String> = drag.base.iter().filter(|x| **x != drag.id).cloned().collect();
        let removing = if inside {
            let step = (f(b.size.width) - 20. + 6.) / MAX_PINNED as f32;
            let slot = ((f(p.x) - f(b.left()) - 10.) / step).floor().max(0.) as usize;
            order.insert(slot.min(order.len()), id);
            None
        } else if drag.base.len() > 1 {
            Some(true)
        } else {
            order = drag.base.clone();
            Some(false)
        };
        drag.outside = !inside;
        if drag.order != order || e.drag(cx).removing.get() != removing {
            drag.order = order;
            e.drag(cx).removing.set(removing);
            cx.notify();
        }
    }
}

/// The drop of a dragged slot, anywhere in the window.
pub fn drop(this: &mut SettingsWindow, d: &SlotDrag, cx: &mut Context<SettingsWindow>) {
    d.removing.set(None);
    let Some(drag) = this.drag.take() else { return };
    if drag.outside && drag.base.len() <= 1 {
        this.pin_message = Some("Keep at least one pointer in the toolbar.".into());
    } else if drag.order != drag.base {
        this.set_pinned(drag.order, cx);
    }
    cx.notify();
}

fn slot_frame(t: &Theme) -> gpui::Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .h(px(58.))
        .rounded(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .bg(t.window_bg().opacity(0.95))
}

fn toolbar_slot(t: &Theme, i: usize, id: &str, custom: &[CustomHand], removing: &Rc<Cell<Option<bool>>>, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let group: SharedString = format!("slot-{i}").into();
    let close_id = id.to_string();
    let drag = SlotDrag { id: id.to_string(), removing: removing.clone() };
    slot_frame(t)
        .id(ElementId::Name(format!("slot-{id}").into()))
        .group(group.clone())
        .relative()
        .shadow(vec![gpui::BoxShadow::new(px(0.), px(1.), hsla(0., 0., 0., if t.dark { 0.25 } else { 0.08 })).blur_radius(px(2.))])
        .cursor_grab()
        .child(pointer_icon(id, 40.))
        .child(
            div()
                .id(ElementId::Name(format!("unpin-{id}").into()))
                .absolute()
                .top(px(-5.))
                .right(px(-5.))
                .size(px(16.))
                .rounded_full()
                .bg(hsla(0., 0., 0., 0.55))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .invisible()
                .group_hover(group, |s| s.visible())
                .child(icons::icon(icons::XMARK, 7., gpui::white()))
                .tooltip(tip("Remove from the toolbar"))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.unpin(&close_id, cx);
                })),
        )
        .tooltip(tip(title(id, custom)))
        .on_drag(drag, |d: &SlotDrag, _, _, cx| cx.new(|_| DragView { id: d.id.clone(), removing: d.removing.clone() }))
        .into_any_element()
}

fn picker(this: &SettingsWindow, t: &Theme, pinned_now: &[String], custom: &[CustomHand], cx: &mut Context<SettingsWindow>) -> impl IntoElement {
    let cell = |id: String, cx: &mut Context<SettingsWindow>| {
        let added = pinned_now.contains(&id);
        let name = title(&id, custom);
        let hover = t.fg(0.08);
        let pick = id.clone();
        div()
            .id(ElementId::Name(format!("pick-{id}").into()))
            .w(px(72.))
            .h(px(58.))
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(62.))
                    .h(px(58.))
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!added, |d| d.hover(move |s| s.bg(hover)).cursor_pointer())
                    .when(added, |d| d.opacity(0.35))
                    .child(pointer_icon(&id, 42.)),
            )
            .tooltip(tip(if added { format!("{name} is already in the toolbar") } else { name }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if added {
                    return;
                }
                this.pin(&pick, cx);
                this.picker_open = false;
                cx.notify();
            }))
            .into_any_element()
    };
    let built_in: Vec<AnyElement> =
        BUILT_IN.iter().map(|b| b.0.to_string()).chain(std::iter::once(LASER.to_string())).map(|id| cell(id, cx)).collect();
    let yours: Vec<AnyElement> = custom.iter().map(|h| cell(h.id.clone(), cx)).collect();
    let _ = this;
    let make_hover = t.fg(0.05);
    let content = div()
        .w(px(400.))
        .flex()
        .flex_col()
        .child(
            div()
                .id("picker-scroll")
                .h(px(340.))
                .overflow_y_scroll()
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(div().flex().flex_wrap().gap(px(4.)).children(built_in))
                .when(!yours.is_empty(), |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(text("Your hands", 11., t.secondary()).font_weight(FontWeight::SEMIBOLD).pl(px(4.)))
                            .child(div().flex().flex_wrap().gap(px(4.)).children(yours)),
                    )
                }),
        )
        .child(div().h(px(1.)).bg(t.fg(0.1)))
        .child(
            div()
                .id("picker-make")
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(14.))
                .py(px(10.))
                .rounded_b(px(10.))
                .hover(move |s| s.bg(make_hover))
                .cursor_pointer()
                .child(icons::icon(icons::WAND, 12., t.secondary()))
                .child(text("Make your own hand…", 12., t.secondary()))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.picker_open = false;
                    this.start_making_hand(cx);
                })),
        );
    floating(64., popover(t, "picker", content).on_mouse_down_out(cx.listener(|this, _, _, cx| {
        this.close_popovers();
        cx.notify();
    })))
}

fn add_slot(this: &SettingsWindow, t: &Theme, pinned_now: &[String], custom: &[CustomHand], cx: &mut Context<SettingsWindow>) -> AnyElement {
    let open = this.picker_open;
    let hover = t.fg(0.09);
    let colour = if open { t.label() } else { t.secondary() };
    div()
        .flex_1()
        .min_w(px(0.))
        .relative()
        .child(
            div()
                .id("add-slot")
                .h(px(58.))
                .rounded(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .bg(t.fg(if open { 0.09 } else { 0.05 }))
                .hover(move |s| s.bg(hover))
                .border_1()
                .border_dashed()
                .border_color(t.fg(0.16))
                .cursor_pointer()
                .child(icons::icon(icons::PLUS, 16., colour))
                .tooltip(tip("Add a pointer"))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if this.popover_just_closed() {
                        return;
                    }
                    this.picker_open = !this.picker_open;
                    cx.notify();
                })),
        )
        .when(open, |d| d.child(picker(this, t, pinned_now, custom, cx)))
        .into_any_element()
}

fn your_hands(this: &SettingsWindow, t: &Theme, s: &Settings, custom: &[CustomHand], cx: &mut Context<SettingsWindow>) -> AnyElement {
    let p = pinned(s, custom);
    let full = p.len() >= MAX_PINNED;
    let chips: Vec<AnyElement> = custom
        .iter()
        .enumerate()
        .map(|(i, hand)| {
            let is_pinned = p.contains(&hand.id);
            let group: SharedString = format!("chip-{i}").into();
            let (id, del, name) = (hand.id.clone(), hand.id.clone(), hand.name.clone());
            let hover = t.fg(0.09);
            let x_hover = t.fg(0.12);
            let swapping = this.swap_for.as_deref() == Some(hand.id.as_str());
            div()
                .relative()
                .child(
                    div()
                        .id(ElementId::Name(format!("chip-{}", hand.id).into()))
                        .group(group.clone())
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(6.))
                        .py(px(5.))
                        .rounded_full()
                        .bg(t.fg(0.05))
                        .when(!is_pinned, |d| d.hover(move |s| s.bg(hover)).cursor_pointer())
                        .child(pointer_icon(&hand.id, 24.))
                        .child(text(hand.name.clone(), 12., t.label()).whitespace_nowrap())
                        .child(
                            div()
                                .id(ElementId::Name(format!("delete-{}", hand.id).into()))
                                .size(px(16.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .invisible()
                                .group_hover(group, |s| s.visible())
                                .hover(move |s| s.bg(x_hover))
                                .cursor_pointer()
                                .child(icons::icon(icons::XMARK, 7., t.secondary()))
                                .tooltip(tip(format!("Delete {}", hand.name)))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.remove_hand(&del, cx);
                                })),
                        )
                        .tooltip(tip(if is_pinned { format!("{name} is in your toolbar") } else { format!("Click to add {name} to the toolbar") }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if is_pinned {
                                return;
                            }
                            if full {
                                if !this.popover_just_closed() {
                                    this.swap_for = Some(id.clone());
                                }
                                cx.notify();
                            } else {
                                this.pin(&id, cx);
                            }
                        })),
                )
                .when(swapping, |d| d.child(swap_popover(t, hand, &p, custom, cx)))
                .into_any_element()
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(4.))
                .child(heading(t, "Your hands"))
                .child(text_button(t, "make-own", "Make your own…", cx, |this, _, cx| this.start_making_hand(cx))),
        )
        .map(|d| {
            if chips.is_empty() {
                d.child(text("Turn any idea into a hand with ChatGPT or another image tool.", 11., t.secondary()).px(px(4.)))
            } else {
                d.child(div().flex().flex_wrap().gap(px(6.)).children(chips))
            }
        })
        .when_some(this.hand_message.clone(), |d, m| d.child(text(m, 11., t.secondary()).px(px(4.))))
        .into_any_element()
}

fn swap_popover(t: &Theme, hand: &CustomHand, pinned_now: &[String], custom: &[CustomHand], cx: &mut Context<SettingsWindow>) -> impl IntoElement {
    let cells: Vec<AnyElement> = pinned_now
        .iter()
        .map(|old| {
            let hover = t.fg(0.1);
            let (old_id, new_id) = (old.clone(), hand.id.clone());
            div()
                .id(ElementId::Name(format!("swap-{old}").into()))
                .size(px(44.))
                .rounded(px(9.))
                .flex()
                .items_center()
                .justify_center()
                .bg(t.fg(0.04))
                .hover(move |s| s.bg(hover))
                .cursor_pointer()
                .child(pointer_icon(old, 32.))
                .tooltip(tip(format!("Replace {}", title(old, custom))))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.swap(&old_id, &new_id, cx);
                    this.swap_for = None;
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();
    let content = div()
        .p(px(12.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(text(format!("The toolbar is full. Swap {} in for:", hand.name), 12., t.secondary()))
        .child(div().flex().gap(px(4.)).children(cells));
    floating(40., popover(t, "swap", content).on_mouse_down_out(cx.listener(|this, _, _, cx| {
        this.close_popovers();
        cx.notify();
    })))
}

pub fn render(this: &mut SettingsWindow, t: &Theme, _window: &mut Window, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let s = this.settings(cx).clone();
    let custom = this.custom_hands(cx);
    let saved = pinned(&s, &custom);
    let order = this.drag.as_ref().map(|d| d.order.clone()).unwrap_or_else(|| saved.clone());
    let dragging = this.drag.as_ref().map(|d| d.id.clone());
    let full = saved.len() >= MAX_PINNED;

    let mut slots: Vec<AnyElement> = Vec::new();
    for (i, id) in order.iter().enumerate() {
        if dragging.as_deref() == Some(id.as_str()) {
            // The lifted pointer follows the mouse; its place shows where it will land.
            slots.push(
                slot_frame(t)
                    .bg(t.fg(0.035))
                    .border_1()
                    .border_color(t.accent().opacity(0.5))
                    .into_any_element(),
            );
        } else {
            slots.push(toolbar_slot(t, i, id, &custom, &this.drag_removing, cx));
        }
    }
    if !full {
        slots.push(add_slot(this, t, &saved, &custom, cx));
    }
    for _ in 0..MAX_PINNED.saturating_sub(order.len() + 1) {
        slots.push(slot_frame(t).bg(t.fg(0.035)).into_any_element());
    }

    let toolbar_detail = if s.show_toolbar {
        "Pops up when the pointer turns on and near the bottom of the screen."
    } else if mac() {
        "Stays hidden. Press Esc to exit, or use the menu bar."
    } else {
        "Stays hidden. Press Esc to exit, or use the tray icon."
    };

    div()
        .flex()
        .flex_col()
        .gap(px(30.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(4.))
                        .child(heading(t, "In your toolbar"))
                        .child(text_button(t, "reset-toolbar", "Reset", cx, |this, _, cx| this.reset_toolbar(cx))),
                )
                .child(
                    div()
                        .id("toolbar-row")
                        .flex()
                        .gap(px(6.))
                        .p(px(10.))
                        .rounded(px(14.))
                        .bg(t.fg(0.045))
                        .when(!s.show_toolbar, |d| d.opacity(0.45))
                        .children(slots)
                        .on_drag_move(cx.listener(|this, e: &DragMoveEvent<SlotDrag>, _, cx| this.drag_moved(e, cx))),
                )
                .child(
                    text(
                        if full {
                            "The toolbar is full. Remove a pointer to add another. Drag to reorder."
                        } else {
                            "Press + to add a pointer. Drag to reorder, or out to remove."
                        },
                        11.,
                        t.secondary(),
                    )
                    .px(px(4.)),
                )
                .when_some(this.pin_message.clone(), |d, m| d.child(text(m, 12., t.red()).px(px(4.)))),
        )
        .child(card(
            t,
            Some("Toolbar"),
            vec![
                row(
                    t,
                    "Show the toolbar",
                    Some(toolbar_detail.into()),
                    switch(t, "show-toolbar", s.show_toolbar, true, cx, |this, on, _, cx| this.change(cx, |s| s.show_toolbar = on)),
                ),
                row(
                    t,
                    "Ink button",
                    Some("Change the colour while you present.".into()),
                    switch(t, "ink-button", s.show_ink_button, true, cx, |this, on, _, cx| this.change(cx, |s| s.show_ink_button = on)),
                ),
            ],
        ))
        .child(your_hands(this, t, &s, &custom, cx))
        .into_any_element()
}
