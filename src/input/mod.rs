//! Input handling subsystem.
//!
//! Modal input processing with Normal, Insert, Command, and Search modes.

pub mod action;
pub mod handler;
pub mod mode;

pub use action::Action;
pub use handler::{key_event_to_bytes, InputHandler};
pub use mode::InputMode;
