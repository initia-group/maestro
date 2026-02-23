//! Stream-JSON mode for non-interactive background agents.
//!
//! Parses Claude Code's `--output-format stream-json` output into structured
//! events and maintains agent state derived from those events rather than
//! heuristic screen parsing.
//! See Feature 20 (Output Export & Stream-JSON) for the full spec.

use crate::agent::state::AgentState;
use chrono::Utc;
use serde::Deserialize;

/// How an agent interacts with its process.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum AgentMode {
    /// Full interactive PTY mode (default).
    /// Agent runs as a TUI, user can type, approve tools, etc.
    #[default]
    Interactive,

    /// Stream-JSON mode for background/autonomous agents.
    /// Agent runs with `--output-format stream-json`.
    /// No interactive input possible.
    StreamJson,
}

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
///
/// Returns `None` if the line is empty, whitespace-only, or malformed JSON.
/// Malformed lines are silently skipped (logged elsewhere if desired).
pub fn parse_stream_event(line: &str) -> Option<StreamEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

/// Truncate a string to at most `max_len` characters, appending "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = max_len.saturating_sub(3);
        format!("{}...", &s[..end])
    }
}

/// State tracking for a stream-JSON agent.
///
/// Processes structured events and derives agent state from them,
/// replacing the heuristic screen-parsing approach used for interactive agents.
#[derive(Debug, Default)]
pub struct StreamJsonState {
    /// All received events.
    events: Vec<StreamEvent>,
    /// Current activity description (for sidebar display).
    current_activity: String,
    /// Whether the agent has completed.
    completed: bool,
    /// Error message if the agent errored.
    error: Option<String>,
    /// Cumulative input token count.
    total_input_tokens: u64,
    /// Cumulative output token count.
    total_output_tokens: u64,
}

impl StreamJsonState {
    /// Create a new empty stream-JSON state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a single stream event, updating internal state.
    pub fn process_event(&mut self, event: StreamEvent) {
        match &event {
            StreamEvent::Assistant {
                text: Some(text), ..
            } => {
                self.current_activity = truncate(text, 60);
            }
            StreamEvent::ToolUse { tool, .. } => {
                self.current_activity = format!("Using {}", tool);
            }
            StreamEvent::ToolResult { error: Some(e), .. } => {
                self.current_activity = format!("Tool error: {}", truncate(e, 50));
            }
            StreamEvent::Result { subtype, cost } => {
                self.completed = true;
                if subtype == "error" {
                    self.error = Some("Agent reported error".into());
                }
                if let Some(cost) = cost {
                    if let Some(input) = cost.get("input_tokens").and_then(|v| v.as_u64()) {
                        self.total_input_tokens += input;
                    }
                    if let Some(output) = cost.get("output_tokens").and_then(|v| v.as_u64()) {
                        self.total_output_tokens += output;
                    }
                }
            }
            _ => {}
        }
        self.events.push(event);
    }

    /// Process a raw output line from the stream-JSON process.
    ///
    /// Parses the line as JSON and processes the resulting event.
    /// Returns `true` if the line was successfully parsed, `false` if skipped.
    pub fn process_line(&mut self, line: &str) -> bool {
        if let Some(event) = parse_stream_event(line) {
            self.process_event(event);
            true
        } else {
            false
        }
    }

    /// Derive the agent state from the stream events received so far.
    pub fn to_agent_state(&self) -> AgentState {
        if self.completed {
            if self.error.is_some() {
                AgentState::Errored {
                    at: Utc::now(),
                    error_hint: self.error.clone(),
                }
            } else {
                AgentState::Completed {
                    at: Utc::now(),
                    exit_code: Some(0),
                }
            }
        } else if self.events.is_empty() {
            AgentState::Spawning { since: Utc::now() }
        } else {
            AgentState::Running { since: Utc::now() }
        }
    }

    /// Get the current activity description for sidebar display.
    pub fn current_activity(&self) -> &str {
        &self.current_activity
    }

    /// Whether the agent has completed (success or error).
    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// Get the error message, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Get the total input tokens consumed.
    pub fn total_input_tokens(&self) -> u64 {
        self.total_input_tokens
    }

    /// Get the total output tokens consumed.
    pub fn total_output_tokens(&self) -> u64 {
        self.total_output_tokens
    }

    /// Get all received events.
    pub fn events(&self) -> &[StreamEvent] {
        &self.events
    }

    /// Get the total number of events received.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Render a text summary of the event log for display in the terminal pane.
    ///
    /// Returns a multi-line string with timestamps and event descriptions,
    /// suitable for rendering in a custom widget instead of tui-term.
    pub fn render_event_log(&self) -> String {
        let mut lines = Vec::new();

        for event in &self.events {
            match event {
                StreamEvent::System { subtype, .. } => {
                    lines.push(format!("  System: {}", subtype));
                }
                StreamEvent::Assistant { subtype, text } => {
                    let description = if let Some(text) = text {
                        truncate(text, 70)
                    } else {
                        subtype.clone()
                    };
                    let label = if subtype == "thinking" {
                        "Thinking"
                    } else {
                        "Assistant"
                    };
                    lines.push(format!("  {}: {}", label, description));
                }
                StreamEvent::ToolUse { tool, .. } => {
                    lines.push(format!("  Using {}", tool));
                }
                StreamEvent::ToolResult {
                    tool,
                    output,
                    error,
                } => {
                    if let Some(err) = error {
                        lines.push(format!("  {} error: {}", tool, truncate(err, 60)));
                    } else if let Some(out) = output {
                        lines.push(format!("  {} result: {}", tool, truncate(out, 60)));
                    } else {
                        lines.push(format!("  {} completed", tool));
                    }
                }
                StreamEvent::Result { subtype, .. } => {
                    lines.push(format!("  Result: {}", subtype));
                }
            }
        }

        if self.total_input_tokens > 0 || self.total_output_tokens > 0 {
            lines.push(String::new());
            lines.push(format!(
                "  Tokens: {} in / {} out",
                self.total_input_tokens, self.total_output_tokens
            ));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- AgentMode tests ---

    #[test]
    fn test_agent_mode_default() {
        assert_eq!(AgentMode::default(), AgentMode::Interactive);
    }

    #[test]
    fn test_agent_mode_equality() {
        assert_eq!(AgentMode::Interactive, AgentMode::Interactive);
        assert_eq!(AgentMode::StreamJson, AgentMode::StreamJson);
        assert_ne!(AgentMode::Interactive, AgentMode::StreamJson);
    }

    // --- parse_stream_event tests ---

    #[test]
    fn test_parse_assistant_event() {
        let line = r#"{"type":"assistant","subtype":"text","text":"Hello"}"#;
        let event = parse_stream_event(line).unwrap();
        assert!(matches!(event, StreamEvent::Assistant { .. }));
        if let StreamEvent::Assistant { subtype, text } = event {
            assert_eq!(subtype, "text");
            assert_eq!(text.unwrap(), "Hello");
        }
    }

    #[test]
    fn test_parse_assistant_thinking_event() {
        let line = r#"{"type":"assistant","subtype":"thinking","text":"Let me analyze..."}"#;
        let event = parse_stream_event(line).unwrap();
        if let StreamEvent::Assistant { subtype, text } = event {
            assert_eq!(subtype, "thinking");
            assert_eq!(text.unwrap(), "Let me analyze...");
        } else {
            panic!("Expected Assistant event");
        }
    }

    #[test]
    fn test_parse_tool_use_event() {
        let line = r#"{"type":"tool_use","tool":"Edit","input":{}}"#;
        let event = parse_stream_event(line).unwrap();
        assert!(matches!(event, StreamEvent::ToolUse { ref tool, .. } if tool == "Edit"));
    }

    #[test]
    fn test_parse_tool_result_event() {
        let line =
            r#"{"type":"tool_result","tool":"Edit","output":"File edited successfully"}"#;
        let event = parse_stream_event(line).unwrap();
        if let StreamEvent::ToolResult { tool, output, error } = event {
            assert_eq!(tool, "Edit");
            assert_eq!(output.unwrap(), "File edited successfully");
            assert!(error.is_none());
        } else {
            panic!("Expected ToolResult event");
        }
    }

    #[test]
    fn test_parse_tool_result_error_event() {
        let line =
            r#"{"type":"tool_result","tool":"Bash","error":"Command failed"}"#;
        let event = parse_stream_event(line).unwrap();
        if let StreamEvent::ToolResult { tool, output, error } = event {
            assert_eq!(tool, "Bash");
            assert!(output.is_none());
            assert_eq!(error.unwrap(), "Command failed");
        } else {
            panic!("Expected ToolResult event");
        }
    }

    #[test]
    fn test_parse_system_event() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc","model":"opus"}"#;
        let event = parse_stream_event(line).unwrap();
        if let StreamEvent::System { subtype, data } = event {
            assert_eq!(subtype, "init");
            assert_eq!(data.get("session_id").unwrap().as_str().unwrap(), "abc");
        } else {
            panic!("Expected System event");
        }
    }

    #[test]
    fn test_parse_result_success_event() {
        let line = r#"{"type":"result","subtype":"success","cost":{"input_tokens":1234,"output_tokens":567}}"#;
        let event = parse_stream_event(line).unwrap();
        if let StreamEvent::Result { subtype, cost } = event {
            assert_eq!(subtype, "success");
            let cost = cost.unwrap();
            assert_eq!(cost.get("input_tokens").unwrap().as_u64().unwrap(), 1234);
            assert_eq!(cost.get("output_tokens").unwrap().as_u64().unwrap(), 567);
        } else {
            panic!("Expected Result event");
        }
    }

    #[test]
    fn test_parse_result_error_event() {
        let line = r#"{"type":"result","subtype":"error"}"#;
        let event = parse_stream_event(line).unwrap();
        if let StreamEvent::Result { subtype, cost } = event {
            assert_eq!(subtype, "error");
            assert!(cost.is_none());
        } else {
            panic!("Expected Result event");
        }
    }

    #[test]
    fn test_parse_empty_line() {
        assert!(parse_stream_event("").is_none());
        assert!(parse_stream_event("   ").is_none());
        assert!(parse_stream_event("\n").is_none());
    }

    #[test]
    fn test_parse_malformed_json() {
        assert!(parse_stream_event("not json").is_none());
        assert!(parse_stream_event("{incomplete").is_none());
        assert!(parse_stream_event(r#"{"type":"unknown_type"}"#).is_none());
    }

    #[test]
    fn test_parse_whitespace_trimmed() {
        let line = r#"  {"type":"assistant","subtype":"text","text":"Hello"}  "#;
        let event = parse_stream_event(line);
        assert!(event.is_some());
    }

    // --- StreamJsonState tests ---

    #[test]
    fn test_stream_state_initial() {
        let state = StreamJsonState::new();
        assert!(state.events.is_empty());
        assert!(state.current_activity.is_empty());
        assert!(!state.completed);
        assert!(state.error.is_none());
        assert_eq!(state.total_input_tokens, 0);
        assert_eq!(state.total_output_tokens, 0);
    }

    #[test]
    fn test_stream_state_spawning() {
        let state = StreamJsonState::new();
        assert!(matches!(state.to_agent_state(), AgentState::Spawning { .. }));
    }

    #[test]
    fn test_stream_state_running() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::Assistant {
            subtype: "text".into(),
            text: Some("Working...".into()),
        });
        assert!(matches!(state.to_agent_state(), AgentState::Running { .. }));
        assert_eq!(state.current_activity(), "Working...");
    }

    #[test]
    fn test_stream_state_completed() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::Result {
            subtype: "success".into(),
            cost: None,
        });
        assert!(matches!(
            state.to_agent_state(),
            AgentState::Completed { .. }
        ));
        assert!(state.is_completed());
        assert!(state.error().is_none());
    }

    #[test]
    fn test_stream_state_errored() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::Result {
            subtype: "error".into(),
            cost: None,
        });
        let agent_state = state.to_agent_state();
        assert!(matches!(agent_state, AgentState::Errored { .. }));
        assert!(state.is_completed());
        assert!(state.error().is_some());
    }

    #[test]
    fn test_stream_state_tool_use_activity() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::ToolUse {
            tool: "Edit".into(),
            input: serde_json::Value::Object(serde_json::Map::new()),
        });
        assert_eq!(state.current_activity(), "Using Edit");
    }

    #[test]
    fn test_stream_state_tool_error_activity() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::ToolResult {
            tool: "Bash".into(),
            output: None,
            error: Some("Command not found".into()),
        });
        assert!(state.current_activity().contains("Tool error"));
    }

    #[test]
    fn test_stream_state_token_tracking() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::Result {
            subtype: "success".into(),
            cost: Some(serde_json::json!({
                "input_tokens": 1234,
                "output_tokens": 567,
            })),
        });
        assert_eq!(state.total_input_tokens(), 1234);
        assert_eq!(state.total_output_tokens(), 567);
    }

    #[test]
    fn test_stream_state_event_count() {
        let mut state = StreamJsonState::new();
        assert_eq!(state.event_count(), 0);

        state.process_event(StreamEvent::Assistant {
            subtype: "text".into(),
            text: Some("Hello".into()),
        });
        assert_eq!(state.event_count(), 1);

        state.process_event(StreamEvent::ToolUse {
            tool: "Edit".into(),
            input: serde_json::Value::Null,
        });
        assert_eq!(state.event_count(), 2);
    }

    #[test]
    fn test_stream_state_process_line() {
        let mut state = StreamJsonState::new();

        // Valid line
        let ok = state.process_line(r#"{"type":"assistant","subtype":"text","text":"Hi"}"#);
        assert!(ok);
        assert_eq!(state.event_count(), 1);

        // Malformed line
        let ok = state.process_line("not json");
        assert!(!ok);
        assert_eq!(state.event_count(), 1); // unchanged

        // Empty line
        let ok = state.process_line("");
        assert!(!ok);
        assert_eq!(state.event_count(), 1); // unchanged
    }

    #[test]
    fn test_stream_state_long_text_truncated() {
        let mut state = StreamJsonState::new();
        let long_text = "x".repeat(200);
        state.process_event(StreamEvent::Assistant {
            subtype: "text".into(),
            text: Some(long_text),
        });
        assert!(state.current_activity().len() <= 60);
        assert!(state.current_activity().ends_with("..."));
    }

    #[test]
    fn test_render_event_log_empty() {
        let state = StreamJsonState::new();
        let log = state.render_event_log();
        assert!(log.is_empty());
    }

    #[test]
    fn test_render_event_log_with_events() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::System {
            subtype: "init".into(),
            data: serde_json::json!({"session_id": "abc"}),
        });
        state.process_event(StreamEvent::Assistant {
            subtype: "thinking".into(),
            text: Some("Let me analyze...".into()),
        });
        state.process_event(StreamEvent::ToolUse {
            tool: "Edit".into(),
            input: serde_json::json!({"file": "src/main.rs"}),
        });
        state.process_event(StreamEvent::ToolResult {
            tool: "Edit".into(),
            output: Some("File edited successfully".into()),
            error: None,
        });

        let log = state.render_event_log();
        assert!(log.contains("System: init"));
        assert!(log.contains("Thinking: Let me analyze..."));
        assert!(log.contains("Using Edit"));
        assert!(log.contains("Edit result: File edited successfully"));
    }

    #[test]
    fn test_render_event_log_with_tokens() {
        let mut state = StreamJsonState::new();
        state.process_event(StreamEvent::Result {
            subtype: "success".into(),
            cost: Some(serde_json::json!({
                "input_tokens": 1000,
                "output_tokens": 500,
            })),
        });

        let log = state.render_event_log();
        assert!(log.contains("Tokens: 1000 in / 500 out"));
    }

    // --- truncate tests ---

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = truncate("hello world this is a long string", 15);
        assert!(result.len() <= 15);
        assert!(result.ends_with("..."));
    }
}
