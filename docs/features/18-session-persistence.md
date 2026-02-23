# Feature 18: Session Persistence (v0.3)

## Overview

Save and restore Maestro sessions across restarts. When Maestro exits, it serializes the current state (which agents are running, their scrollback, layout configuration) to disk. On next startup, it offers to restore the previous session, re-spawning agents and loading scrollback history.

## Dependencies

- **Feature 02** (Configuration System) — session file location.
- **Feature 06** (Agent Lifecycle) — agent spawning from saved state.
- **Feature 15** (Scrollback) — scrollback buffer persistence.

## Technical Specification

### Session File Format

Session data is stored as a TOML file at `~/.local/share/maestro/sessions/last_session.toml`.

```rust
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};

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

/// A saved agent's configuration (not its process — processes can't be serialized).
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
    pub env: std::collections::HashMap<String, String>,
    /// Whether this agent was running at save time.
    pub was_running: bool,
    /// Scrollback file path (relative to session dir).
    pub scrollback_file: Option<String>,
    /// Last known state.
    pub last_state: String,
}
```

### Scrollback Persistence

Each agent's scrollback is saved to a separate file to keep the session TOML small:

```
~/.local/share/maestro/sessions/
├── last_session.toml           # Session metadata
├── scrollback/
│   ├── myapp_backend-refactor.raw  # Raw PTY bytes
│   ├── myapp_test-runner.raw
│   └── webui_frontend-fix.raw
```

Scrollback files contain raw PTY bytes. On restore, these are fed through a `vt100::Parser` to reconstruct the screen.

### Session Manager

```rust
use color_eyre::eyre::Result;
use std::path::{Path, PathBuf};

pub struct SessionManager {
    session_dir: PathBuf,
}

impl SessionManager {
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
```

### Session Save Flow

1. User quits Maestro (or Maestro crashes — see auto-save).
2. For each running agent:
   a. Save scrollback buffer to `scrollback/<project>_<name>.raw`.
   b. Record agent metadata in `SavedAgent`.
3. Serialize `SessionSnapshot` to `last_session.toml`.

### Session Restore Flow

1. Maestro starts.
2. Check for `last_session.toml`.
3. If found, prompt user: "Restore previous session? (y/n/d[elete])".
4. If "y":
   a. Load session snapshot.
   b. For each `SavedAgent` where `was_running = true`:
      - Spawn a new agent with the same parameters.
      - Load scrollback bytes and feed to the new `vt100::Parser`.
   c. Restore the layout mode.
   d. Delete the saved session.
5. If "n": Start fresh, keep the session file for later.
6. If "d": Delete the session file and start fresh.

### Auto-Save (Crash Recovery)

For crash recovery, Maestro periodically auto-saves the session:

```rust
// In the main event loop, every 60 seconds:
if last_autosave.elapsed() > Duration::from_secs(60) {
    if let Err(e) = session_manager.save(&build_snapshot(&app)) {
        tracing::warn!("Auto-save failed: {}", e);
    }
    last_autosave = Instant::now();
}
```

### Limitations

1. **Processes cannot be transferred**: Restored agents are freshly spawned. They don't have the previous conversation context (Claude Code doesn't support session resumption via PTY).
2. **Scrollback is visual only**: The restored scrollback shows what was on screen but Claude Code doesn't know about it.
3. **Claude Code session persistence**: If Claude Code supports `--resume` or session files in the future, we should integrate with that. For now, restoration means "start a new conversation with the same configuration, but show the old output."

### Configuration

```toml
[session]
# Enable session persistence
enabled = true
# Auto-save interval in seconds (0 to disable)
autosave_interval_secs = 60
# Maximum scrollback bytes to save per agent (default 5MB)
max_scrollback_bytes = 5242880
```

## Implementation Steps

1. **Define session data types**
   - `SessionSnapshot`, `SavedAgent` structs in `src/session/mod.rs`.

2. **Implement `SessionManager`**
   - `save()`, `load()`, `clear()` methods.
   - Scrollback file I/O.

3. **Implement save flow**
   - Build `SessionSnapshot` from current `App` state.
   - Call on quit and periodically (auto-save).

4. **Implement restore flow**
   - Check for saved session at startup.
   - Prompt user for restore decision.
   - Re-spawn agents and load scrollback.

5. **Add session config to settings**

6. **Handle edge cases**
   - Stale session from a different config.
   - Corrupt session file.
   - Missing scrollback files.

## Error Handling

| Scenario | Handling |
|---|---|
| Session file corrupt | Log warning, offer to delete. Start fresh. |
| Scrollback file missing | Skip scrollback restore for that agent. Still spawn the agent. |
| Disk full on save | Log error. Session not saved. Non-fatal. |
| Version mismatch | If session version doesn't match current Maestro version, warn and offer to delete. |
| Agent spawn fails on restore | Log error, skip that agent. Continue restoring others. |

## Testing Strategy

### Unit Tests

```rust
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
        }],
        config_path: None,
    };

    manager.save(&snapshot).unwrap();
    let loaded = manager.load().unwrap().unwrap();
    assert_eq!(loaded.agents.len(), 1);
    assert_eq!(loaded.agents[0].name, "test");
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
```

## Acceptance Criteria

- [ ] Session is saved to disk on quit (agents, layout, scrollback).
- [ ] Session is auto-saved periodically (configurable interval).
- [ ] On startup, user is prompted to restore if a saved session exists.
- [ ] Restored agents are spawned with the same configuration.
- [ ] Restored scrollback is displayed in the terminal pane.
- [ ] Layout mode is restored.
- [ ] Corrupt session files are handled gracefully (warning, not crash).
- [ ] Session persistence can be disabled via config.
- [ ] Session file is deleted after successful restore.
- [ ] Auto-save doesn't block the event loop.
