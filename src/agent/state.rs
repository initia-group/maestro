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
    Spawning { since: DateTime<Utc> },

    /// Agent is actively producing output.
    Running { since: DateTime<Utc> },

    /// Agent is waiting for user input (tool approval, question, etc.).
    WaitingForInput {
        since: DateTime<Utc>,
        prompt_type: PromptType,
    },

    /// Agent has not produced output for `idle_timeout_secs`.
    Idle { since: DateTime<Utc> },

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

    /// Returns a detailed human-readable label.
    ///
    /// For `WaitingForInput` states, returns the prompt type detail
    /// (e.g., "Tool: Edit", "Question") instead of the generic "waiting".
    /// For all other states, returns the same as `label()`.
    pub fn detail_label(&self) -> String {
        match self {
            AgentState::WaitingForInput { prompt_type, .. } => prompt_type.short_text(),
            other => other.label().to_string(),
        }
    }

    /// Whether the agent is in a terminal state (no further transitions
    /// possible without explicit restart).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentState::Completed { .. } | AgentState::Errored { .. }
        )
    }

    /// Whether the agent's process is still running.
    pub fn is_alive(&self) -> bool {
        !self.is_terminal()
    }

    /// Compare two states by variant, ignoring timestamps.
    ///
    /// Used by the debounce logic to check if consecutive detections agree
    /// on the state kind without being thrown off by differing `Utc::now()`
    /// timestamps. For `WaitingForInput`, also compares the `PromptType`.
    pub fn same_variant(&self, other: &Self) -> bool {
        match (self, other) {
            (AgentState::Spawning { .. }, AgentState::Spawning { .. }) => true,
            (AgentState::Running { .. }, AgentState::Running { .. }) => true,
            (AgentState::Idle { .. }, AgentState::Idle { .. }) => true,
            (
                AgentState::WaitingForInput { prompt_type: a, .. },
                AgentState::WaitingForInput { prompt_type: b, .. },
            ) => a.same_kind(b),
            (
                AgentState::Completed { exit_code: a, .. },
                AgentState::Completed { exit_code: b, .. },
            ) => a == b,
            (
                AgentState::Errored { error_hint: a, .. },
                AgentState::Errored { error_hint: b, .. },
            ) => a == b,
            _ => false,
        }
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
    ToolApproval { tool_name: String },

    /// Agent is showing an interactive AskUserQuestion prompt with numbered options.
    AskUserQuestion { question: String },

    /// Agent is asking a question to the user (text ending with ?).
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
            PromptType::AskUserQuestion { question } => {
                let truncated = if question.len() > 40 {
                    format!("{}…", &question[..39])
                } else {
                    question.clone()
                };
                format!("Ask: {truncated}")
            }
            PromptType::Question => "Question".to_string(),
            PromptType::InputPrompt => "Input".to_string(),
            PromptType::Unknown => "Waiting".to_string(),
        }
    }

    /// Whether this is an AskUserQuestion prompt (any question text).
    pub fn is_ask_user_question(&self) -> bool {
        matches!(self, PromptType::AskUserQuestion { .. })
    }

    /// Compare two prompt types by kind, ignoring inner data.
    ///
    /// For `ToolApproval`, compares tool names. For `AskUserQuestion`,
    /// compares only by variant (ignores question text to avoid flapping
    /// from minor text differences between detection ticks).
    pub fn same_kind(&self, other: &Self) -> bool {
        match (self, other) {
            (
                PromptType::ToolApproval { tool_name: a },
                PromptType::ToolApproval { tool_name: b },
            ) => a == b,
            (PromptType::AskUserQuestion { .. }, PromptType::AskUserQuestion { .. }) => true,
            (PromptType::Question, PromptType::Question) => true,
            (PromptType::InputPrompt, PromptType::InputPrompt) => true,
            (PromptType::Unknown, PromptType::Unknown) => true,
            _ => false,
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
        assert_eq!(
            AgentState::Spawning { since: Utc::now() }.color_key(),
            "spawning"
        );
        assert_eq!(
            AgentState::Running { since: Utc::now() }.color_key(),
            "running"
        );
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
        assert_eq!(
            AgentState::Spawning { since: Utc::now() }.label(),
            "spawning"
        );
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
        assert_eq!(
            AgentState::Running { since: Utc::now() }.to_string(),
            "running"
        );
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
    fn test_detail_label_waiting_states() {
        let state = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::ToolApproval {
                tool_name: "Edit".into(),
            },
        };
        assert_eq!(state.detail_label(), "Tool: Edit");

        let state = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::Question,
        };
        assert_eq!(state.detail_label(), "Question");

        let state = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::InputPrompt,
        };
        assert_eq!(state.detail_label(), "Input");

        let state = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::Unknown,
        };
        assert_eq!(state.detail_label(), "Waiting");
    }

    #[test]
    fn test_detail_label_non_waiting_states() {
        assert_eq!(
            AgentState::Spawning { since: Utc::now() }.detail_label(),
            "spawning"
        );
        assert_eq!(
            AgentState::Running { since: Utc::now() }.detail_label(),
            "running"
        );
        assert_eq!(
            AgentState::Idle { since: Utc::now() }.detail_label(),
            "idle"
        );
        assert_eq!(
            AgentState::Completed {
                at: Utc::now(),
                exit_code: Some(0)
            }
            .detail_label(),
            "done"
        );
        assert_eq!(
            AgentState::Errored {
                at: Utc::now(),
                error_hint: None
            }
            .detail_label(),
            "error"
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
    fn test_same_variant_ignores_timestamps() {
        let t1 = Utc::now();
        let t2 = t1 + chrono::Duration::seconds(5);
        assert!(AgentState::Running { since: t1 }.same_variant(&AgentState::Running { since: t2 }));
        assert!(
            AgentState::Spawning { since: t1 }.same_variant(&AgentState::Spawning { since: t2 })
        );
        assert!(AgentState::Idle { since: t1 }.same_variant(&AgentState::Idle { since: t2 }));
    }

    #[test]
    fn test_same_variant_different_variants() {
        let now = Utc::now();
        assert!(
            !AgentState::Running { since: now }.same_variant(&AgentState::Spawning { since: now })
        );
        assert!(!AgentState::Running { since: now }.same_variant(&AgentState::Idle { since: now }));
    }

    #[test]
    fn test_same_variant_waiting_compares_prompt_type() {
        let now = Utc::now();
        let w1 = AgentState::WaitingForInput {
            since: now,
            prompt_type: PromptType::Question,
        };
        let w2 = AgentState::WaitingForInput {
            since: now + chrono::Duration::seconds(1),
            prompt_type: PromptType::Question,
        };
        let w3 = AgentState::WaitingForInput {
            since: now,
            prompt_type: PromptType::InputPrompt,
        };
        assert!(w1.same_variant(&w2));
        assert!(!w1.same_variant(&w3));
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
