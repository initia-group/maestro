//! Desktop notification manager.
//!
//! Sends OS-level desktop notifications when agents need attention,
//! complete tasks, or encounter errors. Uses the `notify-rust` crate
//! for cross-platform support (macOS, Linux, Windows).
//!
//! See Feature 17 (Desktop Notifications) for the full spec.

use crate::agent::state::{AgentState, PromptType};
use crate::agent::AgentId;
use crate::config::settings::NotificationConfig;
use notify_rust::Notification;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Manages desktop notification delivery.
///
/// Tracks per-agent cooldowns to prevent notification spam and respects
/// the user's configuration for which events should trigger notifications.
pub struct NotificationManager {
    /// Whether notifications are enabled.
    enabled: bool,
    /// Minimum time between notifications for the same agent (prevents spam).
    cooldown: Duration,
    /// Last notification time per agent.
    last_notified: HashMap<AgentId, Instant>,
    /// Whether to notify on input prompts (configurable, default false).
    notify_on_input_prompt: bool,
}

impl NotificationManager {
    /// Create a new `NotificationManager` from configuration.
    pub fn new(config: &NotificationConfig) -> Self {
        Self {
            enabled: config.enabled,
            cooldown: Duration::from_secs(config.cooldown_secs),
            last_notified: HashMap::new(),
            notify_on_input_prompt: config.notify_on_input_prompt,
        }
    }

    /// Called when an agent's state changes. Sends a notification if appropriate.
    ///
    /// Notifications are sent for:
    /// - `WaitingForInput(ToolApproval)` -- agent needs tool approval
    /// - `WaitingForInput(Question)` -- agent has a question
    /// - `WaitingForInput(InputPrompt)` -- only if `notify_on_input_prompt` is true
    /// - `WaitingForInput(Unknown)` -- agent needs attention
    /// - `Completed` -- agent finished successfully
    /// - `Errored` -- agent encountered an error
    ///
    /// Notifications are NOT sent for:
    /// - `Idle` -- too frequent, would be noisy
    /// - `Running` -- user just provided input, no notification needed
    /// - `Spawning` -- agent is just starting up
    pub fn on_state_change(
        &mut self,
        agent_id: AgentId,
        agent_name: &str,
        project_name: &str,
        old_state: &AgentState,
        new_state: &AgentState,
    ) {
        if !self.enabled {
            return;
        }

        // Check cooldown
        if let Some(last) = self.last_notified.get(&agent_id) {
            if last.elapsed() < self.cooldown {
                return;
            }
        }

        let notification = match new_state {
            AgentState::WaitingForInput { prompt_type, .. } => match prompt_type {
                PromptType::ToolApproval { tool_name } => Some((
                    "Agent needs approval",
                    format!("{}: Allow {} ?", agent_name, tool_name),
                )),
                PromptType::AskUserQuestion { ref question } => Some((
                    "Agent is asking a question",
                    format!("{}: {}", agent_name, question),
                )),
                PromptType::Question => Some((
                    "Agent has a question",
                    format!("{} is waiting for your input", agent_name),
                )),
                PromptType::InputPrompt => {
                    if self.notify_on_input_prompt {
                        Some((
                            "Agent waiting for input",
                            format!("{} is ready for a new prompt", agent_name),
                        ))
                    } else {
                        None
                    }
                }
                PromptType::Unknown => Some((
                    "Agent needs attention",
                    format!("{} is waiting for something", agent_name),
                )),
            },
            AgentState::Completed { .. } => {
                let uptime = match old_state {
                    AgentState::Running { since }
                    | AgentState::WaitingForInput { since, .. }
                    | AgentState::Idle { since } => {
                        let elapsed = chrono::Utc::now() - *since;
                        format!(" ({}m)", elapsed.num_minutes())
                    }
                    _ => String::new(),
                };
                Some((
                    "Agent completed",
                    format!("{} finished successfully{}", agent_name, uptime),
                ))
            }
            AgentState::Errored { error_hint, .. } => {
                let hint = error_hint.as_deref().unwrap_or("Unknown error");
                Some(("Agent error", format!("{}: {}", agent_name, hint)))
            }
            _ => None,
        };

        if let Some((title, body)) = notification {
            self.send(title, &body, agent_name, project_name);
            self.last_notified.insert(agent_id, Instant::now());
        }
    }

    /// Send a desktop notification asynchronously (fire and forget).
    ///
    /// Notification delivery never blocks the event loop. Failures are
    /// logged at the warn level and silently ignored.
    fn send(&self, title: &str, body: &str, _agent_name: &str, _project_name: &str) {
        let full_title = format!("Maestro \u{2014} {}", title);

        // Fire and forget -- notification delivery should never block the event loop
        let title = full_title;
        let body = body.to_string();
        tokio::spawn(async move {
            if let Err(e) = Notification::new()
                .summary(&title)
                .body(&body)
                .appname("Maestro")
                .timeout(notify_rust::Timeout::Milliseconds(5000))
                .show()
            {
                tracing::warn!("Failed to send notification: {}", e);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::state::{AgentState, PromptType};
    use chrono::Utc;

    fn default_config() -> NotificationConfig {
        NotificationConfig::default()
    }

    fn make_agent_id() -> AgentId {
        AgentId::new()
    }

    #[test]
    fn test_disabled_sends_nothing() {
        let config = NotificationConfig {
            enabled: false,
            ..default_config()
        };
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::Errored {
            at: Utc::now(),
            error_hint: Some("boom".into()),
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert!(manager.last_notified.is_empty());
    }

    #[test]
    fn test_idle_does_not_notify() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::Idle {
            since: Utc::now(),
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert!(manager.last_notified.is_empty());
    }

    #[test]
    fn test_running_does_not_notify() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::Question,
        };
        let new = AgentState::Running {
            since: Utc::now(),
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert!(manager.last_notified.is_empty());
    }

    #[test]
    fn test_spawning_does_not_notify() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Spawning {
            since: Utc::now(),
        };
        let new = AgentState::Spawning {
            since: Utc::now(),
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert!(manager.last_notified.is_empty());
    }

    #[tokio::test]
    async fn test_tool_approval_notifies() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::ToolApproval {
                tool_name: "Edit".into(),
            },
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
        assert!(manager.last_notified.contains_key(&id));
    }

    #[tokio::test]
    async fn test_question_notifies() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::Question,
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
    }

    #[tokio::test]
    async fn test_completed_notifies() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::Completed {
            at: Utc::now(),
            exit_code: Some(0),
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
    }

    #[tokio::test]
    async fn test_errored_notifies() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::Errored {
            at: Utc::now(),
            error_hint: Some("API connection failed".into()),
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
    }

    #[tokio::test]
    async fn test_unknown_prompt_notifies() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::Unknown,
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
    }

    #[test]
    fn test_input_prompt_default_no_notify() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::InputPrompt,
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert!(manager.last_notified.is_empty());
    }

    #[tokio::test]
    async fn test_input_prompt_enabled_notifies() {
        let config = NotificationConfig {
            notify_on_input_prompt: true,
            ..default_config()
        };
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::WaitingForInput {
            since: Utc::now(),
            prompt_type: PromptType::InputPrompt,
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
    }

    #[tokio::test]
    async fn test_cooldown_prevents_spam() {
        let config = NotificationConfig {
            cooldown_secs: 60,
            ..default_config()
        };
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::Errored {
            at: Utc::now(),
            error_hint: Some("error".into()),
        };

        // First notification should go through
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
        let first_time = *manager.last_notified.get(&id).unwrap();

        // Second notification within cooldown should be skipped (timestamp unchanged)
        let new2 = AgentState::Errored {
            at: Utc::now(),
            error_hint: Some("error 2".into()),
        };
        manager.on_state_change(id, "agent", "project", &old, &new2);
        let second_time = *manager.last_notified.get(&id).unwrap();
        assert_eq!(first_time, second_time);
    }

    #[tokio::test]
    async fn test_different_agents_independent_cooldown() {
        let config = NotificationConfig {
            cooldown_secs: 60,
            ..default_config()
        };
        let mut manager = NotificationManager::new(&config);
        let id1 = make_agent_id();
        let id2 = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::Errored {
            at: Utc::now(),
            error_hint: Some("error".into()),
        };

        // Both agents should get their first notification
        manager.on_state_change(id1, "agent1", "project", &old, &new);
        manager.on_state_change(id2, "agent2", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 2);
    }

    #[tokio::test]
    async fn test_errored_without_hint() {
        let config = default_config();
        let mut manager = NotificationManager::new(&config);
        let id = make_agent_id();
        let old = AgentState::Running {
            since: Utc::now(),
        };
        let new = AgentState::Errored {
            at: Utc::now(),
            error_hint: None,
        };
        manager.on_state_change(id, "agent", "project", &old, &new);
        assert_eq!(manager.last_notified.len(), 1);
    }
}
