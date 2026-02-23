# Feature 20: Agent Output Export & Stream-JSON Mode (v0.3)

## Overview

Two complementary features: (1) Export an agent's terminal output to a Markdown file for documentation and review, and (2) An alternative "stream-JSON" mode where background/non-interactive agents run with `claude --output-format stream-json` for precise, structured state tracking instead of heuristic screen parsing.

## Dependencies

- **Feature 05** (Agent State Machine) — stream-JSON provides authoritative state.
- **Feature 06** (Agent Lifecycle) — agent spawning with different modes.
- **Feature 15** (Scrollback) — raw bytes for export.

## Technical Specification

### Part 1: Agent Output Export

#### Export Format

Export an agent's conversation to Markdown:

```markdown
# Agent: backend-refactor @ myapp
**Started:** 2026-02-23 10:15:00 UTC
**Duration:** 45m
**Status:** Completed (exit code 0)

---

## Terminal Output

```
$ claude --model opus
I'll start by reviewing the auth module...

Allow Edit to src/auth.rs? [Y/n] y

I've updated the authentication flow to use JWT tokens instead of session cookies.
Here's what I changed:

1. Added `jsonwebtoken` to Cargo.toml
2. Created `src/auth/jwt.rs` with token generation and validation
3. Updated `src/auth/middleware.rs` to check JWT headers
...
```
```

#### Export Implementation

```rust
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use color_eyre::eyre::Result;

pub struct OutputExporter;

impl OutputExporter {
    /// Export an agent's output to a Markdown file.
    pub fn export_to_markdown(
        agent_name: &str,
        project_name: &str,
        started_at: DateTime<Utc>,
        state_label: &str,
        screen_contents: &str,
        output_path: &Path,
    ) -> Result<PathBuf> {
        let duration = Utc::now() - started_at;
        let duration_str = if duration.num_hours() > 0 {
            format!("{}h {}m", duration.num_hours(), duration.num_minutes() % 60)
        } else {
            format!("{}m", duration.num_minutes())
        };

        let markdown = format!(
            "# Agent: {} @ {}\n\
             **Started:** {}\n\
             **Duration:** {}\n\
             **Status:** {}\n\
             \n\
             ---\n\
             \n\
             ## Terminal Output\n\
             \n\
             ```\n\
             {}\n\
             ```\n",
            agent_name,
            project_name,
            started_at.format("%Y-%m-%d %H:%M:%S UTC"),
            duration_str,
            state_label,
            screen_contents,
        );

        // Generate filename
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{}_{}_{}.md",
            project_name,
            agent_name,
            state_label.to_lowercase().replace(' ', "_"),
            timestamp,
        );
        let file_path = output_path.join(filename);

        std::fs::create_dir_all(output_path)?;
        std::fs::write(&file_path, markdown)?;

        tracing::info!("Exported agent output to {}", file_path.display());
        Ok(file_path)
    }

    /// Export using the scrollback buffer for complete history.
    pub fn export_from_scrollback(
        agent_name: &str,
        project_name: &str,
        started_at: DateTime<Utc>,
        state_label: &str,
        raw_bytes: &[u8],
        output_path: &Path,
    ) -> Result<PathBuf> {
        // Re-parse the raw bytes through a fresh vt100 parser to get clean text
        let mut parser = vt100::Parser::new(24, 120, 0);
        parser.process(raw_bytes);
        let screen_contents = parser.screen().contents();

        Self::export_to_markdown(
            agent_name,
            project_name,
            started_at,
            state_label,
            &screen_contents,
            output_path,
        )
    }
}
```

#### Export Trigger

- **Command palette**: `export <agent-name>` or `export` (exports focused agent).
- **Automatic on completion**: Optionally export when an agent completes (configurable).
- **Keybinding**: Not assigned by default (use command palette).

#### Configuration

```toml
[export]
# Directory for exported files (supports ~)
output_dir = "~/.local/share/maestro/exports"
# Automatically export when an agent completes
auto_export_on_complete = false
# Parser dimensions for scrollback re-parsing
export_cols = 120
export_rows = 50
```

### Part 2: Stream-JSON Mode

#### Concept

Claude Code supports `--output-format stream-json` which outputs structured JSON events instead of TUI output. This is ideal for **non-interactive background agents** because:
- State is explicit: tool use, thinking, completion events are clearly typed.
- No heuristic screen parsing needed.
- Output can be parsed programmatically.

However, stream-JSON agents **cannot receive interactive input** (no tool approvals, no questions). They must run in a fully autonomous mode (e.g., `claude -p "do X" --output-format stream-json --allowedTools all`).

#### Agent Mode Enum

```rust
/// How an agent interacts with its process.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentMode {
    /// Full interactive PTY mode (default).
    /// Agent runs as a TUI, user can type, approve tools, etc.
    Interactive,

    /// Stream-JSON mode for background/autonomous agents.
    /// Agent runs with `--output-format stream-json`.
    /// No interactive input possible.
    StreamJson,
}
```

#### Stream-JSON Event Parsing

Claude Code's stream-JSON format emits one JSON object per line:

```jsonl
{"type":"system","subtype":"init","session_id":"abc","model":"opus"}
{"type":"assistant","subtype":"thinking","text":"Let me analyze..."}
{"type":"assistant","subtype":"text","text":"I'll start by..."}
{"type":"tool_use","tool":"Edit","input":{"file":"src/main.rs",...}}
{"type":"tool_result","tool":"Edit","output":"File edited successfully"}
{"type":"assistant","subtype":"text","text":"Done! I've updated..."}
{"type":"result","subtype":"success","cost":{"input_tokens":1234,"output_tokens":567}}
```

```rust
use serde::Deserialize;

/// A parsed event from Claude Code's stream-JSON output.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "system")]
    System {
        subtype: String,
        #[serde(flatten)]
        data: serde_json::Value,
    },

    #[serde(rename = "assistant")]
    Assistant {
        subtype: String,
        text: Option<String>,
    },

    #[serde(rename = "tool_use")]
    ToolUse {
        tool: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool: String,
        output: Option<String>,
        error: Option<String>,
    },

    #[serde(rename = "result")]
    Result {
        subtype: String,
        cost: Option<serde_json::Value>,
    },
}

/// Parse a line of stream-JSON output.
pub fn parse_stream_event(line: &str) -> Option<StreamEvent> {
    serde_json::from_str(line).ok()
}
```

#### Stream-JSON Agent Handle

For stream-JSON agents, the `AgentHandle` works differently:
- No `vt100::Parser` (output is JSON, not ANSI).
- State is derived from parsed events, not screen heuristics.
- Terminal pane shows a custom rendering of events (formatted text, not raw TUI).

```rust
pub struct StreamJsonState {
    /// All received events.
    events: Vec<StreamEvent>,
    /// Current activity description (for sidebar).
    current_activity: String,
    /// Whether the agent has completed.
    completed: bool,
    /// Error message if the agent errored.
    error: Option<String>,
    /// Cost tracking.
    total_input_tokens: u64,
    total_output_tokens: u64,
}

impl StreamJsonState {
    pub fn process_event(&mut self, event: StreamEvent) {
        match &event {
            StreamEvent::Assistant { text: Some(text), .. } => {
                self.current_activity = truncate(text, 60);
            }
            StreamEvent::ToolUse { tool, .. } => {
                self.current_activity = format!("Using {}", tool);
            }
            StreamEvent::Result { subtype, cost } => {
                self.completed = true;
                if subtype == "error" {
                    self.error = Some("Agent reported error".into());
                }
                if let Some(cost) = cost {
                    // Extract token counts
                }
            }
            _ => {}
        }
        self.events.push(event);
    }

    /// Derive the agent state from stream events.
    pub fn to_agent_state(&self) -> AgentState {
        if self.completed {
            if self.error.is_some() {
                AgentState::Errored {
                    at: Utc::now(),
                    exit_code: None,
                    error_hint: self.error.clone(),
                }
            } else {
                AgentState::Completed {
                    at: Utc::now(),
                    exit_code: 0,
                }
            }
        } else if self.events.is_empty() {
            AgentState::Spawning
        } else {
            AgentState::Running { since: Utc::now() }
        }
    }
}
```

#### Custom Rendering for Stream-JSON Agents

Instead of rendering via tui-term, stream-JSON agents get a formatted text view:

```
┌─ Agent: background-task @ myapp [R] (stream-json) ─┐
│                                                      │
│ 10:15:01  Thinking: Let me analyze the code...       │
│ 10:15:03  Using Edit: src/auth.rs                    │
│ 10:15:05  Tool result: File edited successfully      │
│ 10:15:06  Thinking: Now I'll update the tests...     │
│ 10:15:08  Using Edit: tests/auth_test.rs             │
│ 10:15:10  Tool result: File edited successfully      │
│                                                      │
│ Tokens: 1,234 in / 567 out                           │
└──────────────────────────────────────────────────────┘
```

#### Configuration for Stream-JSON Agents

```toml
[[project.agent]]
name = "background-task"
command = "claude"
args = ["-p", "Refactor the auth module", "--output-format", "stream-json", "--allowedTools", "Edit,Bash"]
mode = "stream-json"  # NEW: tells Maestro to use stream-JSON parsing
auto_start = true

# Template example
[[template]]
name = "background"
command = "claude"
args = ["-p", "{prompt}", "--output-format", "stream-json", "--allowedTools", "all"]
mode = "stream-json"
description = "Non-interactive background agent"
```

### Part 3: Auto-Restart Policy

As a bonus in this feature, implement configurable auto-restart for agents:

```toml
[[project.agent]]
name = "watcher"
command = "claude"
auto_start = true
# Auto-restart configuration
auto_restart = true
max_restarts = 3
restart_delay_secs = 5
restart_backoff_multiplier = 2.0  # Exponential backoff
```

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RestartPolicy {
    pub auto_restart: bool,
    pub max_restarts: u32,
    pub restart_delay_secs: u64,
    pub restart_backoff_multiplier: f64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            auto_restart: false,
            max_restarts: 3,
            restart_delay_secs: 5,
            restart_backoff_multiplier: 2.0,
        }
    }
}

pub struct RestartTracker {
    restart_count: u32,
    last_restart: Option<Instant>,
    policy: RestartPolicy,
}

impl RestartTracker {
    /// Should this agent be auto-restarted?
    pub fn should_restart(&self) -> bool {
        self.policy.auto_restart && self.restart_count < self.policy.max_restarts
    }

    /// Calculate the delay before the next restart.
    pub fn next_delay(&self) -> Duration {
        let base = self.policy.restart_delay_secs as f64;
        let multiplied = base * self.policy.restart_backoff_multiplier.powi(self.restart_count as i32);
        Duration::from_secs_f64(multiplied.min(300.0)) // Cap at 5 minutes
    }

    /// Record a restart.
    pub fn record_restart(&mut self) {
        self.restart_count += 1;
        self.last_restart = Some(Instant::now());
    }
}
```

## Implementation Steps

### Output Export
1. **Implement `OutputExporter`** in `src/export/mod.rs`.
2. **Add `export` command** to the command palette.
3. **Add `[export]` config section**.
4. **Optional auto-export** on agent completion.

### Stream-JSON Mode
5. **Add `AgentMode` enum** to agent config.
6. **Implement `StreamEvent` parsing** in `src/agent/stream_json.rs`.
7. **Implement `StreamJsonState`** for event-based state tracking.
8. **Create a stream-JSON terminal pane widget** (formatted text instead of tui-term).
9. **Update `AgentHandle`** to support both interactive and stream-JSON modes.
10. **Update `AgentManager::spawn()`** to handle mode-specific spawning.

### Auto-Restart
11. **Add `RestartPolicy` and `RestartTracker`**.
12. **Integrate with state detection** — when an agent enters Completed/Errored, check if auto-restart is configured.
13. **Schedule restart with backoff** using `tokio::time::sleep`.

## Error Handling

| Scenario | Handling |
|---|---|
| Export directory doesn't exist | Create it (`create_dir_all`). |
| Export write fails (disk full) | Return error to user via status bar message. |
| Stream-JSON parse error | Log warning, skip the malformed line. |
| Agent exits without "result" event | Treat as error. Use process exit code for state. |
| Auto-restart limit reached | Log info, stop restarting. Show in sidebar. |

## Testing Strategy

### Unit Tests — Export

```rust
#[test]
fn test_export_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = OutputExporter::export_to_markdown(
        "agent", "project",
        Utc::now(),
        "Completed",
        "hello world",
        dir.path(),
    ).unwrap();
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("# Agent: agent @ project"));
    assert!(content.contains("hello world"));
}
```

### Unit Tests — Stream-JSON

```rust
#[test]
fn test_parse_assistant_event() {
    let line = r#"{"type":"assistant","subtype":"text","text":"Hello"}"#;
    let event = parse_stream_event(line).unwrap();
    assert!(matches!(event, StreamEvent::Assistant { .. }));
}

#[test]
fn test_parse_tool_use_event() {
    let line = r#"{"type":"tool_use","tool":"Edit","input":{}}"#;
    let event = parse_stream_event(line).unwrap();
    assert!(matches!(event, StreamEvent::ToolUse { tool, .. } if tool == "Edit"));
}

#[test]
fn test_stream_state_running() {
    let mut state = StreamJsonState::default();
    state.process_event(StreamEvent::Assistant {
        subtype: "text".into(),
        text: Some("Working...".into()),
    });
    assert!(matches!(state.to_agent_state(), AgentState::Running { .. }));
}

#[test]
fn test_stream_state_completed() {
    let mut state = StreamJsonState::default();
    state.process_event(StreamEvent::Result {
        subtype: "success".into(),
        cost: None,
    });
    assert!(matches!(state.to_agent_state(), AgentState::Completed { .. }));
}
```

### Unit Tests — Auto-Restart

```rust
#[test]
fn test_should_restart() {
    let tracker = RestartTracker {
        restart_count: 0,
        policy: RestartPolicy { auto_restart: true, max_restarts: 3, ..Default::default() },
        ..Default::default()
    };
    assert!(tracker.should_restart());
}

#[test]
fn test_should_not_restart_at_limit() {
    let tracker = RestartTracker {
        restart_count: 3,
        policy: RestartPolicy { auto_restart: true, max_restarts: 3, ..Default::default() },
        ..Default::default()
    };
    assert!(!tracker.should_restart());
}

#[test]
fn test_backoff_delay() {
    let tracker = RestartTracker {
        restart_count: 2,
        policy: RestartPolicy {
            restart_delay_secs: 5,
            restart_backoff_multiplier: 2.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let delay = tracker.next_delay();
    assert_eq!(delay, Duration::from_secs(20)); // 5 * 2^2 = 20
}
```

## Acceptance Criteria

### Output Export
- [ ] `export` command palette command exports the focused agent to Markdown.
- [ ] Exported file contains agent name, project, duration, status, and output.
- [ ] Export uses scrollback buffer for complete history.
- [ ] Auto-export on completion is configurable.
- [ ] Export directory is configurable.

### Stream-JSON Mode
- [ ] Agents can be configured with `mode = "stream-json"`.
- [ ] Stream-JSON events are parsed and tracked.
- [ ] State is derived from events (Running, Completed, Errored) — no screen heuristics.
- [ ] Terminal pane shows formatted event log instead of raw TUI.
- [ ] Token usage is displayed.
- [ ] Malformed JSON lines are skipped without crashing.

### Auto-Restart
- [ ] Agents with `auto_restart = true` are restarted on exit.
- [ ] Restart count respects `max_restarts` limit.
- [ ] Exponential backoff between restarts.
- [ ] Restart delay is capped at 5 minutes.
- [ ] Restart status is visible in the sidebar.
