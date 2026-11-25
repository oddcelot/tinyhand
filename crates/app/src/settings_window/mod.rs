//! The settings window. Phase 4 fills this in; see crates/docs/plan.md.

use gpui::{App, Entity};

use crate::controller::Controller;

/// Opens the settings window, or brings it forward if it is already open.
#[allow(dead_code)]
pub fn open(_controller: Entity<Controller>, _cx: &mut App) {
    crate::controller::log("settings not yet ported");
}
