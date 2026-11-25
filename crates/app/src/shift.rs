//! Double-tapping ⇧ toggles click-through (`OverlayController.shiftChanged`).
//!
//! A tap is ⇧ held alone for under 0.3 s with no key or mouse press in between (the
//! system's input counter didn't move), so typing a capital letter is not a tap. Two taps
//! make a double if the second ⇧ goes down within 0.35 s of the first tap ending, again
//! with no input in between.

use std::time::{Duration, Instant};

pub const TAP_MAX: Duration = Duration::from_millis(300);
pub const DOUBLE_MAX: Duration = Duration::from_millis(350);

/// Which modifiers are held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub command: bool,
}

impl Mods {
    fn none(self) -> bool {
        self == Mods::default()
    }

    fn shift_alone(self) -> bool {
        self == Mods { shift: true, ..Mods::default() }
    }
}

#[derive(Default)]
pub struct ShiftTaps {
    down_at: Option<Instant>,
    down_inputs: u64,
    last_tap: Option<Instant>,
    last_tap_inputs: u64,
}

impl ShiftTaps {
    /// Forget any tap in progress (the pointer turned off).
    pub fn reset(&mut self) {
        *self = ShiftTaps::default();
    }

    /// Feed every modifier change. `inputs` is the input counter right now. Returns true
    /// when this change completes a double tap.
    pub fn changed(&mut self, held: Mods, now: Instant, inputs: u64) -> bool {
        if held.shift_alone() {
            self.down_at = Some(now);
            self.down_inputs = inputs;
            return false;
        }
        let Some(down) = self.down_at.take().filter(|_| held.none()) else {
            self.down_at = None;
            self.last_tap = None;
            return false;
        };
        if now.duration_since(down) >= TAP_MAX || inputs != self.down_inputs {
            self.last_tap = None;
            return false;
        }
        let double = self
            .last_tap
            .is_some_and(|last| down.saturating_duration_since(last) < DOUBLE_MAX && down >= last)
            && self.down_inputs == self.last_tap_inputs;
        if double {
            self.last_tap = None;
            return true;
        }
        self.last_tap = Some(now);
        self.last_tap_inputs = inputs;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIFT: Mods = Mods { shift: true, control: false, alt: false, command: false };
    const NONE: Mods = Mods { shift: false, control: false, alt: false, command: false };

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    #[test]
    fn double_tap_toggles() {
        let t0 = Instant::now();
        let mut s = ShiftTaps::default();
        assert!(!s.changed(SHIFT, t0, 5));
        assert!(!s.changed(NONE, ms(t0, 100), 5));
        assert!(!s.changed(SHIFT, ms(t0, 300), 5));
        assert!(s.changed(NONE, ms(t0, 380), 5));
        // A third tap starts over.
        assert!(!s.changed(SHIFT, ms(t0, 450), 5));
        assert!(!s.changed(NONE, ms(t0, 500), 5));
    }

    #[test]
    fn slow_or_typed_taps_dont_count() {
        let t0 = Instant::now();
        let mut s = ShiftTaps::default();
        // Held too long.
        s.changed(SHIFT, t0, 0);
        s.changed(NONE, ms(t0, 350), 0);
        s.changed(SHIFT, ms(t0, 400), 0);
        assert!(!s.changed(NONE, ms(t0, 450), 0));
        // Second down too late after the first tap.
        s.changed(SHIFT, ms(t0, 1000), 0);
        assert!(!s.changed(NONE, ms(t0, 1050), 0));
        // A key typed while ⇧ was down (capital letter).
        s.changed(SHIFT, ms(t0, 1100), 0);
        assert!(!s.changed(NONE, ms(t0, 1150), 1));
        // A key typed between the taps.
        let mut s = ShiftTaps::default();
        s.changed(SHIFT, t0, 0);
        s.changed(NONE, ms(t0, 50), 0);
        s.changed(SHIFT, ms(t0, 100), 1);
        assert!(!s.changed(NONE, ms(t0, 150), 1));
        // ⇧ with another modifier.
        let mut s = ShiftTaps::default();
        s.changed(SHIFT, t0, 0);
        s.changed(NONE, ms(t0, 50), 0);
        s.changed(SHIFT, ms(t0, 100), 0);
        s.changed(Mods { command: true, ..SHIFT }, ms(t0, 120), 0);
        assert!(!s.changed(NONE, ms(t0, 150), 0));
    }
}
