//! TinyHand's logic, free of UI and OS code so it can be tested on its own and shared by
//! every platform.

pub mod arm;
pub mod hand;
pub mod particles;
pub mod random;

pub use arm::{Arm, ArmPose};
pub use hand::Hand;
