//! Session persistence — save and restore Maestro sessions across restarts.
//!
//! Serializes agent configurations, layout mode, and scrollback buffers to disk
//! so that a previous session can be restored on the next startup.

use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A serializable snapshot of the Maestro session.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// When this session was saved.
    pub saved_at: DateTime<Utc>,
    /// Maestro version that created this snapshot.
    pub version: String,
    /// The layout mode that was active.
    pub layout: String,
    /// Saved agent sessions.
    pub agents: Vec<SavedAgent>,
    /// Config file path used for this session.
    pub config_path: Option<PathBuf>,
}

/// A saved agent's configuration (not its process -- processes cannot be serialized).
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedAgent {
    /// Agent display name.
    pub name: String,
    /// Project name.
    pub project_name: String,
    /// Command used to spawn.
    pub command: String,
    /// CLI arguments.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Additional environment variables.
    pub env: HashMap<String, String>,
    /// Whether this agent was running at save time.
    pub was_running: bool,
    /// Scrollback file path (relative to session dir).
    pub scrollback_file: Option<String>,
    /// Last known state.
    pub last_state: String,
    /// Claude Code session ID for resuming specific conversations.
    pub session_id: Option<String>,
}

/// Manages saving and loading session data on disk.
pub struct SessionManager {
    session_dir: PathBuf,
}

impl SessionManager {
    /// Create a new session manager rooted at `data_dir/sessions`.
    pub fn new(data_dir: &Path) -> Self {
        let session_dir = data_dir.join("sessions");
        Self { session_dir }
    }

    /// Save the current session to disk.
    pub fn save(&self, snapshot: &SessionSnapshot) -> Result<()> {
        std::fs::create_dir_all(&self.session_dir)?;
        std::fs::create_dir_all(self.session_dir.join("scrollback"))?;

        let session_path = self.session_dir.join("last_session.toml");
        let toml_str = toml::to_string_pretty(snapshot)?;
        std::fs::write(&session_path, toml_str)?;

        tracing::info!("Session saved to {}", session_path.display());
        Ok(())
    }

    /// Save an agent's scrollback buffer to disk.
    pub fn save_scrollback(
        &self,
        project_name: &str,
        agent_name: &str,
        raw_bytes: &[u8],
    ) -> Result<String> {
        std::fs::create_dir_all(self.session_dir.join("scrollback"))?;
        let filename = format!("{}_{}.raw", project_name, agent_name);
        let path = self.session_dir.join("scrollback").join(&filename);
        std::fs::write(&path, raw_bytes)?;
        Ok(filename)
    }

    /// Load the previous session from disk.
    pub fn load(&self) -> Result<Option<SessionSnapshot>> {
        let session_path = self.session_dir.join("last_session.toml");
        if !session_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&session_path)?;
        let snapshot: SessionSnapshot = toml::from_str(&content)?;
        Ok(Some(snapshot))
    }

    /// Load an agent's scrollback from disk.
    pub fn load_scrollback(&self, filename: &str) -> Result<Vec<u8>> {
        let path = self.session_dir.join("scrollback").join(filename);
        Ok(std::fs::read(&path)?)
    }

    /// Delete the saved session (after successful restore or explicit clear).
    pub fn clear(&self) -> Result<()> {
        let session_path = self.session_dir.join("last_session.toml");
        if session_path.exists() {
            std::fs::remove_file(&session_path)?;
        }
        let scrollback_dir = self.session_dir.join("scrollback");
        if scrollback_dir.exists() {
            std::fs::remove_dir_all(&scrollback_dir)?;
        }
        Ok(())
    }

    /// Check if a previous session exists.
    pub fn has_saved_session(&self) -> bool {
        self.session_dir.join("last_session.toml").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());

        let snapshot = SessionSnapshot {
            saved_at: Utc::now(),
            version: "0.3.0".into(),
            layout: "single".into(),
            agents: vec![SavedAgent {
                name: "test".into(),
                project_name: "proj".into(),
                command: "echo".into(),
                args: vec!["hello".into()],
                cwd: "/tmp".into(),
                env: HashMap::new(),
                was_running: true,
                scrollback_file: None,
                last_state: "running".into(),
                session_id: None,
            }],
            config_path: None,
        };

        manager.save(&snapshot).unwrap();
        let loaded = manager.load().unwrap().unwrap();
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.agents[0].name, "test");
        assert_eq!(loaded.agents[0].project_name, "proj");
        assert_eq!(loaded.agents[0].command, "echo");
        assert_eq!(loaded.agents[0].args, vec!["hello".to_string()]);
        assert!(loaded.agents[0].was_running);
        assert_eq!(loaded.agents[0].last_state, "running");
        assert_eq!(loaded.version, "0.3.0");
        assert_eq!(loaded.layout, "single");
    }

    #[test]
    fn test_scrollback_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());

        let data = b"hello world\x1b[32mgreen\x1b[0m";
        let filename = manager.save_scrollback("proj", "agent", data).unwrap();
        let loaded = manager.load_scrollback(&filename).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_no_saved_session() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());
        assert!(!manager.has_saved_session());
        assert!(manager.load().unwrap().is_none());
    }

    #[test]
    fn test_clear_removes_session() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());

        let snapshot = SessionSnapshot {
            saved_at: Utc::now(),
            version: "0.3.0".into(),
            layout: "single".into(),
            agents: vec![],
            config_path: None,
        };
        manager.save(&snapshot).unwrap();
        assert!(manager.has_saved_session());

        // Save some scrollback too
        manager.save_scrollback("proj", "agent", b"data").unwrap();

        manager.clear().unwrap();
        assert!(!manager.has_saved_session());
        assert!(manager.load().unwrap().is_none());
    }

    #[test]
    fn test_clear_nonexistent_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());
        // Should not error when nothing exists.
        manager.clear().unwrap();
    }

    #[test]
    fn test_save_multiple_agents() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());

        let mut env = HashMap::new();
        env.insert("RUST_LOG".into(), "debug".into());

        let snapshot = SessionSnapshot {
            saved_at: Utc::now(),
            version: "0.3.0".into(),
            layout: "split-h".into(),
            agents: vec![
                SavedAgent {
                    name: "backend".into(),
                    project_name: "myapp".into(),
                    command: "claude".into(),
                    args: vec!["--model".into(), "opus".into()],
                    cwd: "/tmp/myapp".into(),
                    env: env.clone(),
                    was_running: true,
                    scrollback_file: Some("myapp_backend.raw".into()),
                    last_state: "running".into(),
                    session_id: Some("550e8400-e29b-41d4-a716-446655440000".into()),
                },
                SavedAgent {
                    name: "frontend".into(),
                    project_name: "myapp".into(),
                    command: "claude".into(),
                    args: vec![],
                    cwd: "/tmp/myapp/web".into(),
                    env: HashMap::new(),
                    was_running: false,
                    scrollback_file: None,
                    last_state: "completed".into(),
                    session_id: None,
                },
            ],
            config_path: Some("/home/user/.config/maestro/config.toml".into()),
        };

        manager.save(&snapshot).unwrap();
        let loaded = manager.load().unwrap().unwrap();
        assert_eq!(loaded.agents.len(), 2);
        assert_eq!(loaded.agents[0].name, "backend");
        assert_eq!(loaded.agents[0].env.get("RUST_LOG").unwrap(), "debug");
        assert_eq!(loaded.agents[1].name, "frontend");
        assert!(!loaded.agents[1].was_running);
        assert_eq!(loaded.layout, "split-h");
        assert!(loaded.config_path.is_some());
    }

    #[test]
    fn test_scrollback_multiple_agents() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());

        let data1 = b"agent1 output";
        let data2 = b"agent2 output with \x1b[31mcolor\x1b[0m";

        let f1 = manager.save_scrollback("proj", "agent1", data1).unwrap();
        let f2 = manager.save_scrollback("proj", "agent2", data2).unwrap();

        assert_eq!(f1, "proj_agent1.raw");
        assert_eq!(f2, "proj_agent2.raw");

        assert_eq!(manager.load_scrollback(&f1).unwrap(), data1);
        assert_eq!(manager.load_scrollback(&f2).unwrap(), data2);
    }

    #[test]
    fn test_load_scrollback_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path());
        let result = manager.load_scrollback("nonexistent.raw");
        assert!(result.is_err());
    }
}
