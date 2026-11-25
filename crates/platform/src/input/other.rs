//! Linux and anything else: no tap. X11 can grab the pointer but not pass single presses on
//! to other windows, and Wayland can't do either, so the app takes the mouse with its
//! capture window instead.

use super::{InputError, InputEvent, Shared};

pub(crate) struct Tap {
    shared: Shared,
}

impl Tap {
    pub fn start(_on_event: Box<dyn Fn(InputEvent) + Send>) -> Result<Tap, InputError> {
        Err(InputError::Unsupported)
    }

    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    pub fn enabled_changed(&self, _on: bool) {}
}

pub fn is_trusted() -> bool {
    true
}

pub fn request_trust() {}

pub fn can_tap() -> bool {
    false
}

pub fn input_counter() -> u64 {
    0
}

pub fn left_button_down() -> bool {
    false
}

pub fn screen_locked() -> bool {
    false
}
