//! Where the stick and hand go for a given pointer position. Same geometry as
//! `ArmView.aim` in the Swift app: the stick comes up from below the screen, held by a grip
//! that lags behind the pointer, and the fingertip lands exactly on the pointer.

use crate::hand::Hand;

pub const HAND_LEN_FRAC: f32 = 0.13;
pub const MIN_ROD_FRAC: f32 = 0.05;
pub const GRIP_LEAN_FRAC: f32 = 0.30;
pub const GRIP_DEPTH_FRAC: f32 = 0.12;
pub const GRIP_FOLLOW: f32 = 0.08;
/// How fast a press springs back, per 1/120 s.
pub const PRESS_RELEASE: f32 = 0.18;

/// One frame of the arm, in screen points with y pointing down.
#[derive(Clone, Debug)]
pub struct ArmPose {
    /// Corners of the stick.
    pub rod: [(f32, f32); 4],
    /// Corners of the highlight strip along the stick.
    pub sheen: [(f32, f32); 4],
    /// Direction across the stick for its shading, in CSS degrees.
    pub rod_shade_angle: f32,
    /// The hand's rotation, anticlockwise on screen, in radians.
    pub rotation: f32,
    /// Centre of the rotated hand image.
    pub hand_center: (f32, f32),
    /// Size of the unrotated hand: width and height.
    pub hand_size: (f32, f32),
}

/// The arm's state between frames.
pub struct Arm {
    grip_x: Option<f32>,
    /// 1 right after a press, easing back to 0.
    pub press: f32,
}

impl Default for Arm {
    fn default() -> Self {
        Arm { grip_x: None, press: 0.0 }
    }
}

impl Arm {
    /// Height in device pixels the hand is drawn at for a screen this tall.
    pub fn hand_pixels(screen_height: f32, scale: f32) -> u32 {
        (HAND_LEN_FRAC * screen_height * scale).round() as u32
    }

    pub fn press_now(&mut self) {
        self.press = 1.0;
    }

    pub fn ease_press(&mut self, dt: f32) {
        self.press -= self.press * (1.0 - (1.0 - PRESS_RELEASE).powf(dt * 120.0));
    }

    /// Works out this frame's pose. `cursor` is in screen points, y down; `dt` in seconds.
    pub fn aim(&mut self, hand: &Hand, cursor: (f32, f32), screen: (f32, f32), dt: f32) -> ArmPose {
        let (w, h) = screen;
        // Same maths as the Swift view, which has y pointing up.
        let cursor = (cursor.0, h - cursor.1);
        let target_x = (cursor.0 + GRIP_LEAN_FRAC * w).clamp(w * 0.15, w * 1.05);
        let follow = 1.0 - (1.0 - GRIP_FOLLOW).powf(dt * 120.0);
        let gx = self.grip_x.map_or(target_x, |g| g + (target_x - g) * follow);
        self.grip_x = Some(gx);
        let grip = (gx, -GRIP_DEPTH_FRAC * h);

        let (dx, dy) = (cursor.0 - grip.0, cursor.1 - grip.1);
        let reach = dx.hypot(dy);
        let aim_angle = dy.atan2(dx);
        let hand_pts = HAND_LEN_FRAC * h * (1.0 - 0.06 * self.press);
        let s = hand_pts / (hand.wrist - hand.hand_top);
        let width_pts = s * hand.aspect;
        let hand_tip_pts = (hand.wrist - hand.tip) * s;
        let tip_off_x = (hand.tip_x - hand.rod_x) * width_pts;
        let along = (reach * reach - tip_off_x * tip_off_x).max(0.0).sqrt();
        let rod_len = (along - hand_tip_pts).max(MIN_ROD_FRAC * h);
        let tip_angle = (rod_len + hand_tip_pts).atan2(tip_off_x);
        let rotation = aim_angle - tip_angle;

        let (sin, cos) = rotation.sin_cos();
        let to_screen = |lx: f32, ly: f32| (grip.0 + lx * cos - ly * sin, h - (grip.1 + lx * sin + ly * cos));
        let quad = |x0: f32, x1: f32, y0: f32, y1: f32| [to_screen(x0, y0), to_screen(x1, y0), to_screen(x1, y1), to_screen(x0, y1)];

        let rod_w = hand.rod_width * width_pts;
        let rod_top = rod_len + 0.04 * hand_pts;
        ArmPose {
            rod: quad(-rod_w / 2.0, rod_w / 2.0, 0.0, rod_top),
            sheen: quad(-rod_w * 0.16, rod_w * 0.04, 0.0, rod_top),
            rod_shade_angle: (90.0 - rotation.to_degrees()).rem_euclid(360.0),
            rotation,
            hand_center: to_screen((0.5 - hand.rod_x) * width_pts, rod_len + hand_pts / 2.0),
            hand_size: (width_pts, hand_pts),
        }
    }

    /// Where the fingertip ends up for a pose: should be the cursor. Used by tests.
    pub fn fingertip(hand: &Hand, pose: &ArmPose) -> (f32, f32) {
        let (width_pts, hand_pts) = pose.hand_size;
        let s = hand_pts / (hand.wrist - hand.hand_top);
        // Offset of the tip from the hand image centre in the arm's frame (y up)...
        let dx = (hand.tip_x - 0.5) * width_pts;
        let dy = (hand.wrist - hand.tip) * s - hand_pts / 2.0;
        // ...turned with the arm and flipped to screen y.
        let (sin, cos) = pose.rotation.sin_cos();
        (pose.hand_center.0 + dx * cos - dy * sin, pose.hand_center.1 - (dx * sin + dy * cos))
    }
}
