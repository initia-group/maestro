//! Agent state machine definitions.
//!
//! Defines the `AgentState` enum representing all possible states an agent
//! can be in, and the `PromptType` enum for classifying waiting states.
//! See Feature 05 (Agent State Machine & Detection) for the full spec.

use chrono::{DateTime, Utc};

/// Represents the current state of an agent process.
///
/// State transitions are driven by the detection engine which scans
/// PTY screen content for patterns and checks process exit status.
///
/// Transition diagram:
/// ```text
///   Spawning -> Running <-> WaitingForInput
///                  |              |
///                  v              v
///                Idle  <----------+
///                  |
///                  v
///          Completed / Errored
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    /// Agent process is starting up. No output received yet.
    Spawning {
        since: DateTime<Utc>,
    },

    /// Agent is actively producing output.
    Running {
        since: DateTime<Utc>,
    },

    /// Agent is waiting for user input (tool approval, question, etc.).
    WaitingForInput {
        since: DateTime<Utc>,
        prompt_type: PromptType,
    },

    /// Agent has not produced output for `idle_timeout_secs`.
    Idle {
        since: DateTime<Utc>,
    },

    /// Agent process exited successfully (exit code 0).
    Completed {
        at: DateTime<Utc>,
        exit_code: Option<i32>,
    },

    /// Agent process exited with an error.
    Errored {
        at: DateTime<Utc>,
        error_hint: Option<String>,
    },
}

impl AgentState {
    /// Returns the status indicator symbol for display.
    pub fn symbol(&self) -> &'static str {
        match self {
            AgentState::Spawning { .. } => "○",
            AgentState::Running { .. } => "●",
            AgentState::WaitingForInput { .. } => "?",
            AgentState::Idle { .. } => "-",
            AgentState::Completed { .. } => "✓",
            AgentState::Errored { .. } => "!",
        }
    }

    /// Returns the color name for theming.
    pub fn color_key(&self) -> &'static str {
        match self {
            AgentState::Spawning { .. } => "spawning",
            AgentState::Running { .. } => "running",
            AgentState::WaitingForInput { .. } => "waiting",
            AgentState::Idle { .. } => "idle",
            AgentState::Completed { .. } => "completed",
            AgentState::Errored { .. } => "errored",
        }
    }

    /// Returns a human-readable short label.
    pub fn label(&self) -> &'static str {
        match self {
            AgentState::Spawning { .. } => "spawning",
            AgentState::Running { .. } => "running",
            AgentState::WaitingForInput { .. } => "waiting",
            AgentState::Idle { .. } => "idle",
            AgentState::Completed { .. } => "done",
            AgentState::Errored { .. } => "error",
        }
    }

    /// Whether the agent is in a terminal state (no further transitions
    /// possible without explicit restart).
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Completed { .. } | AgentState::Errored { .. })
    }

    /// Whether the agent's process is still running.
    pub fn is_alive(&self) -> bool {
        !self.is_terminal()
    }
}

impl Default for AgentState {
    fn default() -> Self {
        AgentState::Spawning { since: Utc::now() }
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// The type of input the agent is waiting for.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptType {
    /// Agent is asking for tool approval (e.g., "Allow Edit to src/auth.rs? [Y/n]").
    ToolApproval {
        tool_name: String,
    },

    /// Agent is asking a question to the user.
    Question,

    /// Agent is showing an input prompt ("> " at the bottom).
    InputPrompt,

    /// We detected a waiting state but couldn't determine the specific type.
    Unknown,
}

impl PromptType {
    /// Short display text for the status bar.
    pub fn short_text(&self) -> String {
        match self {
            PromptType::ToolApproval { tool_name } => format!("Tool: {tool_name}"),
            PromptType::Question => "Question".to_string(),
            PromptType::InputPrompt => "Input".to_string(),
            PromptType::Unknown => "Waiting".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_symbols() {
        assert_eq!(AgentState::Spawning { since: Utc::now() }.symbol(), "○");
        assert_eq!(AgentState::Running { since: Utc::now() }.symbol(), "●");
        assert_eq!(
            AgentState::WaitingForInput {
                prompt_type: PromptType::Question,
                since: Utc::now(),
            }
            .symbol(),
            "?"
        );
        assert_eq!(AgentState::Idle { since: Utc::now() }.symbol(), "-");
        assert_eq!(
            AgentState::Completed {
                at: Utc::now(),
                exit_code: Some(0),
            }
            .symbol(),
            "✓"
        );
        assert_eq!(
            AgentState::Errored {
                at: Utc::now(),
                error_hint: None,
            }
            .symbol(),
            "!"
        );
    }

    #[test]
    fn test_color_keys() {
        assert_eq!(AgentState::Spawning { since: Utc::now() }.color_key(), "spawning");
        assert_eq!(AgentState::Running { since: Utc::now() }.color_key(), "running");
        assert_eq!(
            AgentState::WaitingForInput {
                since: Utc::now(),
                prompt_type: PromptType::InputPrompt,
            }
            .color_key(),
            "waiting"
        );
        assert_eq!(AgentState::Idle { since: Utc::now() }.color_key(), "idle");
        assert_eq!(
            AgentState::Completed {
                at: Utc::now(),
                exit_code: Some(0),
            }
            .color_key(),
            "completed"
        );
        assert_eq!(
            AgentState::Errored {
                at: Utc::now(),
                error_hint: None,
            }
            .color_key(),
            "errored"
        );
    }

    #[test]
    fn test_labels() {
        assert_eq!(AgentState::Spawning { since: Utc::now() }.label(), "spawning");
        assert_eq!(AgentState::Running { since: Utc::now() }.label(), "running");
        assert_eq!(
            AgentState::WaitingForInput {
                since: Utc::now(),
                prompt_type: PromptType::Unknown,
            }
            .label(),
            "waiting"
        );
        assert_eq!(AgentState::Idle { since: Utc::now() }.label(), "idle");
        assert_eq!(
            AgentState::Completed {
                at: Utc::now(),
                exit_code: Some(0),
            }
            .label(),
            "done"
        );
        assert_eq!(
            AgentState::Errored {
                at: Utc::now(),
                error_hint: None,
            }
            .label(),
            "error"
        );
    }

    #[test]
    fn test_terminal_states() {
        assert!(!AgentState::Spawning { since: Utc::now() }.is_terminal());
        assert!(!AgentState::Running { since: Utc::now() }.is_terminal());
        assert!(!AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::Question,
        }
        .is_terminal());
        assert!(!AgentState::Idle { since: Utc::now() }.is_terminal());
        assert!(AgentState::Completed {
            at: Utc::now(),
            exit_code: Some(0),
        }
        .is_terminal());
        assert!(AgentState::Errored {
            at: Utc::now(),
            error_hint: Some("boom".into()),
        }
        .is_terminal());
    }

    #[test]
    fn test_is_alive() {
        assert!(AgentState::Running { since: Utc::now() }.is_alive());
        assert!(AgentState::Idle { since: Utc::now() }.is_alive());
        assert!(!AgentState::Completed {
            at: Utc::now(),
            exit_code: Some(0),
        }
        .is_alive());
        assert!(!AgentState::Errored {
            at: Utc::now(),
            error_hint: None,
        }
        .is_alive());
    }

    #[test]
    fn test_default_state_is_spawning() {
        let state = AgentState::default();
        assert!(matches!(state, AgentState::Spawning { .. }));
    }

    #[test]
    fn test_display() {
        assert_eq!(AgentState::Running { since: Utc::now() }.to_string(), "running");
        assert_eq!(
            AgentState::Completed {
                at: Utc::now(),
                exit_code: Some(0),
            }
            .to_string(),
            "done"
        );
    }

    #[test]
    fn test_prompt_type_short_text() {
        assert_eq!(
            PromptType::ToolApproval {
                tool_name: "Edit".into()
            }
            .short_text(),
            "Tool: Edit"
        );
        assert_eq!(PromptType::Question.short_text(), "Question");
        assert_eq!(PromptType::InputPrompt.short_text(), "Input");
        assert_eq!(PromptType::Unknown.short_text(), "Waiting");
    }

    #[test]
    fn test_prompt_type_equality() {
        assert_eq!(PromptType::Question, PromptType::Question);
        assert_ne!(PromptType::Question, PromptType::InputPrompt);
        assert_eq!(
            PromptType::ToolApproval {
                tool_name: "Edit".into()
            },
            PromptType::ToolApproval {
                tool_name: "Edit".into()
            }
        );
        assert_ne!(
            PromptType::ToolApproval {
                tool_name: "Edit".into()
            },
            PromptType::ToolApproval {
                tool_name: "Write".into()
            }
        );
    }
}
