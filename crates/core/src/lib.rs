//! TinyHand's logic, free of UI and OS code so it can be tested on its own and shared by
//! every platform.

pub mod arm;
pub mod automation;
pub mod custom_hands;
pub mod hand;
pub mod hand_maker;
pub mod ink;
pub mod settings;
pub mod styles;

pub use arm::{Arm, ArmPose};
pub use hand::Hand;
