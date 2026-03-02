//! PTY spawner — creates new pseudo-terminal processes.
//!
//! Uses `portable-pty` to spawn child processes in a PTY with
//! proper environment setup.

use color_eyre::eyre::{eyre, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Configuration for spawning a new process in a PTY.
pub struct SpawnConfig {
    /// The command to execute (e.g., "claude").
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Working directory for the child process.
    pub cwd: PathBuf,
    /// Additional environment variables to set.
    pub env: HashMap<String, String>,
    /// Initial PTY dimensions.
    pub size: PtySize,
}

/// Result of successfully spawning a process.
pub struct SpawnResult {
    /// The master end of the PTY (for reading/writing).
    pub master: Box<dyn MasterPty + Send>,
    /// The child process handle (for monitoring exit status).
    pub child: Box<dyn Child + Send + Sync>,
}

/// Spawn a child process inside a new PTY.
pub fn spawn_in_pty(config: SpawnConfig) -> Result<SpawnResult> {
    let pty_system = native_pty_system();

    // Create PTY pair with specified dimensions
    let pair = pty_system
        .openpty(config.size)
        .map_err(|e| eyre!("Failed to open PTY pair: {e}"))?;

    // Build the command
    let mut cmd = CommandBuilder::new(&config.command);
    cmd.args(&config.args);
    cmd.cwd(&config.cwd);

    // Set environment variables
    // Start with TERM so the child knows it's in a terminal
    cmd.env("TERM", "xterm-256color");

    // Strip tmux-related variables to prevent the outer tmux session
    // from leaking into agents. Without this, agents spawned inside
    // tmux connect to the parent's tmux server, causing session bleed.
    for key in &["TMUX", "TMUX_PANE", "TMUX_PLUGIN_MANAGER_PATH"] {
        cmd.env_remove(*key);
    }

    // Give each agent its own tmux socket directory so that any tmux
    // servers created by the agent are fully isolated from other agents.
    let tmux_tmpdir = std::env::temp_dir().join(format!("maestro-tmux-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmux_tmpdir).ok();
    cmd.env("TMUX_TMPDIR", &tmux_tmpdir);

    // Inherit selected env vars from parent
    for key in &["HOME", "USER", "PATH", "SHELL", "LANG", "LC_ALL"] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    // Apply user-specified env vars (override inherited ones)
    for (key, val) in &config.env {
        cmd.env(key, val);
    }

    // Spawn the child in the slave PTY
    let child = pair.slave.spawn_command(cmd).map_err(|e| {
        eyre!(
            "Failed to spawn '{}' in PTY (cwd: {}): {e}",
            config.command,
            config.cwd.display()
        )
    })?;

    // Drop the slave — we only need the master end.
    // The child holds the slave fd via the spawned process.
    drop(pair.slave);

    Ok(SpawnResult {
        master: pair.master,
        child,
    })
}

/// Create default PTY dimensions based on a Ratatui `Rect`.
/// Subtracts space for the sidebar and status bar.
pub fn default_pty_size(terminal_cols: u16, terminal_rows: u16, sidebar_width: u16) -> PtySize {
    PtySize {
        rows: terminal_rows.saturating_sub(2), // minus status bar and title
        cols: terminal_cols.saturating_sub(sidebar_width + 1), // minus sidebar + border
        pixel_width: 0,
        pixel_height: 0,
    }
}
