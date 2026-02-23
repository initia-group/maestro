# Feature 06: Agent Lifecycle Management

## Overview

Implement the `AgentHandle` (bundles all per-agent resources) and `AgentManager` (orchestrates multiple agents). Together, they provide the complete agent lifecycle: spawn, monitor, interact, restart, kill. The AgentManager is the single point of contact for the rest of the application to manage agents.

## Dependencies

- **Feature 02** (Configuration System) — `ProjectConfig`, `AgentConfig`, `TemplateConfig`.
- **Feature 03** (Core Types & Event System) — `AgentId`, `AppEvent`, `EventBus::sender()`.
- **Feature 04** (PTY Management) — `PtyController`, `spawn_in_pty`, `SpawnConfig`.
- **Feature 05** (Agent State Machine) — `AgentState`, `PromptType`, `DetectionPatterns`, `detect_state`, `DetectionDebounce`.

## Technical Specification

### AgentHandle (`src/agent/handle.rs`)

Bundles all resources and state for a single agent.

```rust
use crate::agent::state::{AgentState, PromptType};
use crate::agent::detector::{DetectionPatterns, DetectionSignals, ProcessExit, detect_state, extract_screen_lines_simple, DetectionDebounce};
use crate::agent::AgentId;
use crate::pty::PtyController;
use chrono::{DateTime, Utc};
use portable_pty::{Child, PtySize};
use color_eyre::eyre::Result;

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
    spawn_cwd: std::path::PathBuf,

    /// Additional environment variables (for restart).
    spawn_env: std::collections::HashMap<String, String>,
}

impl AgentHandle {
    /// Create a new agent handle.
    pub fn new(
        id: AgentId,
        name: String,
        project_name: String,
        pty: PtyController,
        parser: vt100::Parser,
        child: Box<dyn Child + Send + Sync>,
        spawn_command: String,
        spawn_args: Vec<String>,
        spawn_cwd: std::path::PathBuf,
        spawn_env: std::collections::HashMap<String, String>,
    ) -> Self {
        Self {
            id,
            name,
            project_name,
            state: AgentState::Spawning,
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
        }
    }

    // ─── Accessors ─────────────────────────────────────

    pub fn id(&self) -> AgentId { self.id }
    pub fn name(&self) -> &str { &self.name }
    pub fn project_name(&self) -> &str { &self.project_name }
    pub fn state(&self) -> &AgentState { &self.state }
    pub fn spawned_at(&self) -> DateTime<Utc> { self.spawned_at }
    pub fn is_dirty(&self) -> bool { self.dirty }

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
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    // ─── PTY Interaction ───────────────────────────────

    /// Process raw bytes from the PTY output.
    /// Called when a `PtyOutput` event is received.
    pub fn process_output(&mut self, data: &[u8]) {
        self.parser.process(data);
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
        self.parser.set_size(size.rows, size.cols);
        self.dirty = true;
    }

    // ─── State Detection ───────────────────────────────

    /// Run state detection and update the internal state.
    /// Returns `Some(new_state)` if the state changed, `None` otherwise.
    pub fn detect_and_update(
        &mut self,
        patterns: &DetectionPatterns,
        idle_timeout_secs: u64,
    ) -> Option<AgentState> {
        // Check process exit
        let process_exited = match self.child.try_wait() {
            Ok(Some(status)) => Some(ProcessExit {
                exit_code: status.exit_code().map(|c| c as i32),
                signal: false, // portable-pty doesn't expose signal info directly
            }),
            Ok(None) => None, // Still running
            Err(e) => {
                tracing::warn!("try_wait failed for agent {}: {}", self.name, e);
                None
            }
        };

        // Calculate seconds since last output
        let seconds_since_output = match self.last_output_at {
            Some(last) => (Utc::now() - last).num_milliseconds() as f64 / 1000.0,
            None => 0.0, // No output yet — don't trigger idle
        };

        // Extract screen lines
        let screen_lines = extract_screen_lines_simple(self.parser.screen(), patterns.scan_lines());

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
            let old_state = std::mem::replace(&mut self.state, new_state.clone());
            self.dirty = true;
            Some(new_state)
        } else {
            None
        }
    }

    // ─── Lifecycle ─────────────────────────────────────

    /// Kill the agent's process.
    pub fn kill(&mut self) {
        // First try graceful termination
        if let Err(e) = self.child.kill() {
            tracing::warn!("Failed to kill agent {}: {}", self.name, e);
        }
        self.pty.shutdown();
        self.state = AgentState::Errored {
            at: Utc::now(),
            exit_code: None,
            error_hint: Some("Killed by user".to_string()),
        };
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
        }
    }
}

/// Parameters needed to restart an agent.
pub struct RestartParams {
    pub name: String,
    pub project_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: std::path::PathBuf,
    pub env: std::collections::HashMap<String, String>,
}
```

### AgentManager (`src/agent/manager.rs`)

Orchestrates all agents. Provides a high-level API for the application layer.

```rust
use crate::agent::handle::{AgentHandle, RestartParams};
use crate::agent::state::AgentState;
use crate::agent::detector::DetectionPatterns;
use crate::agent::AgentId;
use crate::config::settings::{AgentConfig, MaestroConfig, ProjectConfig};
use crate::event::types::AppEvent;
use crate::pty::{spawn_in_pty, SpawnConfig};
use color_eyre::eyre::{Result, bail};
use portable_pty::PtySize;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

/// Manages the lifecycle of all agents.
pub struct AgentManager {
    /// All agent handles, indexed by ID.
    agents: HashMap<AgentId, AgentHandle>,

    /// Agent IDs in display order (for sidebar rendering).
    /// Grouped by project: Vec<(project_name, Vec<AgentId>)>
    display_order: Vec<(String, Vec<AgentId>)>,

    /// Event bus sender — passed to new PTY controllers.
    event_tx: mpsc::UnboundedSender<AppEvent>,

    /// Detection patterns (compiled once, shared across all agents).
    detection_patterns: DetectionPatterns,

    /// Maximum number of concurrent agents.
    max_agents: usize,

    /// Global config reference for defaults.
    config: MaestroConfig,
}

impl AgentManager {
    /// Create a new manager from configuration.
    pub fn new(config: &MaestroConfig, event_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        let detection_patterns = DetectionPatterns::from_config(&config.detection);

        Self {
            agents: HashMap::new(),
            display_order: Vec::new(),
            event_tx,
            detection_patterns,
            max_agents: config.global.max_agents,
            config: config.clone(),
        }
    }

    // ─── Spawning ──────────────────────────────────────

    /// Spawn a new agent from explicit parameters.
    pub fn spawn(
        &mut self,
        name: String,
        project_name: String,
        command: String,
        args: Vec<String>,
        cwd: std::path::PathBuf,
        env: HashMap<String, String>,
        pty_size: PtySize,
    ) -> Result<AgentId> {
        // Check agent limit
        let alive_count = self.agents.values().filter(|a| a.state().is_alive()).count();
        if alive_count >= self.max_agents {
            bail!(
                "Agent limit reached ({}/{}). Kill an agent first.",
                alive_count,
                self.max_agents,
            );
        }

        // Check for duplicate name within same project
        if self.find_by_name(&project_name, &name).is_some() {
            bail!("Agent '{}' already exists in project '{}'", name, project_name);
        }

        info!("Spawning agent '{}' in project '{}': {} {:?}", name, project_name, command, args);

        let spawn_config = SpawnConfig {
            command: command.clone(),
            args: args.clone(),
            cwd: cwd.clone(),
            env: env.clone(),
            size: pty_size,
        };

        let result = spawn_in_pty(spawn_config)?;
        let id = AgentId::new();

        // Create vt100 parser with matching dimensions
        let parser = vt100::Parser::new(pty_size.rows, pty_size.cols, 0);

        // Create PTY controller
        let pty_controller = PtyController::new(id, result.master, self.event_tx.clone())?;

        // Create handle
        let handle = AgentHandle::new(
            id,
            name.clone(),
            project_name.clone(),
            pty_controller,
            parser,
            result.child,
            command,
            args,
            cwd,
            env,
        );

        // Add to collections
        self.agents.insert(id, handle);
        self.add_to_display_order(&project_name, id);

        info!("Agent '{}' spawned with ID {}", name, id);
        Ok(id)
    }

    /// Spawn all auto_start agents from the configuration.
    pub fn spawn_auto_start_agents(&mut self, pty_size: PtySize) -> Vec<(String, color_eyre::eyre::Report)> {
        let mut errors = Vec::new();

        let projects: Vec<ProjectConfig> = self.config.project.clone();
        for project in &projects {
            for agent_config in &project.agent {
                if agent_config.auto_start {
                    let command = agent_config.command
                        .clone()
                        .unwrap_or_else(|| self.config.global.claude_binary.clone());
                    let cwd = agent_config.cwd
                        .clone()
                        .unwrap_or_else(|| project.path.clone());

                    if let Err(e) = self.spawn(
                        agent_config.name.clone(),
                        project.name.clone(),
                        command,
                        agent_config.args.clone(),
                        cwd,
                        agent_config.env.clone(),
                        pty_size,
                    ) {
                        let msg = format!("{}/{}", project.name, agent_config.name);
                        errors.push((msg, e));
                    }
                }
            }
        }

        errors
    }

    // ─── Lifecycle ─────────────────────────────────────

    /// Kill an agent by ID.
    pub fn kill(&mut self, id: AgentId) -> Result<()> {
        match self.agents.get_mut(&id) {
            Some(handle) => {
                info!("Killing agent '{}'", handle.name());
                handle.kill();
                Ok(())
            }
            None => bail!("Agent {} not found", id),
        }
    }

    /// Restart an agent by ID. Kills the old one and spawns a new one.
    /// Returns the new AgentId.
    pub fn restart(&mut self, id: AgentId, pty_size: PtySize) -> Result<AgentId> {
        let params = match self.agents.get(&id) {
            Some(handle) => handle.restart_params(),
            None => bail!("Agent {} not found", id),
        };

        // Kill the old agent
        self.kill(id)?;

        // Remove old agent from collections
        self.agents.remove(&id);
        self.remove_from_display_order(id);

        // Spawn replacement
        let new_id = self.spawn(
            params.name,
            params.project_name,
            params.command,
            params.args,
            params.cwd,
            params.env,
            pty_size,
        )?;

        Ok(new_id)
    }

    /// Kill all agents (for graceful shutdown).
    /// Returns after sending kill signals. Does NOT wait for processes to exit.
    pub fn kill_all(&mut self) {
        for handle in self.agents.values_mut() {
            if handle.state().is_alive() {
                handle.kill();
            }
        }
    }

    /// Wait for all agents to exit, with a timeout.
    /// After timeout, force-kills any remaining agents.
    pub async fn shutdown_all(&mut self, timeout: std::time::Duration) {
        self.kill_all();

        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let any_alive = self.agents.values().any(|a| {
                matches!(a.state(), AgentState::Spawning | AgentState::Running { .. } | AgentState::WaitingForInput { .. } | AgentState::Idle { .. })
            });

            if !any_alive {
                info!("All agents have exited");
                return;
            }

            if tokio::time::Instant::now() >= deadline {
                warn!("Shutdown timeout reached — some agents may still be running");
                return;
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Re-check process status
            for handle in self.agents.values_mut() {
                handle.detect_and_update(&self.detection_patterns, self.config.global.idle_timeout_secs);
            }
        }
    }

    // ─── Queries ───────────────────────────────────────

    /// Get an agent handle by ID (immutable).
    pub fn get(&self, id: AgentId) -> Option<&AgentHandle> {
        self.agents.get(&id)
    }

    /// Get an agent handle by ID (mutable).
    pub fn get_mut(&mut self, id: AgentId) -> Option<&mut AgentHandle> {
        self.agents.get_mut(&id)
    }

    /// Find an agent by name within a project.
    pub fn find_by_name(&self, project_name: &str, agent_name: &str) -> Option<AgentId> {
        self.agents.values()
            .find(|a| a.project_name() == project_name && a.name() == agent_name)
            .map(|a| a.id())
    }

    /// Get all agents in display order, grouped by project.
    pub fn agents_by_project(&self) -> &[(String, Vec<AgentId>)] {
        &self.display_order
    }

    /// Get the total count of agents in each state (for status bar).
    pub fn state_counts(&self) -> StateCounts {
        let mut counts = StateCounts::default();
        for handle in self.agents.values() {
            match handle.state() {
                AgentState::Spawning => counts.spawning += 1,
                AgentState::Running { .. } => counts.running += 1,
                AgentState::WaitingForInput { .. } => counts.waiting += 1,
                AgentState::Idle { .. } => counts.idle += 1,
                AgentState::Completed { .. } => counts.completed += 1,
                AgentState::Errored { .. } => counts.errored += 1,
            }
        }
        counts
    }

    /// Total number of agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Flat list of all agent IDs in display order.
    pub fn all_agent_ids_ordered(&self) -> Vec<AgentId> {
        self.display_order
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }

    // ─── State Detection ───────────────────────────────

    /// Run state detection on all agents. Called on every StateTick event.
    /// Returns a list of (agent_id, old_state, new_state) for any agents that changed.
    pub fn detect_all_states(&mut self) -> Vec<(AgentId, AgentState, AgentState)> {
        let mut changes = Vec::new();
        let idle_timeout = self.config.global.idle_timeout_secs;
        let agent_ids: Vec<AgentId> = self.agents.keys().copied().collect();

        for id in agent_ids {
            if let Some(handle) = self.agents.get_mut(&id) {
                let old_state = handle.state().clone();
                if let Some(new_state) = handle.detect_and_update(&self.detection_patterns, idle_timeout) {
                    changes.push((id, old_state, new_state));
                }
            }
        }

        changes
    }

    // ─── Internal ──────────────────────────────────────

    fn add_to_display_order(&mut self, project_name: &str, id: AgentId) {
        if let Some((_, ids)) = self.display_order.iter_mut().find(|(name, _)| name == project_name) {
            ids.push(id);
        } else {
            self.display_order.push((project_name.to_string(), vec![id]));
        }
    }

    fn remove_from_display_order(&mut self, id: AgentId) {
        for (_, ids) in &mut self.display_order {
            ids.retain(|&i| i != id);
        }
        self.display_order.retain(|(_, ids)| !ids.is_empty());
    }
}

/// Aggregate state counts for the status bar.
#[derive(Debug, Default, Clone)]
pub struct StateCounts {
    pub spawning: usize,
    pub running: usize,
    pub waiting: usize,
    pub idle: usize,
    pub completed: usize,
    pub errored: usize,
}

impl StateCounts {
    /// Format for the status bar: "● 2 running  ? 1 waiting  - 1 idle"
    pub fn format_status_bar(&self) -> String {
        let mut parts = Vec::new();
        if self.running > 0 {
            parts.push(format!("● {} running", self.running));
        }
        if self.waiting > 0 {
            parts.push(format!("? {} waiting", self.waiting));
        }
        if self.idle > 0 {
            parts.push(format!("- {} idle", self.idle));
        }
        if self.completed > 0 {
            parts.push(format!("✓ {} done", self.completed));
        }
        if self.errored > 0 {
            parts.push(format!("! {} err", self.errored));
        }
        if self.spawning > 0 {
            parts.push(format!("○ {} starting", self.spawning));
        }
        parts.join("  ")
    }
}
```

### Agent Display Order

Agents are displayed in the sidebar grouped by project, in the order projects appear in the config file. Within each project, agents are ordered by spawn time (oldest first). This is maintained by `display_order: Vec<(String, Vec<AgentId>)>`.

When a new agent is spawned, it's appended to its project's group. If the project group doesn't exist yet, a new group is created at the end.

### Sidebar Selection State

The sidebar selection is **not** part of AgentManager — it belongs to the App (Feature 12). The AgentManager provides `all_agent_ids_ordered()` which the App uses to translate a selection index to an AgentId.

## Implementation Steps

1. **Implement `src/agent/handle.rs`**
   - `AgentHandle` struct with all fields.
   - Constructor, accessors, PTY interaction methods.
   - `detect_and_update()` for state detection.
   - `kill()` and `restart_params()`.
   - `RestartParams` struct.

2. **Implement `src/agent/manager.rs`**
   - `AgentManager` struct.
   - `spawn()`, `kill()`, `restart()` methods.
   - `spawn_auto_start_agents()` for startup.
   - `kill_all()`, `shutdown_all()` for graceful shutdown.
   - Query methods: `get()`, `find_by_name()`, `agents_by_project()`, `state_counts()`.
   - `detect_all_states()` for the state tick handler.
   - `StateCounts` struct with formatting.

3. **Update `src/agent/mod.rs`**
   - Re-export `AgentHandle`, `AgentManager`, `AgentId`, `StateCounts`.

4. **Write tests** (see below).

## Error Handling

| Error | Handling |
|---|---|
| Agent limit reached | Return descriptive error. UI should show this in status bar or popup. |
| Duplicate agent name | Return error. User must choose a different name. |
| Spawn failure (command not found, bad cwd) | Propagated from PTY spawner with context. |
| Kill failure (already exited) | Log warning, continue. Not an error for the user. |
| Restart of non-existent agent | Return error. |
| `try_wait()` fails in detection | Log warning, skip process exit detection for this tick. |

## Testing Strategy

### Unit Tests — `AgentHandle`

```rust
#[test]
fn test_uptime_formatting() {
    // Test with known elapsed times
    // (This requires mocking Utc::now(), which is complex.
    //  Alternative: test the formatting function separately with explicit durations.)
}

#[test]
fn test_agent_starts_in_spawning_state() {
    // After construction, state should be Spawning
}
```

### Unit Tests — `AgentManager`

```rust
// These tests would require mocking the PTY system.
// For v0.1, focus on integration tests with real PTYs.
```

### Unit Tests — `StateCounts`

```rust
#[test]
fn test_status_bar_format_all_states() {
    let counts = StateCounts {
        running: 2,
        waiting: 1,
        idle: 1,
        completed: 1,
        errored: 1,
        spawning: 0,
    };
    let text = counts.format_status_bar();
    assert!(text.contains("● 2 running"));
    assert!(text.contains("? 1 waiting"));
    assert!(text.contains("- 1 idle"));
    assert!(text.contains("✓ 1 done"));
    assert!(text.contains("! 1 err"));
}

#[test]
fn test_status_bar_format_empty() {
    let counts = StateCounts::default();
    assert_eq!(counts.format_status_bar(), "");
}

#[test]
fn test_status_bar_format_running_only() {
    let counts = StateCounts { running: 3, ..Default::default() };
    assert_eq!(counts.format_status_bar(), "● 3 running");
}
```

### Integration Tests (`tests/integration/agent_lifecycle.rs`)

```rust
#[tokio::test]
async fn test_spawn_and_kill_agent() {
    let config = MaestroConfig::default();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut manager = AgentManager::new(&config, tx);

    let pty_size = PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 };
    let id = manager.spawn(
        "test-agent".into(),
        "test-project".into(),
        "cat".into(),
        vec![],
        std::env::temp_dir(),
        HashMap::new(),
        pty_size,
    ).unwrap();

    // Agent should exist
    assert!(manager.get(id).is_some());
    assert_eq!(manager.agent_count(), 1);

    // Kill it
    manager.kill(id).unwrap();

    // Agent should be in Errored state (killed by user)
    let handle = manager.get(id).unwrap();
    assert!(handle.state().is_terminal());
}

#[tokio::test]
async fn test_spawn_limit_enforcement() {
    let mut config = MaestroConfig::default();
    config.global.max_agents = 2;
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut manager = AgentManager::new(&config, tx);

    let pty_size = PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 };

    // Spawn 2 agents (at limit)
    manager.spawn("a1".into(), "p".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size).unwrap();
    manager.spawn("a2".into(), "p".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size).unwrap();

    // Third should fail
    let result = manager.spawn("a3".into(), "p".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_duplicate_name_rejected() {
    let config = MaestroConfig::default();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut manager = AgentManager::new(&config, tx);

    let pty_size = PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 };

    manager.spawn("agent".into(), "project".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size).unwrap();

    // Same name + project should fail
    let result = manager.spawn("agent".into(), "project".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size);
    assert!(result.is_err());

    // Same name + different project should succeed
    let result = manager.spawn("agent".into(), "other-project".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_display_order_grouping() {
    let config = MaestroConfig::default();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut manager = AgentManager::new(&config, tx);

    let pty_size = PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 };

    manager.spawn("a1".into(), "proj-a".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size).unwrap();
    manager.spawn("b1".into(), "proj-b".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size).unwrap();
    manager.spawn("a2".into(), "proj-a".into(), "cat".into(), vec![], std::env::temp_dir(), HashMap::new(), pty_size).unwrap();

    let groups = manager.agents_by_project();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].0, "proj-a");
    assert_eq!(groups[0].1.len(), 2);
    assert_eq!(groups[1].0, "proj-b");
    assert_eq!(groups[1].1.len(), 1);
}
```

## Acceptance Criteria

- [ ] `AgentHandle` bundles PTY controller, vt100 parser, state, and metadata.
- [ ] `AgentHandle::process_output()` feeds data to the vt100 parser and sets dirty flag.
- [ ] `AgentHandle::write_input()` sends bytes to the PTY.
- [ ] `AgentHandle::resize()` resizes both PTY and vt100 parser.
- [ ] `AgentHandle::detect_and_update()` runs detection with debounce.
- [ ] `AgentManager::spawn()` creates an agent with a PTY and returns its ID.
- [ ] `AgentManager::kill()` kills the agent's process and marks it Errored.
- [ ] `AgentManager::restart()` kills and re-spawns with the same parameters.
- [ ] `AgentManager::spawn_auto_start_agents()` spawns all `auto_start = true` agents.
- [ ] Agent limit is enforced.
- [ ] Duplicate names within the same project are rejected.
- [ ] `display_order` maintains correct project grouping.
- [ ] `state_counts()` returns accurate aggregate counts.
- [ ] `shutdown_all()` kills all agents and waits for exit with timeout.
- [ ] All tests pass.
