//! Event system — central event bus and type definitions.
//!
//! Provides the async event multiplexer that combines PTY output,
//! keyboard input, timers, and state change events.

pub mod bus;
pub mod types;

pub use bus::EventBus;
pub use types::{AppEvent, InputEvent};
