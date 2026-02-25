//! Event type definitions.
//!
//! Defines `AppEvent` (PTY output, input, state changes, timers)
//! and `InputEvent` (keyboard) types that flow through the event bus.

use crate::agent::state::AgentState;
use crate::agent::AgentId;
use crossterm::event::{KeyEvent, MouseEvent};

/// All events the application can receive.
///
/// Events flow from the outside world (keyboard, PTY output, timers)
/// into the main application loop via the [`super::bus::EventBus`].
#[derive(Debug)]
pub enum AppEvent {
    /// A keyboard event from the terminal.
    Input(InputEvent),

    /// Raw bytes received from an agent's PTY.
    PtyOutput { agent_id: AgentId, data: Vec<u8> },

    /// An agent's PTY has closed (process exited or PTY error).
    PtyEof { agent_id: AgentId },

    /// An agent's state has changed (detected by the state detector).
    AgentStateChanged {
        agent_id: AgentId,
        old_state: AgentState,
        new_state: AgentState,
    },

    /// Periodic tick for state detection.
    /// Fires every `state_check_interval_ms` (default 250ms).
    StateTick,

    /// Render tick — signals that a frame may be drawn.
    /// The app only renders if something is dirty.
    RenderRequest,

    /// Terminal window was resized.
    Resize { cols: u16, rows: u16 },

    /// Request to quit the application.
    QuitRequested,
}

/// Input events from crossterm, abstracted slightly.
#[derive(Debug)]
pub enum InputEvent {
    /// A keyboard key press event.
    Key(KeyEvent),
    /// A mouse event (click, scroll, etc.).
    Mouse(MouseEvent),
}
