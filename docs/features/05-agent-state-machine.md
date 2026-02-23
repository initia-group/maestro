# Feature 05: Agent State Machine & Detection

## Overview

Implement the agent state machine (defining all possible agent states and transitions) and the state detection system that infers an agent's current state by analyzing its terminal output and process status. This is the intelligence layer that gives Maestro its "at a glance" monitoring capability.

## Dependencies

- **Feature 01** (Project Scaffold) — module structure.
- **Feature 04** (PTY Management) — `PtyController` provides output and process handles for detection.

## Technical Specification

### State Machine (`src/agent/state.rs`)

```
                    ┌────────────┐
                    │  Spawning  │
                    └─────┬──────┘
                          │ first output received
                          ▼
    ┌─────────────────► Running ◄──────────────────┐
    │                     │ │                       │
    │                     │ │ pattern match on      │
    │                     │ │ bottom screen lines    │
    │  output resumes     │ ▼                       │ output resumes
    │                   WaitingForInput              │
    │                     │                         │
    │  no output for      │ no output for           │
    │  idle_timeout       │ idle_timeout            │
    │                     │                         │
    │                     ▼                         │
    └──────────────── Idle ─────────────────────────┘
                       │
                       │ process exits
                       ▼
              ┌────────────────┐
              │  Completed (0) │   exit code 0
              └────────────────┘
              ┌────────────────┐
              │  Errored (N)   │   exit code != 0 or signal
              └────────────────┘
```

```rust
use chrono::{DateTime, Utc};

/// The current state of an agent.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    /// Agent process is starting up. No output received yet.
    Spawning,

    /// Agent is actively producing output.
    Running {
        /// When the agent entered Running state.
        since: DateTime<Utc>,
    },

    /// Agent is waiting for user input (tool approval, question, etc.).
    WaitingForInput {
        /// What type of input is expected.
        prompt_type: PromptType,
        /// When the waiting was first detected.
        since: DateTime<Utc>,
    },

    /// Agent has not produced output for `idle_timeout_secs`.
    Idle {
        /// When idle was first detected.
        since: DateTime<Utc>,
    },

    /// Agent process exited successfully (exit code 0).
    Completed {
        /// When the process exited.
        at: DateTime<Utc>,
        /// The exit code (always 0 for Completed).
        exit_code: i32,
    },

    /// Agent process exited with an error.
    Errored {
        /// When the error was detected.
        at: DateTime<Utc>,
        /// The exit code, if available.
        exit_code: Option<i32>,
        /// A hint about the error, extracted from terminal output.
        error_hint: Option<String>,
    },
}

impl AgentState {
    /// Returns the status indicator symbol for display.
    pub fn symbol(&self) -> &'static str {
        match self {
            AgentState::Spawning => "○",
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
            AgentState::Spawning => "spawning",
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
            AgentState::Spawning => "spawning",
            AgentState::Running { .. } => "running",
            AgentState::WaitingForInput { .. } => "waiting",
            AgentState::Idle { .. } => "idle",
            AgentState::Completed { .. } => "done",
            AgentState::Errored { .. } => "error",
        }
    }

    /// Whether the agent is in a terminal state (no further transitions possible
    /// without explicit restart).
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Completed { .. } | AgentState::Errored { .. })
    }

    /// Whether the agent's process is still running.
    pub fn is_alive(&self) -> bool {
        !self.is_terminal()
    }
}

/// The type of input the agent is waiting for.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptType {
    /// Agent is asking for tool approval (e.g., "Allow Edit to src/auth.rs? [Y/n]").
    ToolApproval {
        /// Name of the tool requesting approval.
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
            PromptType::ToolApproval { tool_name } => format!("Tool: {}", tool_name),
            PromptType::Question => "Question".to_string(),
            PromptType::InputPrompt => "Input".to_string(),
            PromptType::Unknown => "Waiting".to_string(),
        }
    }
}
```

### State Detector (`src/agent/detector.rs`)

The detector runs every `state_check_interval_ms` (default 250ms) and checks each agent's state using a priority-ordered signal cascade.

```rust
use crate::agent::state::{AgentState, PromptType};
use crate::config::settings::DetectionConfig;
use chrono::Utc;
use regex::Regex;
use tracing::debug;

/// Compiled detection patterns for efficient repeated matching.
pub struct DetectionPatterns {
    /// Built-in tool approval patterns + user-configured additions.
    tool_approval: Vec<Regex>,
    /// Built-in error patterns + user-configured additions.
    error: Vec<Regex>,
    /// Built-in input prompt patterns + user-configured additions.
    input_prompt: Vec<Regex>,
    /// Number of bottom screen lines to scan.
    scan_lines: usize,
}

impl DetectionPatterns {
    /// Create from configuration, compiling all regex patterns.
    pub fn from_config(config: &DetectionConfig) -> Self {
        let mut tool_approval = vec![
            // Built-in patterns for Claude Code
            Regex::new(r"Allow\s+(\w+)").unwrap(),
            Regex::new(r"\[Y/n\]").unwrap(),
            Regex::new(r"\[y/N\]").unwrap(),
        ];
        for pattern in &config.tool_approval_patterns {
            if let Ok(re) = Regex::new(pattern) {
                tool_approval.push(re);
            }
        }

        let mut error = vec![
            Regex::new(r"(?i)error:").unwrap(),
            Regex::new(r"(?i)api\s+error").unwrap(),
            Regex::new(r"(?i)rate\s+limit").unwrap(),
            Regex::new(r"(?i)connection\s+refused").unwrap(),
            Regex::new(r"(?i)ECONNREFUSED").unwrap(),
            Regex::new(r"(?i)timeout").unwrap(),
        ];
        for pattern in &config.error_patterns {
            if let Ok(re) = Regex::new(pattern) {
                error.push(re);
            }
        }

        let mut input_prompt = vec![
            // Claude Code's main input prompt
            Regex::new(r"^>\s*$").unwrap(),
            Regex::new(r"^\$\s*$").unwrap(),
        ];
        for pattern in &config.input_prompt_patterns {
            if let Ok(re) = Regex::new(pattern) {
                input_prompt.push(re);
            }
        }

        Self {
            tool_approval,
            error,
            input_prompt,
            scan_lines: config.scan_lines,
        }
    }
}

/// Signals available for state detection, in priority order.
pub struct DetectionSignals<'a> {
    /// Has the child process exited? If so, what was the exit status?
    pub process_exited: Option<ProcessExit>,
    /// The vt100 screen content (bottom N lines).
    pub screen_lines: Vec<String>,
    /// Seconds since last PTY output was received.
    pub seconds_since_output: f64,
    /// The current state (for hysteresis — avoid flapping).
    pub current_state: &'a AgentState,
    /// Configured idle timeout in seconds.
    pub idle_timeout_secs: u64,
}

/// Information about a process exit.
pub struct ProcessExit {
    /// The exit code, if available.
    pub exit_code: Option<i32>,
    /// Whether the process was killed by a signal.
    pub signal: bool,
}

/// Determine the new state for an agent based on available signals.
///
/// Signal priority (highest to lowest):
/// 1. Process exit → Completed or Errored (authoritative)
/// 2. Screen content patterns → WaitingForInput or Errored (heuristic)
/// 3. Output timing → Idle or Running (fallback)
pub fn detect_state(
    patterns: &DetectionPatterns,
    signals: &DetectionSignals,
) -> AgentState {
    // ─── Priority 1: Process exit (most reliable) ──────────
    if let Some(exit) = &signals.process_exited {
        let at = Utc::now();
        return match exit.exit_code {
            Some(0) => AgentState::Completed {
                at,
                exit_code: 0,
            },
            Some(code) => AgentState::Errored {
                at,
                exit_code: Some(code),
                error_hint: extract_error_hint(&signals.screen_lines),
            },
            None => {
                if exit.signal {
                    AgentState::Errored {
                        at,
                        exit_code: None,
                        error_hint: Some("Killed by signal".to_string()),
                    }
                } else {
                    AgentState::Completed {
                        at,
                        exit_code: 0,
                    }
                }
            }
        };
    }

    // Don't re-detect if already in a terminal state
    if signals.current_state.is_terminal() {
        return signals.current_state.clone();
    }

    // ─── Priority 2: Screen content patterns ────────────────
    let bottom_lines = &signals.screen_lines;

    // Check for tool approval prompts
    if let Some(tool_name) = detect_tool_approval(patterns, bottom_lines) {
        // Only transition if not already in the correct WaitingForInput state
        // (hysteresis — avoid resetting `since` timestamp)
        if let AgentState::WaitingForInput {
            prompt_type: PromptType::ToolApproval { tool_name: existing },
            ..
        } = signals.current_state
        {
            if *existing == tool_name {
                return signals.current_state.clone();
            }
        }
        return AgentState::WaitingForInput {
            prompt_type: PromptType::ToolApproval { tool_name },
            since: Utc::now(),
        };
    }

    // Check for input prompts
    if detect_input_prompt(patterns, bottom_lines) {
        if matches!(
            signals.current_state,
            AgentState::WaitingForInput { prompt_type: PromptType::InputPrompt, .. }
        ) {
            return signals.current_state.clone();
        }
        return AgentState::WaitingForInput {
            prompt_type: PromptType::InputPrompt,
            since: Utc::now(),
        };
    }

    // Check for question prompts (line ending with ?)
    if detect_question(bottom_lines) {
        if matches!(
            signals.current_state,
            AgentState::WaitingForInput { prompt_type: PromptType::Question, .. }
        ) {
            return signals.current_state.clone();
        }
        return AgentState::WaitingForInput {
            prompt_type: PromptType::Question,
            since: Utc::now(),
        };
    }

    // Check for error patterns in screen content
    // Note: Only transition to Errored from screen patterns if the process is still running
    // but showing persistent errors. Process exit is handled above.
    // We do NOT auto-transition to Errored from screen content in v0.1.
    // Instead, we log it. The error patterns are used primarily for error_hint extraction.

    // ─── Priority 3: Output timing ─────────────────────────
    if signals.seconds_since_output >= signals.idle_timeout_secs as f64 {
        if matches!(signals.current_state, AgentState::Idle { .. }) {
            return signals.current_state.clone(); // hysteresis
        }
        return AgentState::Idle {
            since: Utc::now(),
        };
    }

    // Active output → Running
    if matches!(signals.current_state, AgentState::Running { .. }) {
        return signals.current_state.clone(); // hysteresis
    }
    AgentState::Running {
        since: Utc::now(),
    }
}

/// Extract the tool name from tool approval patterns.
fn detect_tool_approval(patterns: &DetectionPatterns, lines: &[String]) -> Option<String> {
    // Look for "[Y/n]" or "[y/N]" first (faster check)
    let has_yn = lines.iter().any(|line| {
        patterns.tool_approval.iter().any(|re| {
            re.as_str().contains("Y/n") && re.is_match(line)
                || re.as_str().contains("y/N") && re.is_match(line)
        })
    });

    if !has_yn {
        // Fallback: check all tool approval patterns
        let found = lines.iter().any(|line| {
            patterns.tool_approval.iter().any(|re| re.is_match(line))
        });
        if !found {
            return None;
        }
    }

    // Try to extract the tool name from "Allow <ToolName>"
    for line in lines {
        for re in &patterns.tool_approval {
            if let Some(captures) = re.captures(line) {
                if let Some(tool_match) = captures.get(1) {
                    return Some(tool_match.as_str().to_string());
                }
            }
        }
    }

    Some("Unknown".to_string())
}

/// Check if the bottom lines indicate an input prompt.
fn detect_input_prompt(patterns: &DetectionPatterns, lines: &[String]) -> bool {
    // Check the very last non-empty line
    let last_line = lines.iter().rev().find(|l| !l.trim().is_empty());
    if let Some(line) = last_line {
        for re in &patterns.input_prompt {
            if re.is_match(line.trim()) {
                return true;
            }
        }
    }
    false
}

/// Check if the bottom lines contain a question.
fn detect_question(lines: &[String]) -> bool {
    // Look for a line ending with "?" that isn't part of a prompt
    for line in lines.iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.ends_with('?') && trimmed.len() > 1 {
            return true;
        }
        break; // Only check the last non-empty line
    }
    false
}

/// Extract an error hint from the screen content.
fn extract_error_hint(lines: &[String]) -> Option<String> {
    for line in lines.iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Return the last non-empty line as the error hint (truncated)
        let hint = if trimmed.len() > 80 {
            format!("{}...", &trimmed[..77])
        } else {
            trimmed.to_string()
        };
        return Some(hint);
    }
    None
}

/// Extract the bottom N lines from a vt100 screen as strings.
/// Strips trailing whitespace from each line.
pub fn extract_screen_lines(screen: &vt100::Screen, n: usize) -> Vec<String> {
    let rows = screen.size().0 as usize;
    let start_row = rows.saturating_sub(n);
    let mut lines = Vec::with_capacity(n);

    for row in start_row..rows {
        let line = screen.rows_formatted(row as u16, row as u16 + 1);
        // Convert to string, strip ANSI codes for pattern matching
        let plain = strip_ansi_escapes(
            &String::from_utf8_lossy(&line.into_iter().flatten().collect::<Vec<_>>())
        );
        lines.push(plain.trim_end().to_string());
    }

    lines
}

/// Simple ANSI escape code stripper for pattern matching.
/// Uses a regex to remove escape sequences.
fn strip_ansi_escapes(s: &str) -> String {
    lazy_static::lazy_static! {
        static ref ANSI_RE: Regex = Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
    }
    ANSI_RE.replace_all(s, "").to_string()
}
```

> **Note**: Add `lazy_static = "1"` to `Cargo.toml` for the ANSI stripping regex. Alternatively, use `std::sync::OnceLock` (available in Rust 1.75+) to avoid the dependency.

### Screen Line Extraction — Alternative Approach

The `vt100` crate provides `screen.contents()` which returns the full screen text. An alternative to row-by-row extraction:

```rust
pub fn extract_screen_lines_simple(screen: &vt100::Screen, n: usize) -> Vec<String> {
    let contents = screen.contents();
    let all_lines: Vec<&str> = contents.lines().collect();
    let start = all_lines.len().saturating_sub(n);
    all_lines[start..].iter().map(|s| s.to_string()).collect()
}
```

This is simpler and avoids ANSI stripping (since `contents()` returns plain text). **Recommended approach for v0.1**.

### Hysteresis / Anti-Flapping

The detector includes hysteresis to prevent rapid state flapping:

1. **Same-state returns current**: If the detection result matches the current state, return the existing state object (preserving its `since` timestamp).
2. **Debounce for pattern detection**: Tool approval detection requires the pattern to be present for at least 2 consecutive checks (500ms) before transitioning. This is implemented by the caller (AgentHandle) keeping a `pending_state: Option<(AgentState, u8)>` counter.
3. **Conservative defaults**: When uncertain, prefer "Running" over "WaitingForInput" — a false "waiting" is worse than a false "running" because it might cause the user to switch to an agent that doesn't actually need input.

### Debounce Implementation (in AgentHandle)

```rust
/// In AgentHandle (Feature 06):
struct DetectionDebounce {
    /// The state that was detected last tick.
    pending: Option<AgentState>,
    /// Number of consecutive ticks with this detection result.
    count: u8,
    /// Number of ticks required before transitioning.
    threshold: u8,
}

impl DetectionDebounce {
    fn new() -> Self {
        Self {
            pending: None,
            count: 0,
            threshold: 2, // 2 ticks = 500ms at 250ms interval
        }
    }

    /// Process a detection result. Returns the state to transition to,
    /// or None if still debouncing.
    fn process(&mut self, detected: AgentState, current: &AgentState) -> Option<AgentState> {
        // Process exit is always immediate (no debounce)
        if detected.is_terminal() {
            return Some(detected);
        }

        // If same as current state, no transition needed
        if &detected == current {
            self.pending = None;
            self.count = 0;
            return None;
        }

        // Check if this matches the pending detection
        match &self.pending {
            Some(pending) if pending == &detected => {
                self.count += 1;
                if self.count >= self.threshold {
                    self.pending = None;
                    self.count = 0;
                    Some(detected)
                } else {
                    None
                }
            }
            _ => {
                // New detection — start debounce
                self.pending = Some(detected);
                self.count = 1;
                None
            }
        }
    }
}
```

## Implementation Steps

1. **Implement `src/agent/state.rs`**
   - `AgentState` enum with all variants and methods.
   - `PromptType` enum with display methods.

2. **Implement `src/agent/detector.rs`**
   - `DetectionPatterns` struct with regex compilation from config.
   - `DetectionSignals` struct.
   - `detect_state()` function with priority cascade.
   - Helper functions: `detect_tool_approval()`, `detect_input_prompt()`, `detect_question()`, `extract_error_hint()`.
   - `extract_screen_lines()` or `extract_screen_lines_simple()`.
   - ANSI stripping helper.

3. **Add debounce logic** (can be in `detector.rs` or `handle.rs`)
   - `DetectionDebounce` struct.
   - `process()` method with configurable threshold.

4. **Update `src/agent/mod.rs`**
   - Re-export `AgentState`, `PromptType`, `DetectionPatterns`, `detect_state`.

5. **Write comprehensive tests**.

## Error Handling

| Error | Handling |
|---|---|
| Regex compilation failure | Caught during `DetectionPatterns::from_config()`. Invalid user patterns are skipped with a warning log. Built-in patterns are hardcoded and always valid. |
| Screen extraction out of bounds | `saturating_sub()` prevents underflow. Empty screen returns empty lines. |
| `try_wait()` fails | Return `None` for process exit — detection falls through to pattern/timing checks. |
| Unexpected screen content | Returns `Running` as the safe default. |

## Testing Strategy

### Unit Tests — `AgentState`

```rust
#[test]
fn test_state_symbols() {
    assert_eq!(AgentState::Spawning.symbol(), "○");
    assert_eq!(AgentState::Running { since: Utc::now() }.symbol(), "●");
    assert_eq!(
        AgentState::WaitingForInput {
            prompt_type: PromptType::Question,
            since: Utc::now(),
        }.symbol(),
        "?"
    );
}

#[test]
fn test_terminal_states() {
    assert!(!AgentState::Running { since: Utc::now() }.is_terminal());
    assert!(AgentState::Completed { at: Utc::now(), exit_code: 0 }.is_terminal());
    assert!(AgentState::Errored { at: Utc::now(), exit_code: Some(1), error_hint: None }.is_terminal());
}
```

### Unit Tests — `detect_state()`

```rust
#[test]
fn test_detect_process_exit_success() {
    let patterns = DetectionPatterns::from_config(&DetectionConfig::default());
    let signals = DetectionSignals {
        process_exited: Some(ProcessExit { exit_code: Some(0), signal: false }),
        screen_lines: vec![],
        seconds_since_output: 0.0,
        current_state: &AgentState::Running { since: Utc::now() },
        idle_timeout_secs: 3,
    };
    let state = detect_state(&patterns, &signals);
    assert!(matches!(state, AgentState::Completed { exit_code: 0, .. }));
}

#[test]
fn test_detect_process_exit_error() {
    let patterns = DetectionPatterns::from_config(&DetectionConfig::default());
    let signals = DetectionSignals {
        process_exited: Some(ProcessExit { exit_code: Some(1), signal: false }),
        screen_lines: vec!["Error: API connection failed".into()],
        seconds_since_output: 0.0,
        current_state: &AgentState::Running { since: Utc::now() },
        idle_timeout_secs: 3,
    };
    let state = detect_state(&patterns, &signals);
    match state {
        AgentState::Errored { exit_code, error_hint, .. } => {
            assert_eq!(exit_code, Some(1));
            assert!(error_hint.unwrap().contains("API connection failed"));
        }
        _ => panic!("Expected Errored state"),
    }
}

#[test]
fn test_detect_tool_approval() {
    let patterns = DetectionPatterns::from_config(&DetectionConfig::default());
    let signals = DetectionSignals {
        process_exited: None,
        screen_lines: vec![
            "I'll edit the file now.".into(),
            "".into(),
            "Allow Edit to src/main.rs? [Y/n]".into(),
        ],
        seconds_since_output: 1.0,
        current_state: &AgentState::Running { since: Utc::now() },
        idle_timeout_secs: 3,
    };
    let state = detect_state(&patterns, &signals);
    match state {
        AgentState::WaitingForInput { prompt_type: PromptType::ToolApproval { tool_name }, .. } => {
            assert_eq!(tool_name, "Edit");
        }
        _ => panic!("Expected WaitingForInput(ToolApproval)"),
    }
}

#[test]
fn test_detect_idle() {
    let patterns = DetectionPatterns::from_config(&DetectionConfig::default());
    let signals = DetectionSignals {
        process_exited: None,
        screen_lines: vec!["some old output".into()],
        seconds_since_output: 5.0,
        current_state: &AgentState::Running { since: Utc::now() },
        idle_timeout_secs: 3,
    };
    let state = detect_state(&patterns, &signals);
    assert!(matches!(state, AgentState::Idle { .. }));
}

#[test]
fn test_detect_running() {
    let patterns = DetectionPatterns::from_config(&DetectionConfig::default());
    let signals = DetectionSignals {
        process_exited: None,
        screen_lines: vec!["working on things...".into()],
        seconds_since_output: 0.5,
        current_state: &AgentState::Running { since: Utc::now() },
        idle_timeout_secs: 3,
    };
    let state = detect_state(&patterns, &signals);
    assert!(matches!(state, AgentState::Running { .. }));
}

#[test]
fn test_hysteresis_preserves_timestamp() {
    let original_since = Utc::now() - chrono::Duration::seconds(10);
    let patterns = DetectionPatterns::from_config(&DetectionConfig::default());
    let current = AgentState::Running { since: original_since };
    let signals = DetectionSignals {
        process_exited: None,
        screen_lines: vec![],
        seconds_since_output: 0.5,
        current_state: &current,
        idle_timeout_secs: 3,
    };
    let state = detect_state(&patterns, &signals);
    match state {
        AgentState::Running { since } => assert_eq!(since, original_since),
        _ => panic!("Expected Running with original timestamp"),
    }
}
```

### Unit Tests — `DetectionDebounce`

```rust
#[test]
fn test_debounce_requires_two_ticks() {
    let mut debounce = DetectionDebounce::new();
    let current = AgentState::Running { since: Utc::now() };
    let waiting = AgentState::WaitingForInput {
        prompt_type: PromptType::InputPrompt,
        since: Utc::now(),
    };

    // First tick — should not transition yet
    assert!(debounce.process(waiting.clone(), &current).is_none());

    // Second tick — should transition now
    assert!(debounce.process(waiting.clone(), &current).is_some());
}

#[test]
fn test_debounce_resets_on_different_state() {
    let mut debounce = DetectionDebounce::new();
    let current = AgentState::Running { since: Utc::now() };

    // First tick: detected WaitingForInput
    debounce.process(
        AgentState::WaitingForInput {
            prompt_type: PromptType::InputPrompt,
            since: Utc::now(),
        },
        &current,
    );

    // Second tick: detected Idle instead — debounce resets
    let result = debounce.process(AgentState::Idle { since: Utc::now() }, &current);
    assert!(result.is_none()); // Reset, not enough ticks for Idle
}

#[test]
fn test_debounce_terminal_state_immediate() {
    let mut debounce = DetectionDebounce::new();
    let current = AgentState::Running { since: Utc::now() };
    let completed = AgentState::Completed { at: Utc::now(), exit_code: 0 };

    // Terminal states bypass debounce
    let result = debounce.process(completed, &current);
    assert!(result.is_some());
}
```

## Acceptance Criteria

- [ ] `AgentState` has 6 variants: Spawning, Running, WaitingForInput, Idle, Completed, Errored.
- [ ] `PromptType` has 4 variants: ToolApproval, Question, InputPrompt, Unknown.
- [ ] State detection correctly prioritizes: process exit > screen patterns > timing.
- [ ] Tool approval detection extracts the tool name from "Allow <Tool>" patterns.
- [ ] Input prompt detection recognizes Claude Code's ">" prompt.
- [ ] Idle detection triggers after `idle_timeout_secs` of no output.
- [ ] Hysteresis prevents state flapping (preserves timestamps, debounce logic).
- [ ] User-configured patterns are merged with built-in defaults.
- [ ] Invalid user regex patterns are skipped with warnings (not crashes).
- [ ] Process exit with code 0 → Completed; non-zero → Errored with error hint.
- [ ] All unit tests pass.
