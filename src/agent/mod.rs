//! Agent management subsystem.
//!
//! Handles agent lifecycle, state machine, detection, and process management.

pub mod detector;
pub mod handle;
pub mod manager;
pub mod restart;
pub mod scrollback;
pub mod state;
pub mod stream_json;

pub use detector::{
    detect_state, DetectionDebounce, DetectionPatterns, DetectionSignals, ProcessExit,
};
pub use handle::{AgentHandle, RestartParams};
pub use manager::{AgentManager, StateCounts};
pub use restart::{RestartPolicy, RestartTracker};
pub use scrollback::{ScrollbackBuffer, SearchMatch, SearchState};
pub use state::{AgentState, PromptType};
pub use stream_json::{parse_stream_event, AgentMode, StreamEvent, StreamJsonState};

use uuid::Uuid;

/// A unique identifier for an agent instance.
///
/// Wraps a `Uuid` for type safety — prevents accidentally mixing up
/// agent IDs with other UUID-based identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentId(Uuid);

impl AgentId {
    /// Create a new random agent ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the inner `Uuid`.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Short 8-char prefix for compact display in the UI.
        let s = self.0.to_string();
        write!(f, "{}", &s[..8])
    }
}

impl From<Uuid> for AgentId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_uniqueness() {
        let a = AgentId::new();
        let b = AgentId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn agent_id_display_is_short() {
        let id = AgentId::new();
        let display = format!("{id}");
        assert_eq!(display.len(), 8);
    }

    #[test]
    fn agent_id_copy_semantics() {
        let a = AgentId::new();
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn agent_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let id = AgentId::from(uuid);
        assert_eq!(*id.as_uuid(), uuid);
    }

    #[test]
    fn agent_id_default() {
        let a = AgentId::default();
        let b = AgentId::default();
        assert_ne!(a, b);
    }
}
