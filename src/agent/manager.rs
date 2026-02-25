//! Agent lifecycle management.
//!
//! The `AgentManager` owns all agent handles and provides spawn, kill,
//! restart, and state detection operations. It is the single point of
//! contact for the rest of the application to manage agents.

use crate::agent::detector::DetectionPatterns;
use crate::agent::handle::{AgentHandle, RestartParams};
use crate::agent::state::AgentState;
use crate::agent::AgentId;
use crate::config::settings::MaestroConfig;
use crate::event::types::AppEvent;
use crate::pty::{spawn_in_pty, PtyController, SpawnConfig};
use color_eyre::eyre::{bail, Result};
use portable_pty::PtySize;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Manages the lifecycle of all agents.
pub struct AgentManager {
    /// All agent handles, indexed by ID.
    agents: HashMap<AgentId, AgentHandle>,

    /// Agent IDs in display order, grouped by project.
    display_order: Vec<(String, Vec<AgentId>)>,

    /// Unbounded event sender — passed to new PTY controllers.
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

    // ---- Spawning ----

    /// Spawn a new agent from explicit parameters.
    #[allow(clippy::too_many_arguments)]
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
            bail!(
                "Agent '{}' already exists in project '{}'",
                name,
                project_name
            );
        }

        // For Claude commands, generate a session ID and inject --session-id
        // so we can resume this exact conversation later.
        let mut args = args;
        let session_id = if command.ends_with("claude") {
            if let Some(pos) = args.iter().position(|a| a == "--session-id") {
                // Already has --session-id, extract the value
                args.get(pos + 1).cloned()
            } else if let Some(pos) = args.iter().position(|a| a == "--resume" || a == "-r") {
                // Resuming a session — the session ID is the next arg
                args.get(pos + 1).cloned()
            } else {
                // New session: generate a UUID and inject --session-id
                let sid = uuid::Uuid::new_v4().to_string();
                args.push("--session-id".to_string());
                args.push(sid.clone());
                Some(sid)
            }
        } else {
            None
        };

        info!(
            "Spawning agent '{}' in project '{}': {} {:?}",
            name, project_name, command, args
        );

        let spawn_config = SpawnConfig {
            command: command.clone(),
            args: args.clone(),
            cwd: cwd.clone(),
            env: env.clone(),
            size: pty_size,
        };

        let result = spawn_in_pty(spawn_config)?;
        let id = AgentId::new();

        // Create vt100 parser with matching dimensions and 10000 lines of scrollback
        let parser = vt100::Parser::new(pty_size.rows, pty_size.cols, 10000);

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
            session_id,
        );

        // Add to collections
        self.agents.insert(id, handle);
        self.add_to_display_order(&project_name, id);

        info!("Agent '{}' spawned with ID {}", name, id);
        Ok(id)
    }

    /// Spawn all auto_start agents from the configuration.
    pub fn spawn_auto_start_agents(
        &mut self,
        pty_size: PtySize,
    ) -> Vec<(String, color_eyre::eyre::Report)> {
        let mut errors = Vec::new();

        let projects: Vec<_> = self.config.project.clone();
        for project in &projects {
            for agent_config in &project.agent {
                if agent_config.auto_start {
                    let command = agent_config
                        .command
                        .clone()
                        .unwrap_or_else(|| self.config.global.claude_binary.clone());
                    let cwd = agent_config
                        .cwd
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
                        error!("Failed to spawn auto-start agent {}: {}", msg, e);
                        errors.push((msg, e));
                    }
                }
            }
        }

        errors
    }

    // ---- Lifecycle ----

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
        let params: RestartParams = match self.agents.get(&id) {
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

    /// Retry a failed `--resume` agent by spawning a fresh session.
    ///
    /// Strips `--resume`/`-r` and its session-id value from the args,
    /// removes the errored agent, and spawns a new one. The new agent
    /// has `resume_retry_attempted` set to prevent further retries.
    pub fn retry_without_resume(
        &mut self,
        id: AgentId,
        pty_size: PtySize,
    ) -> Result<AgentId> {
        let params = match self.agents.get(&id) {
            Some(handle) => handle.restart_params(),
            None => bail!("Agent {} not found", id),
        };

        // Strip --resume/-r and its value from args
        let mut new_args: Vec<String> = Vec::new();
        let mut skip_next = false;
        for arg in &params.args {
            if skip_next {
                skip_next = false;
                continue;
            }
            if arg == "--resume" || arg == "-r" {
                skip_next = true;
                continue;
            }
            new_args.push(arg.clone());
        }

        // Remove the errored agent (already exited, no need to kill)
        self.agents.remove(&id);
        self.remove_from_display_order(id);

        // Spawn fresh replacement
        let new_id = self.spawn(
            params.name,
            params.project_name,
            params.command,
            new_args,
            params.cwd,
            params.env,
            pty_size,
        )?;

        // Prevent further auto-retry on the new handle
        if let Some(handle) = self.agents.get_mut(&new_id) {
            handle.set_resume_retry_attempted(true);
        }

        info!("Retried stale session agent {} -> {} (fresh session)", id, new_id);
        Ok(new_id)
    }

    /// Rename an agent by ID.
    ///
    /// Validates that the new name is not empty and not a duplicate
    /// within the same project. Returns the old name on success.
    pub fn rename(&mut self, id: AgentId, new_name: String) -> Result<String> {
        // Validate non-empty
        if new_name.is_empty() {
            bail!("Agent name cannot be empty");
        }

        // Check the agent exists and get its project
        let project_name = match self.agents.get(&id) {
            Some(handle) => handle.project_name().to_string(),
            None => bail!("Agent {} not found", id),
        };

        // Check for duplicate name within the same project (excluding self)
        let duplicate = self.agents.values().any(|a| {
            a.id() != id && a.project_name() == project_name && a.name() == new_name
        });
        if duplicate {
            bail!(
                "Agent '{}' already exists in project '{}'",
                new_name,
                project_name
            );
        }

        let handle = self.agents.get_mut(&id).unwrap();
        let old_name = handle.name().to_string();
        handle.set_name(new_name.clone());

        info!("Renamed agent '{}' -> '{}'", old_name, new_name);
        Ok(old_name)
    }

    /// Rename a project.
    ///
    /// Updates the project name in the display order and all agent handles
    /// that belong to the project. Returns an error if the new name is empty,
    /// the old project doesn't exist, or a project with the new name already exists.
    pub fn rename_project(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        if new_name.is_empty() {
            bail!("Project name cannot be empty");
        }

        if old_name == new_name {
            return Ok(());
        }

        // Check that the old project exists
        let project_exists = self
            .display_order
            .iter()
            .any(|(name, _)| name == old_name);
        if !project_exists {
            bail!("Project '{}' not found", old_name);
        }

        // Check that the new name doesn't conflict
        let name_taken = self
            .display_order
            .iter()
            .any(|(name, _)| name == new_name);
        if name_taken {
            bail!("Project '{}' already exists", new_name);
        }

        // Update display order
        for (name, _) in &mut self.display_order {
            if name == old_name {
                *name = new_name.to_string();
                break;
            }
        }

        // Update all agent handles that belong to this project
        for handle in self.agents.values_mut() {
            if handle.project_name() == old_name {
                handle.set_project_name(new_name.to_string());
            }
        }

        info!("Renamed project '{}' -> '{}'", old_name, new_name);
        Ok(())
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
            let any_alive = self.agents.values().any(|a| a.state().is_alive());

            if !any_alive {
                info!("All agents have exited");
                return;
            }

            if tokio::time::Instant::now() >= deadline {
                warn!("Shutdown timeout reached -- some agents may still be running");
                return;
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Re-check process status
            let idle_timeout = self.config.global.idle_timeout_secs;
            let ids: Vec<AgentId> = self.agents.keys().copied().collect();
            for id in ids {
                if let Some(handle) = self.agents.get_mut(&id) {
                    handle.detect_and_update(&self.detection_patterns, idle_timeout);
                }
            }
        }
    }

    // ---- Queries ----

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
        self.agents
            .values()
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
                AgentState::Spawning { .. } => counts.spawning += 1,
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

    // ---- State Detection ----

    /// Run state detection on all agents. Called on every StateTick event.
    /// Returns a list of (agent_id, old_state, new_state) for any agents that changed.
    pub fn detect_all_states(&mut self) -> Vec<(AgentId, AgentState, AgentState)> {
        let mut changes = Vec::new();
        let idle_timeout = self.config.global.idle_timeout_secs;
        let agent_ids: Vec<AgentId> = self.agents.keys().copied().collect();

        for id in agent_ids {
            if let Some(handle) = self.agents.get_mut(&id) {
                let old_state = handle.state().clone();
                if let Some(new_state) =
                    handle.detect_and_update(&self.detection_patterns, idle_timeout)
                {
                    changes.push((id, old_state, new_state));
                }
            }
        }

        changes
    }

    /// Register an empty project in the display order.
    ///
    /// This adds a project header with no agents to the sidebar.
    /// Agents can be spawned into this project later via `spawn()`.
    /// Returns `Err` if a project with the same name already exists.
    pub fn add_empty_project(&mut self, project_name: &str) -> Result<()> {
        if self
            .display_order
            .iter()
            .any(|(name, _)| name == project_name)
        {
            bail!("Project '{}' already exists", project_name);
        }
        self.display_order
            .push((project_name.to_string(), vec![]));
        Ok(())
    }

    /// Remove a dead agent from all collections.
    ///
    /// The agent must already be in a terminal state (completed/errored).
    /// This removes it from the `agents` map and from the display order
    /// so it disappears from the sidebar immediately.
    pub fn remove(&mut self, id: AgentId) {
        self.agents.remove(&id);
        self.remove_from_display_order(id);
    }

    // ---- Internal ----

    fn add_to_display_order(&mut self, project_name: &str, id: AgentId) {
        if let Some((_, ids)) = self
            .display_order
            .iter_mut()
            .find(|(name, _)| name == project_name)
        {
            ids.push(id);
        } else {
            self.display_order
                .push((project_name.to_string(), vec![id]));
        }
    }

    fn remove_from_display_order(&mut self, id: AgentId) {
        for (_, ids) in &mut self.display_order {
            ids.retain(|&i| i != id);
        }
        self.display_order.retain(|(_, ids)| !ids.is_empty());
    }

    /// Move the given agent one position earlier within its project group.
    pub fn move_agent_up(&mut self, id: AgentId) {
        for (_, ids) in &mut self.display_order {
            if let Some(pos) = ids.iter().position(|&i| i == id) {
                if pos > 0 {
                    ids.swap(pos - 1, pos);
                }
                return;
            }
        }
    }

    /// Move the given agent one position later within its project group.
    pub fn move_agent_down(&mut self, id: AgentId) {
        for (_, ids) in &mut self.display_order {
            if let Some(pos) = ids.iter().position(|&i| i == id) {
                if pos + 1 < ids.len() {
                    ids.swap(pos, pos + 1);
                }
                return;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- StateCounts tests ----

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
        let counts = StateCounts {
            running: 3,
            ..Default::default()
        };
        assert_eq!(counts.format_status_bar(), "● 3 running");
    }

    #[test]
    fn test_status_bar_format_spawning() {
        let counts = StateCounts {
            spawning: 2,
            ..Default::default()
        };
        assert_eq!(counts.format_status_bar(), "○ 2 starting");
    }

    #[test]
    fn test_status_bar_format_order() {
        let counts = StateCounts {
            running: 1,
            spawning: 1,
            ..Default::default()
        };
        let text = counts.format_status_bar();
        // Running should appear before spawning
        let running_pos = text.find("running").unwrap();
        let spawning_pos = text.find("starting").unwrap();
        assert!(running_pos < spawning_pos);
    }

    // ---- Integration tests (spawn/kill with real PTY) ----

    #[tokio::test]
    async fn test_spawn_and_kill_agent() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };
        let id = manager
            .spawn(
                "test-agent".into(),
                "test-project".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

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

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        // Spawn 2 agents (at limit)
        manager
            .spawn(
                "a1".into(),
                "p".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();
        manager
            .spawn(
                "a2".into(),
                "p".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        // Third should fail
        let result = manager.spawn(
            "a3".into(),
            "p".into(),
            "cat".into(),
            vec![],
            std::env::temp_dir(),
            HashMap::new(),
            pty_size,
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_duplicate_name_rejected() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        manager
            .spawn(
                "agent".into(),
                "project".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        // Same name + project should fail
        let result = manager.spawn(
            "agent".into(),
            "project".into(),
            "cat".into(),
            vec![],
            std::env::temp_dir(),
            HashMap::new(),
            pty_size,
        );
        assert!(result.is_err());

        // Same name + different project should succeed
        let result = manager.spawn(
            "agent".into(),
            "other-project".into(),
            "cat".into(),
            vec![],
            std::env::temp_dir(),
            HashMap::new(),
            pty_size,
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_display_order_grouping() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        manager
            .spawn(
                "a1".into(),
                "proj-a".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();
        manager
            .spawn(
                "b1".into(),
                "proj-b".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();
        manager
            .spawn(
                "a2".into(),
                "proj-a".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        let groups = manager.agents_by_project();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "proj-a");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "proj-b");
        assert_eq!(groups[1].1.len(), 1);
    }

    #[tokio::test]
    async fn test_kill_nonexistent_agent() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let result = manager.kill(AgentId::new());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_restart_agent() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        let old_id = manager
            .spawn(
                "test-agent".into(),
                "test-project".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        let new_id = manager.restart(old_id, pty_size).unwrap();

        // Old agent should be gone
        assert!(manager.get(old_id).is_none());
        // New agent should exist
        assert!(manager.get(new_id).is_some());
        assert_eq!(manager.get(new_id).unwrap().name(), "test-agent");
        assert_eq!(manager.agent_count(), 1);
    }

    #[tokio::test]
    async fn test_state_counts() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        // Spawn two agents (both start in Spawning state)
        manager
            .spawn(
                "a1".into(),
                "p".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();
        manager
            .spawn(
                "a2".into(),
                "p".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        let counts = manager.state_counts();
        assert_eq!(counts.spawning, 2);
        assert_eq!(counts.running, 0);
    }

    #[tokio::test]
    async fn test_all_agent_ids_ordered() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        let id1 = manager
            .spawn(
                "a1".into(),
                "p1".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();
        let id2 = manager
            .spawn(
                "a2".into(),
                "p2".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        let ordered = manager.all_agent_ids_ordered();
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0], id1);
        assert_eq!(ordered[1], id2);
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        let id = manager
            .spawn(
                "my-agent".into(),
                "my-project".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        assert_eq!(manager.find_by_name("my-project", "my-agent"), Some(id));
        assert_eq!(manager.find_by_name("my-project", "other"), None);
        assert_eq!(manager.find_by_name("other-project", "my-agent"), None);
    }

    #[tokio::test]
    async fn test_add_empty_project() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        manager.add_empty_project("new-project").unwrap();

        let groups = manager.agents_by_project();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "new-project");
        assert!(groups[0].1.is_empty());
    }

    #[tokio::test]
    async fn test_add_duplicate_project_rejected() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        manager.add_empty_project("proj").unwrap();
        let result = manager.add_empty_project("proj");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_spawn_into_empty_project() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        manager.add_empty_project("proj").unwrap();

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };
        let id = manager
            .spawn(
                "agent1".into(),
                "proj".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        let groups = manager.agents_by_project();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "proj");
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[0].1[0], id);
    }

    #[tokio::test]
    async fn test_kill_all() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        manager
            .spawn(
                "a1".into(),
                "p".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();
        manager
            .spawn(
                "a2".into(),
                "p".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        manager.kill_all();

        // All agents should be in terminal state
        for handle in manager.agents.values() {
            assert!(handle.state().is_terminal());
        }
    }

    #[tokio::test]
    async fn test_display_order_after_restart() {
        let config = MaestroConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut manager = AgentManager::new(&config, tx);

        let pty_size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        manager
            .spawn(
                "a1".into(),
                "proj".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();
        let id2 = manager
            .spawn(
                "a2".into(),
                "proj".into(),
                "cat".into(),
                vec![],
                std::env::temp_dir(),
                HashMap::new(),
                pty_size,
            )
            .unwrap();

        // Restart a2 — should remove old and add new
        let new_id2 = manager.restart(id2, pty_size).unwrap();

        let groups = manager.agents_by_project();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 2);
        assert!(groups[0].1.contains(&new_id2));
        assert!(!groups[0].1.contains(&id2));
    }
}
