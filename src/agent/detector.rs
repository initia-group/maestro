//! State detection engine.
//!
//! Uses compiled regex patterns to detect agent state from PTY screen content.
//! Implements debounce logic to prevent state flapping.
//! See Feature 05 (Agent State Machine & Detection) for the full spec.

use crate::agent::state::{AgentState, PromptType};
use crate::config::settings::DetectionConfig;
use chrono::Utc;
use regex::Regex;
use tracing::warn;

/// Compiled detection patterns for efficient repeated matching.
pub struct DetectionPatterns {
    /// Built-in tool approval patterns + user-configured additions.
    tool_approval: Vec<Regex>,
    /// Built-in error patterns + user-configured additions.
    /// Used for error_hint extraction on process exit; not yet used for
    /// auto-transitioning to Errored from screen content in v0.1.
    #[allow(dead_code)]
    error: Vec<Regex>,
    /// Built-in input prompt patterns + user-configured additions.
    input_prompt: Vec<Regex>,
    /// Built-in AskUserQuestion patterns + user-configured additions.
    ask_user_question: Vec<Regex>,
    /// Number of bottom screen lines to scan.
    scan_lines: usize,
}

impl DetectionPatterns {
    /// Create from configuration, compiling all regex patterns.
    ///
    /// Invalid user-provided patterns are skipped with a warning log.
    /// Built-in patterns are hardcoded and always valid.
    pub fn from_config(config: &DetectionConfig) -> Self {
        let mut tool_approval = vec![
            Regex::new(r"Allow\s+(\w+)").unwrap(),
            Regex::new(r"\[Y/n\]").unwrap(),
            Regex::new(r"\[y/N\]").unwrap(),
        ];
        for pattern in &config.tool_approval_patterns {
            match Regex::new(pattern) {
                Ok(re) => tool_approval.push(re),
                Err(e) => warn!("Invalid tool_approval regex {pattern:?}: {e}"),
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
            match Regex::new(pattern) {
                Ok(re) => error.push(re),
                Err(e) => warn!("Invalid error regex {pattern:?}: {e}"),
            }
        }

        let mut input_prompt = vec![
            Regex::new(r"^>\s*$").unwrap(),
            Regex::new(r"^\$\s*$").unwrap(),
        ];
        for pattern in &config.input_prompt_patterns {
            match Regex::new(pattern) {
                Ok(re) => input_prompt.push(re),
                Err(e) => warn!("Invalid input_prompt regex {pattern:?}: {e}"),
            }
        }

        // AskUserQuestion patterns: detect Claude Code's interactive numbered-option prompts.
        let mut ask_user_question = vec![
            // Numbered option line (with optional selection cursor ❯)
            Regex::new(r"^\s*❯?\s*\d+[\.:]\s+.+").unwrap(),
            // The auto-appended "Other" / "Type something else" option
            Regex::new(r"(?i)type something else|other.*free.text").unwrap(),
        ];
        for pattern in &config.ask_user_question_patterns {
            match Regex::new(pattern) {
                Ok(re) => ask_user_question.push(re),
                Err(e) => warn!("Invalid ask_user_question regex {pattern:?}: {e}"),
            }
        }

        Self {
            tool_approval,
            error,
            input_prompt,
            ask_user_question,
            scan_lines: config.scan_lines,
        }
    }

    /// Returns the configured number of screen lines to scan.
    pub fn scan_lines(&self) -> usize {
        self.scan_lines
    }
}

/// Signals available for state detection, in priority order.
pub struct DetectionSignals<'a> {
    /// Has the child process exited? If so, what was the exit status?
    pub process_exited: Option<ProcessExit>,
    /// The vt100 screen content (bottom N lines, plain text).
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
/// 1. Process exit -> Completed or Errored (authoritative)
/// 2. Screen content patterns -> WaitingForInput (heuristic)
/// 3. Output timing -> Idle or Running (fallback)
pub fn detect_state(patterns: &DetectionPatterns, signals: &DetectionSignals) -> AgentState {
    // --- Priority 1: Process exit (most reliable) ---
    if let Some(exit) = &signals.process_exited {
        let at = Utc::now();
        return match exit.exit_code {
            Some(0) => AgentState::Completed {
                at,
                exit_code: Some(0),
            },
            Some(code) => AgentState::Errored {
                at,
                error_hint: extract_error_hint(&signals.screen_lines)
                    .or_else(|| Some(format!("exit code {code}"))),
            },
            None => {
                if exit.signal {
                    AgentState::Errored {
                        at,
                        error_hint: Some("Killed by signal".to_string()),
                    }
                } else {
                    AgentState::Completed {
                        at,
                        exit_code: None,
                    }
                }
            }
        };
    }

    // Don't re-detect if already in a terminal state
    if signals.current_state.is_terminal() {
        return signals.current_state.clone();
    }

    // --- Priority 2: Screen content patterns ---
    let bottom_lines = &signals.screen_lines;

    // Check for tool approval prompts
    if let Some(tool_name) = detect_tool_approval(patterns, bottom_lines) {
        // Hysteresis: avoid resetting `since` timestamp if already in same state
        if let AgentState::WaitingForInput {
            prompt_type:
                PromptType::ToolApproval {
                    tool_name: existing,
                },
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

    // Check for AskUserQuestion interactive prompts (numbered options)
    if let Some(question) = detect_ask_user_question(patterns, bottom_lines) {
        if let AgentState::WaitingForInput {
            prompt_type: PromptType::AskUserQuestion { .. },
            ..
        } = signals.current_state
        {
            return signals.current_state.clone();
        }
        return AgentState::WaitingForInput {
            prompt_type: PromptType::AskUserQuestion { question },
            since: Utc::now(),
        };
    }

    // Check for input prompts
    if detect_input_prompt(patterns, bottom_lines) {
        if matches!(
            signals.current_state,
            AgentState::WaitingForInput {
                prompt_type: PromptType::InputPrompt,
                ..
            }
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
            AgentState::WaitingForInput {
                prompt_type: PromptType::Question,
                ..
            }
        ) {
            return signals.current_state.clone();
        }
        return AgentState::WaitingForInput {
            prompt_type: PromptType::Question,
            since: Utc::now(),
        };
    }

    // Note: Error patterns from screen content do NOT auto-transition to Errored
    // in v0.1. They are used only for error_hint extraction on process exit.

    // --- Priority 3: Output timing ---
    if signals.seconds_since_output >= signals.idle_timeout_secs as f64 {
        if matches!(signals.current_state, AgentState::Idle { .. }) {
            return signals.current_state.clone(); // hysteresis
        }
        return AgentState::Idle { since: Utc::now() };
    }

    // Active output -> Running
    if matches!(signals.current_state, AgentState::Running { .. }) {
        return signals.current_state.clone(); // hysteresis
    }
    AgentState::Running { since: Utc::now() }
}

/// Extract the tool name from tool approval patterns.
fn detect_tool_approval(patterns: &DetectionPatterns, lines: &[String]) -> Option<String> {
    // Quick check: do any lines match any tool_approval pattern?
    let has_match = lines
        .iter()
        .any(|line| patterns.tool_approval.iter().any(|re| re.is_match(line)));

    if !has_match {
        return None;
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

/// Check if the bottom lines show a Claude Code AskUserQuestion interactive prompt.
///
/// Looks for the pattern of numbered options (e.g., "❯ 1. Summary - Brief overview")
/// with at least 2 option lines, and optionally extracts the question text from
/// a preceding line ending with `?`.
fn detect_ask_user_question(patterns: &DetectionPatterns, lines: &[String]) -> Option<String> {
    let numbered_option_re = &patterns.ask_user_question[0]; // ❯?\s*\d+[.:]\s+.+

    // Count how many lines match the numbered option pattern
    let option_count = lines
        .iter()
        .filter(|l| numbered_option_re.is_match(l.trim()))
        .count();

    // Need at least 2 numbered option lines to consider this an AskUserQuestion
    if option_count < 2 {
        return None;
    }

    // Find the first numbered option line index
    let first_option_idx = lines
        .iter()
        .position(|l| numbered_option_re.is_match(l.trim()));

    // Try to extract the question text from lines above the options
    if let Some(opt_idx) = first_option_idx {
        for i in (0..opt_idx).rev() {
            let trimmed = lines[i].trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.ends_with('?') {
                // Found a question line — truncate for display
                let question = if trimmed.len() > 80 {
                    format!("{}…", &trimmed[..79])
                } else {
                    trimmed.to_string()
                };
                return Some(question);
            }
            // Take the first non-empty line above options as context
            let question = if trimmed.len() > 80 {
                format!("{}…", &trimmed[..79])
            } else {
                trimmed.to_string()
            };
            return Some(question);
        }
    }

    // No question text found above, but the options pattern is clear
    Some("Interactive prompt".to_string())
}

/// Check if the bottom lines contain a question.
fn detect_question(lines: &[String]) -> bool {
    for line in lines.iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // A line ending with '?' that isn't trivially short
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
        // Return the last non-empty line as the error hint (truncated to 80 chars)
        let hint = if trimmed.len() > 80 {
            format!("{}...", &trimmed[..77])
        } else {
            trimmed.to_string()
        };
        return Some(hint);
    }
    None
}

/// Extract the bottom N non-empty lines from a vt100 screen as plain-text strings.
///
/// Scans the screen from bottom to top to find the last row with content,
/// then returns up to N lines ending at that row. Uses cell-by-cell access
/// instead of `screen.contents()` to avoid building a full screen string.
/// This is much cheaper for state detection, which only needs a few bottom
/// lines and runs every 250ms across all agents.
pub fn extract_screen_lines(screen: &vt100::Screen, n: usize) -> Vec<String> {
    let (rows, cols) = screen.size();
    let rows = rows as usize;
    let cols = cols as usize;

    // Find the last non-empty row by scanning from bottom
    let mut last_content_row: Option<usize> = None;
    for row in (0..rows).rev() {
        for col in 0..cols {
            if let Some(cell) = screen.cell(row as u16, col as u16) {
                let ch = cell.contents();
                if !ch.is_empty() && ch != " " {
                    last_content_row = Some(row);
                    break;
                }
            }
        }
        if last_content_row.is_some() {
            break;
        }
    }

    let last_row = match last_content_row {
        Some(r) => r,
        None => return Vec::new(), // Screen is entirely empty
    };

    // Read the bottom N rows up to and including last_content_row
    let start_row = (last_row + 1).saturating_sub(n);
    let mut lines = Vec::with_capacity(n);

    for row in start_row..=last_row {
        let mut line = String::with_capacity(cols);
        let mut last_non_space = 0;
        for col in 0..cols {
            if let Some(cell) = screen.cell(row as u16, col as u16) {
                let ch = cell.contents();
                if ch.is_empty() {
                    line.push(' ');
                } else {
                    line.push_str(ch);
                }
                if !ch.is_empty() && ch != " " {
                    last_non_space = line.len();
                }
            } else {
                line.push(' ');
            }
        }
        line.truncate(last_non_space);
        lines.push(line);
    }
    lines
}

/// Anti-flapping debounce for state transitions.
///
/// Requires a detected state to persist for `threshold` consecutive ticks
/// before allowing a transition. Terminal states (Completed/Errored) bypass
/// debounce entirely.
pub struct DetectionDebounce {
    /// The state that was detected last tick.
    pending: Option<AgentState>,
    /// Number of consecutive ticks with this detection result.
    count: u8,
    /// Number of ticks required before transitioning.
    threshold: u8,
}

impl DetectionDebounce {
    /// Create a new debouncer with the default threshold of 2 ticks.
    pub fn new() -> Self {
        Self {
            pending: None,
            count: 0,
            threshold: 2,
        }
    }

    /// Create a debouncer with a custom tick threshold.
    pub fn with_threshold(threshold: u8) -> Self {
        Self {
            pending: None,
            count: 0,
            threshold,
        }
    }

    /// Process a detection result. Returns the state to transition to,
    /// or `None` if still debouncing (no transition yet).
    pub fn process(&mut self, detected: AgentState, current: &AgentState) -> Option<AgentState> {
        // Terminal states bypass debounce — always immediate
        if detected.is_terminal() {
            self.pending = None;
            self.count = 0;
            return Some(detected);
        }

        // If same as current state, no transition needed
        if detected.same_variant(current) {
            self.pending = None;
            self.count = 0;
            return None;
        }

        // Check if this matches the pending detection
        match &self.pending {
            Some(pending) if pending.same_variant(&detected) => {
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

impl Default for DetectionDebounce {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::DetectionConfig;

    fn default_patterns() -> DetectionPatterns {
        DetectionPatterns::from_config(&DetectionConfig::default())
    }

    // --- DetectionPatterns tests ---

    #[test]
    fn test_patterns_from_default_config() {
        let patterns = default_patterns();
        assert!(!patterns.tool_approval.is_empty());
        assert!(!patterns.error.is_empty());
        assert!(!patterns.input_prompt.is_empty());
        assert!(!patterns.ask_user_question.is_empty());
        assert_eq!(patterns.scan_lines, 10);
    }

    #[test]
    fn test_patterns_with_user_additions() {
        let config = DetectionConfig {
            tool_approval_patterns: vec!["approve\\?".into()],
            error_patterns: vec!["FATAL".into()],
            input_prompt_patterns: vec![">>>".into()],
            ask_user_question_patterns: vec!["custom_ask".into()],
            scan_lines: 10,
        };
        let patterns = DetectionPatterns::from_config(&config);
        // Built-in (3) + user (1) = 4
        assert_eq!(patterns.tool_approval.len(), 4);
        // Built-in (6) + user (1) = 7
        assert_eq!(patterns.error.len(), 7);
        // Built-in (2) + user (1) = 3
        assert_eq!(patterns.input_prompt.len(), 3);
        // Built-in (2) + user (1) = 3
        assert_eq!(patterns.ask_user_question.len(), 3);
        assert_eq!(patterns.scan_lines, 10);
    }

    #[test]
    fn test_invalid_user_regex_skipped() {
        let config = DetectionConfig {
            tool_approval_patterns: vec!["[invalid".into()],
            error_patterns: vec![],
            input_prompt_patterns: vec![],
            ask_user_question_patterns: vec![],
            scan_lines: 5,
        };
        let patterns = DetectionPatterns::from_config(&config);
        // Only built-in patterns (invalid one skipped)
        assert_eq!(patterns.tool_approval.len(), 3);
    }

    // --- detect_state tests ---

    #[test]
    fn test_detect_process_exit_success() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: Some(ProcessExit {
                exit_code: Some(0),
                signal: false,
            }),
            screen_lines: vec![],
            seconds_since_output: 0.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        assert!(matches!(
            state,
            AgentState::Completed {
                exit_code: Some(0),
                ..
            }
        ));
    }

    #[test]
    fn test_detect_process_exit_error() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: Some(ProcessExit {
                exit_code: Some(1),
                signal: false,
            }),
            screen_lines: vec!["Error: API connection failed".into()],
            seconds_since_output: 0.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        match state {
            AgentState::Errored { error_hint, .. } => {
                assert!(error_hint.unwrap().contains("API connection failed"));
            }
            _ => panic!("Expected Errored state"),
        }
    }

    #[test]
    fn test_detect_process_exit_signal() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: Some(ProcessExit {
                exit_code: None,
                signal: true,
            }),
            screen_lines: vec![],
            seconds_since_output: 0.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        match state {
            AgentState::Errored { error_hint, .. } => {
                assert_eq!(error_hint.unwrap(), "Killed by signal");
            }
            _ => panic!("Expected Errored state"),
        }
    }

    #[test]
    fn test_detect_process_exit_none_no_signal() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: Some(ProcessExit {
                exit_code: None,
                signal: false,
            }),
            screen_lines: vec![],
            seconds_since_output: 0.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        assert!(matches!(state, AgentState::Completed { .. }));
    }

    #[test]
    fn test_detect_tool_approval() {
        let patterns = default_patterns();
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
            AgentState::WaitingForInput {
                prompt_type: PromptType::ToolApproval { tool_name },
                ..
            } => {
                assert_eq!(tool_name, "Edit");
            }
            _ => panic!("Expected WaitingForInput(ToolApproval), got {state:?}"),
        }
    }

    #[test]
    fn test_detect_tool_approval_yn_lowercase() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec!["Proceed? [y/N]".into()],
            seconds_since_output: 1.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        // Should detect as waiting (the [y/N] pattern matches)
        assert!(matches!(state, AgentState::WaitingForInput { .. }));
    }

    #[test]
    fn test_detect_input_prompt() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec!["some output".into(), ">".into()],
            seconds_since_output: 1.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        assert!(matches!(
            state,
            AgentState::WaitingForInput {
                prompt_type: PromptType::InputPrompt,
                ..
            }
        ));
    }

    #[test]
    fn test_detect_question() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec!["Would you like to proceed with this change?".into()],
            seconds_since_output: 1.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        assert!(matches!(
            state,
            AgentState::WaitingForInput {
                prompt_type: PromptType::Question,
                ..
            }
        ));
    }

    #[test]
    fn test_detect_idle() {
        let patterns = default_patterns();
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
        let patterns = default_patterns();
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
    fn test_detect_running_from_spawning() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec!["first output".into()],
            seconds_since_output: 0.1,
            current_state: &AgentState::Spawning { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        assert!(matches!(state, AgentState::Running { .. }));
    }

    #[test]
    fn test_hysteresis_preserves_timestamp() {
        let original_since = Utc::now() - chrono::Duration::seconds(10);
        let patterns = default_patterns();
        let current = AgentState::Running {
            since: original_since,
        };
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

    #[test]
    fn test_hysteresis_idle_preserves_timestamp() {
        let original_since = Utc::now() - chrono::Duration::seconds(30);
        let patterns = default_patterns();
        let current = AgentState::Idle {
            since: original_since,
        };
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec!["old".into()],
            seconds_since_output: 10.0,
            current_state: &current,
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        match state {
            AgentState::Idle { since } => assert_eq!(since, original_since),
            _ => panic!("Expected Idle with original timestamp"),
        }
    }

    #[test]
    fn test_hysteresis_waiting_preserves_timestamp() {
        let original_since = Utc::now() - chrono::Duration::seconds(5);
        let patterns = default_patterns();
        let current = AgentState::WaitingForInput {
            since: original_since,
            prompt_type: PromptType::ToolApproval {
                tool_name: "Edit".into(),
            },
        };
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec!["Allow Edit to foo.rs? [Y/n]".into()],
            seconds_since_output: 1.0,
            current_state: &current,
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        match &state {
            AgentState::WaitingForInput { since, .. } => assert_eq!(*since, original_since),
            _ => panic!("Expected WaitingForInput with original timestamp"),
        }
    }

    #[test]
    fn test_terminal_state_not_overridden() {
        let patterns = default_patterns();
        let current = AgentState::Completed {
            at: Utc::now(),
            exit_code: Some(0),
        };
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec!["some output".into()],
            seconds_since_output: 0.5,
            current_state: &current,
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        assert!(matches!(state, AgentState::Completed { .. }));
    }

    #[test]
    fn test_process_exit_overrides_screen_patterns() {
        let patterns = default_patterns();
        // Even though screen shows a tool approval prompt, process exit takes priority
        let signals = DetectionSignals {
            process_exited: Some(ProcessExit {
                exit_code: Some(0),
                signal: false,
            }),
            screen_lines: vec!["Allow Edit to foo.rs? [Y/n]".into()],
            seconds_since_output: 0.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        assert!(matches!(state, AgentState::Completed { .. }));
    }

    // --- extract_error_hint tests ---

    #[test]
    fn test_extract_error_hint_last_line() {
        let lines = vec![
            "some output".into(),
            "Error: connection refused".into(),
            "".into(),
        ];
        let hint = extract_error_hint(&lines);
        assert_eq!(hint.unwrap(), "Error: connection refused");
    }

    #[test]
    fn test_extract_error_hint_truncated() {
        let long_line = "x".repeat(100);
        let lines = vec![long_line];
        let hint = extract_error_hint(&lines).unwrap();
        assert_eq!(hint.len(), 80); // 77 chars + "..."
        assert!(hint.ends_with("..."));
    }

    #[test]
    fn test_extract_error_hint_empty() {
        let lines: Vec<String> = vec!["".into(), "  ".into()];
        assert!(extract_error_hint(&lines).is_none());
    }

    // --- detect_question tests ---

    #[test]
    fn test_question_detection() {
        assert!(detect_question(&["Are you sure?".into()]));
        assert!(detect_question(&["".into(), "Continue?".into(), "".into()]));
        // Single "?" is not a question (too short)
        assert!(!detect_question(&["?".into()]));
        // Empty lines only
        assert!(!detect_question(&["".into()]));
    }

    // --- detect_ask_user_question tests ---

    #[test]
    fn test_ask_user_question_detection() {
        let patterns = default_patterns();
        let lines = vec![
            "Format: How should I format the output?".into(),
            "❯ 1. Summary - Brief overview".into(),
            "  2. Detailed - Full explanation".into(),
            "  3. Type something else...".into(),
        ];
        let result = detect_ask_user_question(&patterns, &lines);
        assert!(result.is_some());
        assert!(result.unwrap().contains("How should I format"));
    }

    #[test]
    fn test_ask_user_question_without_cursor() {
        let patterns = default_patterns();
        let lines = vec![
            "Which approach do you prefer?".into(),
            "  1. Option A - First approach".into(),
            "  2. Option B - Second approach".into(),
        ];
        let result = detect_ask_user_question(&patterns, &lines);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Which approach"));
    }

    #[test]
    fn test_ask_user_question_too_few_options() {
        let patterns = default_patterns();
        // Only 1 option line — not enough for AskUserQuestion
        let lines = vec!["Some question?".into(), "  1. Only option".into()];
        let result = detect_ask_user_question(&patterns, &lines);
        assert!(result.is_none());
    }

    #[test]
    fn test_ask_user_question_in_detect_state() {
        let patterns = default_patterns();
        let signals = DetectionSignals {
            process_exited: None,
            screen_lines: vec![
                "How should we proceed?".into(),
                "❯ 1. Refactor - Clean up the code".into(),
                "  2. Leave as-is - Skip changes".into(),
                "  3. Type something else...".into(),
            ],
            seconds_since_output: 1.0,
            current_state: &AgentState::Running { since: Utc::now() },
            idle_timeout_secs: 3,
        };
        let state = detect_state(&patterns, &signals);
        match state {
            AgentState::WaitingForInput {
                prompt_type: PromptType::AskUserQuestion { question },
                ..
            } => {
                assert!(question.contains("How should we proceed"));
            }
            _ => panic!("Expected WaitingForInput(AskUserQuestion), got {state:?}"),
        }
    }

    // --- detect_input_prompt tests ---

    #[test]
    fn test_input_prompt_detection() {
        let patterns = default_patterns();
        assert!(detect_input_prompt(&patterns, &[">".into()]));
        assert!(detect_input_prompt(&patterns, &["> ".into()]));
        assert!(detect_input_prompt(&patterns, &["$".into()]));
        assert!(detect_input_prompt(
            &patterns,
            &["some output".into(), "$ ".into()]
        ));
        // Not a prompt
        assert!(!detect_input_prompt(
            &patterns,
            &["working on code...".into()]
        ));
    }

    // --- extract_screen_lines tests ---

    #[test]
    fn test_extract_screen_lines_empty_screen() {
        let parser = vt100::Parser::new(24, 80, 0);
        let lines = extract_screen_lines(parser.screen(), 5);
        // Empty screen with contents() returns empty string, so no lines
        assert!(lines.is_empty());
    }

    #[test]
    fn test_extract_screen_lines_with_content() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"Hello, world!\r\nSecond line\r\nThird line");
        let lines = extract_screen_lines(parser.screen(), 3);
        // We should get the bottom 3 lines from the content
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_extract_screen_lines_content_fewer_than_requested() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"One line");
        let lines = extract_screen_lines(parser.screen(), 100);
        // Only 1 line of content exists
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "One line");
    }

    #[test]
    fn test_extract_screen_lines_returns_bottom_n() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"Line 1\r\nLine 2\r\nLine 3\r\nLine 4\r\nLine 5");
        let lines = extract_screen_lines(parser.screen(), 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Line 4");
        assert_eq!(lines[1], "Line 5");
    }

    // --- DetectionDebounce tests ---

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
        assert!(debounce.process(waiting, &current).is_some());
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
        let completed = AgentState::Completed {
            at: Utc::now(),
            exit_code: Some(0),
        };

        // Terminal states bypass debounce
        let result = debounce.process(completed, &current);
        assert!(result.is_some());
    }

    #[test]
    fn test_debounce_same_as_current_no_transition() {
        let mut debounce = DetectionDebounce::new();
        let current = AgentState::Running { since: Utc::now() };

        // Detecting same state as current — no transition needed
        let result = debounce.process(AgentState::Running { since: Utc::now() }, &current);
        assert!(result.is_none());
    }

    #[test]
    fn test_debounce_custom_threshold() {
        let mut debounce = DetectionDebounce::with_threshold(3);
        let current = AgentState::Running { since: Utc::now() };
        let idle = AgentState::Idle { since: Utc::now() };

        assert!(debounce.process(idle.clone(), &current).is_none()); // tick 1
        assert!(debounce.process(idle.clone(), &current).is_none()); // tick 2
        assert!(debounce.process(idle, &current).is_some()); // tick 3 — transition
    }

    #[test]
    fn test_debounce_default_impl() {
        let debounce = DetectionDebounce::default();
        assert_eq!(debounce.threshold, 2);
        assert_eq!(debounce.count, 0);
        assert!(debounce.pending.is_none());
    }
}
