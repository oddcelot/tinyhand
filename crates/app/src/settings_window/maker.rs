//! "Make your own hand" (MakeHandPage and the hand parts of SettingsModel): describe it,
//! make the picture with ChatGPT or another tool, then drop the PNG in, name it, add it.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, AppContext, ClickEvent, ClipboardItem, Context, Entity, ExternalPaths, FontWeight, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, RenderImage, StatefulInteractiveElement, Styled, div, img, prelude::FluentBuilder, px, rgb,
};
use image::RgbaImage;
use tinyhand_core::custom_hands::CustomHands;
use tinyhand_core::hand_maker;
use tinyhand_core::styles::{LASER, MAX_PINNED};

use super::text_input::TextInput;
use super::ui::{Theme, primary_button, text, text_button};
use super::{SettingsWindow, icons, thumbs};

struct Draft {
    image: RgbaImage,
    ink: u32,
    preview: Arc<RenderImage>,
}

pub struct Maker {
    pub idea: Entity<TextInput>,
    name: Entity<TextInput>,
    draft: Option<Draft>,
    error: Option<String>,
    copied: bool,
    copied_count: u64,
}

impl Maker {
    pub fn new(cx: &mut Context<SettingsWindow>) -> Maker {
        Maker {
            idea: cx.new(|cx| TextInput::new("e.g. knitted wool in mustard yellow", None, cx)),
            name: cx.new(|cx| TextInput::new("My hand", Some(tinyhand_core::custom_hands::MAX_NAME), cx)),
            draft: None,
            error: None,
            copied: false,
            copied_count: 0,
        }
    }
}

impl SettingsWindow {
    pub fn start_making_hand(&mut self, cx: &mut Context<Self>) {
        self.maker.draft = None;
        self.maker.error = None;
        self.maker.copied = false;
        self.hand_message = None;
        self.making_hand = true;
        self.close_popovers();
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    fn idea(&self, cx: &Context<Self>) -> String {
        self.maker.idea.read(cx).content.clone()
    }

    fn copy_prompt(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(hand_maker::prompt_for(&self.idea(cx))));
        self.maker.copied = true;
        self.maker.copied_count += 1;
        let count = self.maker.copied_count;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs_f32(1.6)).await;
            let _ = this.update(cx, |this, cx| {
                if this.maker.copied_count == count {
                    this.maker.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn choose_hand_file(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.load_draft(path, cx));
            }
        })
        .detach();
    }

    pub fn load_draft(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let made = std::fs::read(&path).map_err(|_| "TinyHand couldn't read that file.".to_string()).and_then(|bytes| {
            hand_maker::make(&bytes).map_err(|e| e.to_string())
        });
        match made {
            Ok((image, ink)) => {
                let preview = thumbs::render_crop(&image, 112, 90, 800, 1000, 288, 360);
                self.maker.draft = Some(Draft { image, ink, preview });
                self.maker.error = None;
                let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                let name = hand_maker::suggested_name(&file).unwrap_or_else(|| self.controller.read(cx).custom.default_name());
                self.maker.name.update(cx, |n, cx| n.set_text(&name, cx));
            }
            Err(message) => {
                self.maker.draft = None;
                self.maker.error = Some(message);
            }
        }
        cx.notify();
    }

    /// SettingsModel.saveDraft: saves the hand, puts it in the toolbar if there is room
    /// (before the laser dot), and makes it the pointer.
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.maker.draft.take() else { return };
        let name = self.maker.name.read(cx).content.clone();
        let mut hands = CustomHands::load_default();
        let hand = match hands.add(&draft.image, &name, draft.ink) {
            Ok(hand) => hand,
            Err(e) => {
                crate::controller::log(&format!("can't save the hand: {e}"));
                self.maker.draft = Some(draft);
                self.maker.error = Some("TinyHand couldn't save that hand.".into());
                cx.notify();
                return;
            }
        };
        self.controller.update(cx, |c, cx| c.reload_custom_hands(cx));
        let custom = self.custom_hands(cx);
        let mut pinned = self.settings(cx).pinned_existing(&custom);
        if pinned.len() < MAX_PINNED {
            let at = pinned.len() - usize::from(pinned.last().is_some_and(|l| l == LASER));
            pinned.insert(at, hand.id.clone());
            self.hand_message = Some(format!("{} is in your toolbar.", hand.name));
        } else {
            self.hand_message = Some(format!("{} is ready. Click it to add it to the toolbar.", hand.name));
        }
        let id = hand.id.clone();
        self.change(cx, |s| {
            s.pinned_styles = pinned;
            s.pointer_style = id;
        });
        self.maker.idea.update(cx, |i, cx| i.set_text("", cx));
        self.maker.error = None;
        self.making_hand = false;
        cx.notify();
    }
}

fn step(t: &Theme, number: u32, title: &str, detail: Option<&str>, content: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(px(12.))
        .child(
            div()
                .flex_none()
                .size(px(20.))
                .rounded_full()
                .bg(t.fg(0.75))
                .flex()
                .items_center()
                .justify_center()
                .child(text(number.to_string(), 11., t.window_bg()).font_weight(FontWeight::BOLD)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(text(title.to_string(), 13., t.label()).font_weight(FontWeight::SEMIBOLD))
                        .when_some(detail, |d, detail| d.child(text(detail.to_string(), 12., t.secondary()))),
                )
                .child(content),
        )
        .into_any_element()
}

fn drop_zone(this: &SettingsWindow, t: &Theme, cx: &mut Context<SettingsWindow>) -> impl IntoElement {
    let error = this.maker.error.clone();
    let (over_bg, accent) = (t.fg(0.07), t.accent());
    let (symbol, colour) = if error.is_some() { (icons::WARNING, t.orange()) } else { (icons::TRAY_DOWN, t.secondary()) };
    div()
        .id("drop-zone")
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .px(px(24.))
        .w_full()
        .h(px(130.))
        .rounded(px(12.))
        .bg(t.fg(0.03))
        .border(px(1.2))
        .border_dashed()
        .border_color(t.fg(0.14))
        .cursor_pointer()
        .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(over_bg).border_color(accent))
        .child(icons::icon(symbol, 22., colour))
        .child(
            text(error.clone().unwrap_or_else(|| "Drop the PNG here, or click to choose it".into()), 12., if error.is_some() { t.label() } else { t.secondary() })
                .text_center(),
        )
        .when(error.is_some(), |d| d.child(text("Try again with another image", 11., t.secondary())))
        .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
            if let Some(path) = paths.paths().first() {
                this.load_draft(path.clone(), cx);
            }
        }))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.choose_hand_file(cx)))
}

fn preview(this: &SettingsWindow, t: &Theme, draft: &Draft, cx: &mut Context<SettingsWindow>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(22.))
        .child(
            div()
                .flex_none()
                .w(px(170.))
                .py(px(10.))
                .rounded(px(12.))
                .bg(rgb(0xE8CFB5))
                .flex()
                .justify_center()
                .child(img(draft.preview.clone()).w(px(144.)).h(px(180.))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(text("Name", 11., t.secondary()).font_weight(FontWeight::MEDIUM))
                        .child(div().w(px(220.)).child(this.maker.name.clone())),
                )
                .child(
                    text("It should show the back of the hand, like the ones in TinyHand. If you see the palm, try another image.", 11., t.secondary())
                        .max_w(px(300.)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(primary_button(t, "add-hand", "Add hand", None, true, cx, |this, _, cx| this.save_draft(cx)))
                        .child(text_button(t, "another-image", "Use another image", cx, |this, _, cx| this.choose_hand_file(cx))),
                )
                .when_some(this.maker.error.clone(), |d, e| d.child(text(e, 11., t.red()))),
        )
}

pub fn render(this: &mut SettingsWindow, t: &Theme, cx: &mut Context<SettingsWindow>) -> AnyElement {
    let copied = this.maker.copied;
    let add: AnyElement = match this.maker.draft.as_ref() {
        Some(draft) => preview(this, t, draft, cx).into_any_element(),
        None => drop_zone(this, t, cx).into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .gap(px(26.))
        .child(step(t, 1, "Describe it", Some("What should your hand look like?"), div().max_w(px(420.)).child(this.maker.idea.clone())))
        .child(step(
            t,
            2,
            "Make the image",
            Some("Open it in ChatGPT, or copy the prompt for another image tool. Download the result as a PNG with a transparent background."),
            div()
                .flex()
                .gap(px(8.))
                .child(primary_button(t, "open-chatgpt", "Open in ChatGPT", Some(icons::ARROW_UP_RIGHT), true, cx, |this, _, cx| {
                    let url = hand_maker::chatgpt_url(&this.idea(cx));
                    cx.open_url(&url);
                }))
                .child(primary_button(
                    t,
                    "copy-prompt",
                    if copied { "Copied" } else { "Copy prompt" },
                    Some(if copied { icons::CHECKMARK } else { icons::DOC_ON_DOC }),
                    false,
                    cx,
                    |this, _, cx| this.copy_prompt(cx),
                )),
        ))
        .child(step(t, 3, "Add it", None, add))
        .into_any_element()
}
