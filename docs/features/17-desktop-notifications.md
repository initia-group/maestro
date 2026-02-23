# Feature 17: Desktop Notifications (v0.3)

## Overview

Send OS-level desktop notifications when agents need attention, complete tasks, or encounter errors. This is essential when Maestro runs in the background (minimized terminal, another workspace) — users get notified without constantly watching the dashboard.

## Dependencies

- **Feature 05** (Agent State Machine) — state changes trigger notifications.
- **Feature 12** (App Bootstrap) — integrates with the main event loop.

## Technical Specification

### Notification Triggers

| Agent State Change | Notification? | Title | Body Example |
|---|---|---|---|
| Any → `WaitingForInput(ToolApproval)` | Yes | "Agent needs approval" | "backend-refactor: Allow Edit to src/auth.rs?" |
| Any → `WaitingForInput(Question)` | Yes | "Agent has a question" | "backend-refactor is waiting for your input" |
| Any → `WaitingForInput(InputPrompt)` | Yes (configurable) | "Agent waiting for input" | "backend-refactor is ready for a new prompt" |
| Any → `Completed` | Yes | "Agent completed" | "backend-refactor finished successfully (12m)" |
| Any → `Errored` | Yes | "Agent error" | "backend-refactor: Error: API connection failed" |
| Any → `Idle` | No | — | Too frequent, would be noisy |
| `WaitingForInput` → `Running` | No | — | User provided input, no notification needed |

### Notification Crate

Use the `notify-rust` crate (cross-platform: macOS, Linux, Windows):

```toml
[dependencies]
notify-rust = "4"
```

### Notification Manager

```rust
use notify_rust::Notification;
use crate::agent::state::{AgentState, PromptType};
use crate::config::settings::NotificationConfig;
use std::time::{Duration, Instant};

/// Manages desktop notification delivery.
pub struct NotificationManager {
    /// Whether notifications are enabled.
    enabled: bool,
    /// Minimum time between notifications for the same agent (prevents spam).
    cooldown: Duration,
    /// Last notification time per agent.
    last_notified: std::collections::HashMap<AgentId, Instant>,
    /// Whether to notify on input prompts (configurable, default false).
    notify_on_input_prompt: bool,
}

impl NotificationManager {
    pub fn new(config: &NotificationConfig) -> Self {
        Self {
            enabled: config.enabled,
            cooldown: Duration::from_secs(config.cooldown_secs),
            last_notified: std::collections::HashMap::new(),
            notify_on_input_prompt: config.notify_on_input_prompt,
        }
    }

    /// Called when an agent's state changes. Sends a notification if appropriate.
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
            AgentState::WaitingForInput { prompt_type, .. } => {
                match prompt_type {
                    PromptType::ToolApproval { tool_name } => Some((
                        "Agent needs approval",
                        format!("{}: Allow {} ?", agent_name, tool_name),
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
                }
            }
            AgentState::Completed { .. } => {
                let uptime = match old_state {
                    AgentState::Running { since } |
                    AgentState::WaitingForInput { since, .. } |
                    AgentState::Idle { since } => {
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
                Some((
                    "Agent error",
                    format!("{}: {}", agent_name, hint),
                ))
            }
            _ => None,
        };

        if let Some((title, body)) = notification {
            self.send(title, &body, agent_name, project_name);
            self.last_notified.insert(agent_id, Instant::now());
        }
    }

    fn send(&self, title: &str, body: &str, agent_name: &str, project_name: &str) {
        let full_title = format!("Maestro — {}", title);

        // Fire and forget — notification delivery should never block the event loop
        let title = full_title.clone();
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
```

### Configuration

```toml
[notifications]
# Enable desktop notifications
enabled = true
# Minimum seconds between notifications for the same agent
cooldown_secs = 10
# Notify when agent shows input prompt (can be noisy)
notify_on_input_prompt = false
```

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub cooldown_secs: u64,
    pub notify_on_input_prompt: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cooldown_secs: 10,
            notify_on_input_prompt: false,
        }
    }
}
```

### macOS-Specific Considerations

On macOS, `notify-rust` uses the native `NSUserNotification` API. For better integration:
- Set `appname("Maestro")` to group notifications.
- Consider bundling Maestro as a `.app` for proper notification center integration (v1.0).
- Sound can be added: `.sound_name("default")`.

### Linux-Specific Considerations

On Linux, `notify-rust` uses D-Bus via `libnotify`. Requires:
- `libdbus` development headers at build time.
- A notification daemon running (most desktop environments have one).

## Implementation Steps

1. **Add `notify-rust` to `Cargo.toml`**
   ```toml
   notify-rust = "4"
   ```

2. **Add `NotificationConfig` to settings**
   - Add `[notifications]` section to config structs.

3. **Implement `NotificationManager`**
   - State change handler.
   - Cooldown tracking.
   - Async notification sending.

4. **Integrate with `App`**
   - Create `NotificationManager` at startup.
   - Call `on_state_change()` whenever an agent state changes.

5. **Test on macOS and Linux**

## Error Handling

| Scenario | Handling |
|---|---|
| Notification daemon not available | Log warning, continue. Notifications silently fail. |
| D-Bus error (Linux) | Log warning per attempt, don't retry. |
| Permission denied | Log warning. On macOS, user may need to grant notification permission. |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_cooldown_prevents_spam() {
    let config = NotificationConfig { cooldown_secs: 60, ..Default::default() };
    let mut manager = NotificationManager::new(&config);

    // First notification should go through
    manager.on_state_change(id, "agent", "project", &old, &new);
    assert_eq!(manager.last_notified.len(), 1);

    // Second notification within cooldown should be skipped
    // (We can't easily test this without mocking time, so test the cooldown check logic)
}

#[test]
fn test_disabled_sends_nothing() {
    let config = NotificationConfig { enabled: false, ..Default::default() };
    let mut manager = NotificationManager::new(&config);
    manager.on_state_change(id, "agent", "project", &old, &new);
    assert!(manager.last_notified.is_empty());
}

#[test]
fn test_idle_does_not_notify() {
    let config = NotificationConfig::default();
    let mut manager = NotificationManager::new(&config);
    let old = AgentState::Running { since: Utc::now() };
    let new = AgentState::Idle { since: Utc::now() };
    manager.on_state_change(id, "agent", "project", &old, &new);
    assert!(manager.last_notified.is_empty()); // No notification for Idle
}
```

## Acceptance Criteria

- [ ] Notification sent when agent enters `WaitingForInput(ToolApproval)`.
- [ ] Notification sent when agent enters `WaitingForInput(Question)`.
- [ ] Notification sent when agent enters `Completed`.
- [ ] Notification sent when agent enters `Errored` (with error hint).
- [ ] No notification for `Idle` state.
- [ ] Cooldown prevents notification spam (configurable, default 10s).
- [ ] Notifications can be disabled via config.
- [ ] Notification delivery never blocks the event loop.
- [ ] Works on macOS (native notifications).
- [ ] Works on Linux (D-Bus/libnotify).
- [ ] Graceful failure if notification system is unavailable.
