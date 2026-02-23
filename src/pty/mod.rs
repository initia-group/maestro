//! PTY management subsystem.
//!
//! Handles spawning processes in pseudo-terminals and controlling I/O.

pub mod controller;
pub mod spawner;

pub use controller::PtyController;
pub use spawner::{default_pty_size, spawn_in_pty, SpawnConfig, SpawnResult};
