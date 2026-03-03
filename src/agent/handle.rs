//! Agent handle — bundles PTY, parser, state, and metadata for one agent.
//!
//! Each `AgentHandle` owns all the resources for a single agent process:
//! the PTY controller, vt100 terminal parser, process handle, detection
//! debounce state, and metadata needed for restart.

use crate::agent::detector::{
    detect_state, extract_screen_lines, DetectionDebounce, DetectionPatterns, DetectionSignals,
    ProcessExit,
};
use crate::agent::scrollback::ScrollbackBuffer;
use crate::agent::state::AgentState;
use crate::agent::AgentId;
use crate::pty::PtyController;
use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use portable_pty::{Child, PtySize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// All resources and metadata for a single agent.
pub struct AgentHandle {
    /// Unique agent identifier.
    id: AgentId,

    /// Display name (e.g., "backend-refactor").
    name: String,

    /// Project this agent belongs to.
    project_name: String,

    /// Current agent state.
    state: AgentState,

    /// PTY controller for I/O.
    pty: PtyController,

    /// Virtual terminal parser — processes PTY output into a screen buffer.
    parser: vt100::Parser,

    /// Child process handle — for checking exit status.
    child: Box<dyn Child + Send + Sync>,

    /// When this agent was spawned.
    spawned_at: DateTime<Utc>,

    /// When the last PTY output was received.
    last_output_at: Option<DateTime<Utc>>,

    /// Debounce state for state detection.
    debounce: DetectionDebounce,

    /// Whether the terminal content has changed since last render.
    dirty: bool,

    /// The command that was used to spawn this agent (for restart).
    spawn_command: String,

    /// The args that were used to spawn this agent (for restart).
    spawn_args: Vec<String>,

    /// The working directory (for restart).
    spawn_cwd: PathBuf,

    /// Additional environment variables (for restart).
    spawn_env: HashMap<String, String>,

    /// Scrollback buffer for scroll offset tracking and search.
    scrollback: ScrollbackBuffer,

    /// Claude Code session ID for resuming specific conversations.
    session_id: Option<String>,

    /// Whether this agent is a retry after a stale `--resume` failure.
    /// Prevents further automatic retry on quick exit.
    resume_retry_attempted: bool,

    /// Whether this agent completed/errored and the user hasn't viewed it yet.
    /// Set to `true` on transition to a terminal state; cleared when selected.
    has_unread_result: bool,

    /// When state detection was last run for this agent.
    /// Used to throttle detection frequency for idle agents.
    last_detection_at: Instant,
}

impl AgentHandle {
    /// Create a new agent handle.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: AgentId,
        name: String,
        project_name: String,
        pty: PtyController,
        parser: vt100::Parser,
        child: Box<dyn Child + Send + Sync>,
        spawn_command: String,
        spawn_args: Vec<String>,
        spawn_cwd: PathBuf,
        spawn_env: HashMap<String, String>,
        session_id: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            project_name,
            state: AgentState::Spawning { since: Utc::now() },
            pty,
            parser,
            child,
            spawned_at: Utc::now(),
            last_output_at: None,
            debounce: DetectionDebounce::new(),
            dirty: true,
            spawn_command,
            spawn_args,
            spawn_cwd,
            spawn_env,
            scrollback: ScrollbackBuffer::new(10 * 1024 * 1024), // 10MB default
            session_id,
            resume_retry_attempted: false,
            has_unread_result: false,
            last_detection_at: Instant::now(),
        }
    }

    // ---- Accessors ----

    pub fn id(&self) -> AgentId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Rename this agent.
    pub fn set_name(&mut self, new_name: String) {
        self.name = new_name;
        self.dirty = true;
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn set_project_name(&mut self, new_name: String) {
        self.project_name = new_name;
        self.dirty = true;
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn resume_retry_attempted(&self) -> bool {
        self.resume_retry_attempted
    }

    pub fn set_resume_retry_attempted(&mut self, val: bool) {
        self.resume_retry_attempted = val;
    }

    /// Whether this agent has an unread completion/error result.
    pub fn has_unread_result(&self) -> bool {
        self.has_unread_result
    }

    /// Mark the agent's result as read (user has viewed it).
    pub fn mark_result_read(&mut self) {
        self.has_unread_result = false;
    }

    /// Whether this agent's error looks like a stale Claude session resume failure.
    ///
    /// Returns `true` if the agent errored out within `max_secs` of spawning
    /// and was attempting a `--resume`, suggesting the session ID is stale.
    pub fn is_stale_resume_failure(&self, max_secs: u64) -> bool {
        if self.resume_retry_attempted {
            return false;
        }
        if !matches!(self.state, AgentState::Errored { .. }) {
            return false;
        }
        let age = (Utc::now() - self.spawned_at).num_seconds();
        if age > max_secs as i64 {
            return false;
        }
        self.spawn_args.iter().any(|a| a == "--resume" || a == "-r")
    }

    pub fn state(&self) -> &AgentState {
        &self.state
    }

    pub fn spawned_at(&self) -> DateTime<Utc> {
        self.spawned_at
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get the vt100 screen for rendering.
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Mark as rendered (clear dirty flag).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Uptime as a human-readable string.
    pub fn uptime(&self) -> String {
        let elapsed = Utc::now() - self.spawned_at;
        let secs = elapsed.num_seconds();
        if secs < 60 {
            format!("{secs}s")
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    // ---- PTY Interaction ----

    /// Process raw bytes from the PTY output.
    /// Called when a `PtyOutput` event is received.
    pub fn process_output(&mut self, data: &[u8]) {
        self.parser.process(data);
        self.scrollback.append(data);

        // Auto-scroll to bottom when new output arrives (if already at bottom)
        if !self.scrollback.is_scrolled() {
            self.scrollback.scroll_to_bottom();
        }

        self.last_output_at = Some(Utc::now());
        self.dirty = true;
    }

    /// Send raw bytes to the agent's PTY (user input in Insert Mode).
    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        self.pty.write(data)
    }

    /// Resize the PTY and vt100 parser to new dimensions.
    pub fn resize(&mut self, size: PtySize) {
        if let Err(e) = self.pty.resize(size) {
            tracing::warn!("Failed to resize PTY for agent {}: {}", self.name, e);
        }
        self.parser.screen_mut().set_size(size.rows, size.cols);
        self.dirty = true;
    }

    // ---- Scrollback & Search ----

    /// Get the scrollback buffer (immutable).
    pub fn scrollback(&self) -> &ScrollbackBuffer {
        &self.scrollback
    }

    /// Get the scrollback buffer (mutable).
    pub fn scrollback_mut(&mut self) -> &mut ScrollbackBuffer {
        &mut self.scrollback
    }

    /// Scroll up by half a page.
    pub fn scroll_up(&mut self, page_height: usize) {
        self.scrollback.scroll_up(page_height);
        // Discover actual max scrollback: set_scrollback(usize::MAX) clamps
        // to the real scrollback length, then we read it back and reset.
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let max_scrollback = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(0);
        self.scrollback.clamp_scroll(max_scrollback);
        self.dirty = true;
    }

    /// Set the vt100 scrollback viewing offset.
    ///
    /// Shifts which rows `screen.cell()` returns so that tui-term's
    /// `PseudoTerminal` renders scrollback content directly.
    /// Call with `0` to reset to live view after rendering.
    pub fn set_scrollback_view(&mut self, offset: usize) {
        self.parser.screen_mut().set_scrollback(offset);
    }

    /// Scroll down by half a page.
    pub fn scroll_down(&mut self, page_height: usize) {
        self.scrollback.scroll_down(page_height);
        self.dirty = true;
    }

    /// Scroll up by a fixed number of lines (mouse wheel).
    pub fn mouse_scroll_up(&mut self, lines: usize) {
        self.scrollback.mouse_scroll_up(lines);
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let max_scrollback = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(0);
        self.scrollback.clamp_scroll(max_scrollback);
        self.dirty = true;
    }

    /// Scroll down by a fixed number of lines (mouse wheel).
    pub fn mouse_scroll_down(&mut self, lines: usize) {
        self.scrollback.mouse_scroll_down(lines);
        self.dirty = true;
    }

    /// Get the current scroll offset.
    pub fn scroll_offset(&self) -> usize {
        self.scrollback.scroll_offset()
    }

    /// Whether the agent is scrolled up from the bottom.
    pub fn is_scrolled(&self) -> bool {
        self.scrollback.is_scrolled()
    }

    /// Start a search with the given query.
    pub fn start_search(&mut self, query: &str) {
        self.scrollback.start_search(query);
        if let Some(search) = self.scrollback.search_mut() {
            search.search(self.parser.screen());
        }
        self.dirty = true;
    }

    /// Clear the current search.
    pub fn clear_search(&mut self) {
        self.scrollback.clear_search();
        self.dirty = true;
    }

    /// Navigate to the next search match. Returns the line of the match.
    pub fn search_next(&mut self) -> Option<usize> {
        let line = self
            .scrollback
            .search_mut()
            .and_then(|s| s.next_match().map(|m| m.line));
        if line.is_some() {
            self.dirty = true;
        }
        line
    }

    /// Navigate to the previous search match. Returns the line of the match.
    pub fn search_prev(&mut self) -> Option<usize> {
        let line = self
            .scrollback
            .search_mut()
            .and_then(|s| s.prev_match().map(|m| m.line));
        if line.is_some() {
            self.dirty = true;
        }
        line
    }

    // ---- State Detection ----

    /// Run state detection and update the internal state.
    /// Returns `Some(new_state)` if the state changed, `None` otherwise.
    ///
    /// Idle agents are throttled to check at most every 2 seconds instead
    /// of every 250ms tick, since they haven't produced output recently and
    /// are unlikely to change state without new PTY output.
    pub fn detect_and_update(
        &mut self,
        patterns: &DetectionPatterns,
        idle_timeout_secs: u64,
    ) -> Option<AgentState> {
        // Throttle detection for idle agents: skip if checked within 2 seconds.
        // New PTY output resets this via process_output() setting dirty, and
        // the visual terminal updates immediately regardless.
        if matches!(self.state, AgentState::Idle { .. })
            && self.last_detection_at.elapsed() < Duration::from_secs(2)
        {
            return None;
        }
        self.last_detection_at = Instant::now();

        // Check process exit
        let process_exited = match self.child.try_wait() {
            Ok(Some(status)) => Some(ProcessExit {
                exit_code: Some(status.exit_code() as i32),
                signal: status.signal().is_some(),
            }),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!("try_wait failed for agent {}: {}", self.name, e);
                None
            }
        };

        // Calculate seconds since last output
        let seconds_since_output = match self.last_output_at {
            Some(last) => (Utc::now() - last).num_milliseconds() as f64 / 1000.0,
            None => 0.0,
        };

        // Extract screen lines
        let screen_lines = extract_screen_lines(self.parser.screen(), patterns.scan_lines());

        let signals = DetectionSignals {
            process_exited,
            screen_lines,
            seconds_since_output,
            current_state: &self.state,
            idle_timeout_secs,
        };

        let detected = detect_state(patterns, &signals);

        // Apply debounce
        if let Some(new_state) = self.debounce.process(detected, &self.state) {
            let _old_state = std::mem::replace(&mut self.state, new_state.clone());
            self.dirty = true;
            if new_state.is_terminal() {
                self.has_unread_result = true;
            }
            Some(new_state)
        } else {
            None
        }
    }

    // ---- Lifecycle ----

    /// Gracefully shut down the agent's process.
    ///
    /// Sends Ctrl-C via the PTY to let Electron/Claude Code shut down cleanly,
    /// then sends SIGTERM to the process group for a clean exit. Falls back to
    /// the hard kill only if the process doesn't exit within the grace period.
    /// This avoids the macOS "Electron was unexpectedly ended" crash dialog
    /// that appears when Electron receives SIGKILL without a chance to clean up.
    pub fn kill(&mut self) {
        // Step 1: Try graceful shutdown by sending Ctrl-C through the PTY.
        // This is how a terminal user would interrupt a process.
        let _ = self.pty.write(b"\x03");

        // Step 2: Send SIGTERM to the process group for a clean exit.
        // The child was spawned with setsid(), so its PID == PGID.
        // Using negative PID sends the signal to the entire process group,
        // ensuring Electron's child processes also receive it.
        if let Some(pid) = self.child.process_id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }

        // Step 3: Give the process a generous grace period to exit cleanly.
        // Electron needs more time than the 250ms that portable-pty allows.
        for _ in 0..20 {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.pty.shutdown();
                    self.state = AgentState::Errored {
                        at: Utc::now(),
                        error_hint: Some("Killed by user".to_string()),
                    };
                    self.dirty = true;
                    return;
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }

        // Step 4: Last resort — force kill via portable-pty (SIGHUP → SIGKILL).
        tracing::warn!(
            "Agent '{}' did not exit after 1s grace period, force killing",
            self.name
        );
        if let Err(e) = self.child.kill() {
            tracing::warn!("Failed to kill agent {}: {}", self.name, e);
        }
        self.pty.shutdown();
        self.state = AgentState::Errored {
            at: Utc::now(),
            error_hint: Some("Killed by user".to_string()),
        };
        self.dirty = true;
    }

    /// Get the raw scrollback bytes for session persistence.
    pub fn scrollback_raw_bytes(&self) -> &[u8] {
        self.scrollback.raw_bytes()
    }

    /// Load scrollback bytes for history without affecting the visible screen.
    ///
    /// Stores raw bytes in the ScrollbackBuffer so they are persisted on the
    /// next session save, but does NOT feed them through the vt100 parser,
    /// keeping the visible terminal screen clean for the new process.
    pub fn load_scrollback_history(&mut self, data: &[u8]) {
        self.scrollback.append(data);
        self.dirty = true;
    }

    /// Get the spawn parameters needed to restart this agent.
    pub fn restart_params(&self) -> RestartParams {
        RestartParams {
            name: self.name.clone(),
            project_name: self.project_name.clone(),
            command: self.spawn_command.clone(),
            args: self.spawn_args.clone(),
            cwd: self.spawn_cwd.clone(),
            env: self.spawn_env.clone(),
            session_id: self.session_id.clone(),
        }
    }
}

/// Parameters needed to restart an agent.
pub struct RestartParams {
    pub name: String,
    pub project_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub session_id: Option<String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_uptime_formatting_seconds() {
        // Test seconds-only range
        let fmt = format_duration(30);
        assert_eq!(fmt, "30s");
    }

    #[test]
    fn test_uptime_formatting_minutes() {
        let fmt = format_duration(125);
        assert_eq!(fmt, "2m 5s");
    }

    #[test]
    fn test_uptime_formatting_hours() {
        let fmt = format_duration(3661);
        assert_eq!(fmt, "1h 1m");
    }

    /// Helper that mimics the uptime formatting logic with a known seconds value.
    fn format_duration(secs: i64) -> String {
        if secs < 60 {
            format!("{secs}s")
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }
}
