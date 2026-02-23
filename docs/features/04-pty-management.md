# Feature 04: PTY Management

## Overview

Implement the pseudo-terminal (PTY) layer that spawns child processes (Claude Code) in isolated terminal sessions and provides async read/write access. This is the most critical infrastructure feature — it's how Maestro communicates with agents. The PTY gives each agent a full terminal environment, so Claude Code's TUI rendering (colors, cursor movement, clearing) works correctly.

## Dependencies

- **Feature 01** (Project Scaffold) — Cargo.toml with `portable-pty` and `tokio`.
- **Feature 03** (Core Types & Event System) — `AppEvent::PtyOutput`, `AppEvent::PtyEof`, `AgentId`, and `EventBus::sender()`.

## Technical Specification

### Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│ Maestro Process                                     │
│                                                     │
│  PtyController                                      │
│  ├── master_pty (MasterPty)   ← read/write end      │
│  ├── read_task (spawn_blocking) → mpsc → EventBus   │
│  └── write() → master_pty.write_all()               │
│                                                     │
│  ┌───────── PTY boundary ─────────┐                 │
│  │                                │                 │
│  │  slave_pty → child process     │                 │
│  │  (claude CLI running here)     │                 │
│  │  stdin/stdout/stderr = PTY     │                 │
│  │                                │                 │
│  └────────────────────────────────┘                 │
└─────────────────────────────────────────────────────┘
```

### PTY Spawner (`src/pty/spawner.rs`)

Responsible for creating a PTY pair and launching a child process inside it.

```rust
use portable_pty::{
    native_pty_system, Child, CommandBuilder, MasterPty, PtySize, PtySystem,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use color_eyre::eyre::{Result, WrapErr};

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
        .wrap_err("Failed to open PTY pair")?;

    // Build the command
    let mut cmd = CommandBuilder::new(&config.command);
    cmd.args(&config.args);
    cmd.cwd(&config.cwd);

    // Set environment variables
    // Start with TERM so the child knows it's in a terminal
    cmd.env("TERM", "xterm-256color");
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
    let child = pair
        .slave
        .spawn_command(cmd)
        .wrap_err_with(|| {
            format!(
                "Failed to spawn '{}' in PTY (cwd: {})",
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
        rows: terminal_rows.saturating_sub(2),  // minus status bar and title
        cols: terminal_cols.saturating_sub(sidebar_width + 1), // minus sidebar + border
        pixel_width: 0,
        pixel_height: 0,
    }
}
```

### PTY Controller (`src/pty/controller.rs`)

Wraps the raw PTY master into an async-friendly interface with a background read loop.

```rust
use crate::agent::AgentId;
use crate::event::types::AppEvent;
use portable_pty::{MasterPty, PtySize};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, warn};
use color_eyre::eyre::Result;

/// Manages async read/write for a single agent's PTY.
pub struct PtyController {
    /// Agent this controller belongs to.
    agent_id: AgentId,

    /// The master end of the PTY, wrapped for thread-safe write access.
    /// Using Arc<Mutex<>> because portable-pty's Write impl is !Send
    /// and we need to write from the main tokio thread.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,

    /// The master PTY handle (for resize operations).
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,

    /// Handle to the background read task (for cancellation on shutdown).
    read_task: Option<JoinHandle<()>>,
}

impl PtyController {
    /// Create a new controller and start the background read loop.
    ///
    /// # Arguments
    /// * `agent_id` — identifies this agent in events.
    /// * `master` — the master end of the PTY.
    /// * `event_tx` — sender to the central event bus.
    pub fn new(
        agent_id: AgentId,
        master: Box<dyn MasterPty + Send>,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Self> {
        // Get a writer handle from the master
        let writer = master
            .try_clone_writer()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to clone PTY writer: {}", e))?;

        let writer = Arc::new(Mutex::new(writer));
        let master = Arc::new(Mutex::new(master));

        // Get a reader handle
        let reader = {
            let master = master.lock().unwrap();
            master
                .try_clone_reader()
                .map_err(|e| color_eyre::eyre::eyre!("Failed to clone PTY reader: {}", e))?
        };

        // Spawn the background read task
        let read_task = Self::spawn_read_task(agent_id, reader, event_tx);

        Ok(Self {
            agent_id,
            writer,
            master,
            read_task: Some(read_task),
        })
    }

    /// Spawn a blocking task that reads from the PTY and sends output events.
    fn spawn_read_task(
        agent_id: AgentId,
        mut reader: Box<dyn std::io::Read + Send>,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF — child process has exited
                        debug!("PTY EOF for agent {}", agent_id);
                        let _ = event_tx.send(AppEvent::PtyEof { agent_id });
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if event_tx
                            .send(AppEvent::PtyOutput {
                                agent_id,
                                data,
                            })
                            .is_err()
                        {
                            // Event bus is gone — app is shutting down
                            debug!("Event bus closed, PTY reader exiting for agent {}", agent_id);
                            break;
                        }
                    }
                    Err(e) => {
                        // Read error — PTY is probably closed
                        if e.kind() != std::io::ErrorKind::BrokenPipe {
                            warn!("PTY read error for agent {}: {}", agent_id, e);
                        }
                        let _ = event_tx.send(AppEvent::PtyEof { agent_id });
                        break;
                    }
                }
            }
        })
    }

    /// Write bytes to the agent's PTY (sends input to the child process).
    /// This is synchronous and fast — just writes to a pipe.
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer
            .write_all(data)
            .map_err(|e| color_eyre::eyre::eyre!("PTY write error for agent {}: {}", self.agent_id, e))?;
        writer
            .flush()
            .map_err(|e| color_eyre::eyre::eyre!("PTY flush error for agent {}: {}", self.agent_id, e))?;
        Ok(())
    }

    /// Resize the PTY. Called when the terminal window or layout changes.
    pub fn resize(&self, size: PtySize) -> Result<()> {
        let master = self.master.lock().unwrap();
        master
            .resize(size)
            .map_err(|e| color_eyre::eyre::eyre!("PTY resize error for agent {}: {}", self.agent_id, e))?;
        Ok(())
    }

    /// Get the agent ID this controller belongs to.
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Shut down the controller — abort the read task.
    pub fn shutdown(&mut self) {
        if let Some(task) = self.read_task.take() {
            task.abort();
        }
    }
}

impl Drop for PtyController {
    fn drop(&mut self) {
        self.shutdown();
    }
}
```

### Key Design Decisions

#### 1. `spawn_blocking` for PTY reads

`portable-pty` is synchronous. Using `spawn_blocking` moves the blocking `read()` call onto Tokio's blocking thread pool. Each agent gets its own blocking thread. With 15 agents, that's 15 blocking threads — well within Tokio's default pool of 512.

#### 2. `mpsc::unbounded_channel` for PTY output

Bounded channels risk backpressure blocking the read thread, which would buffer data in the kernel pipe and eventually block the child process's writes. Unbounded channels avoid this, and memory usage is bounded in practice because:
- `vt100::Parser` processes data as fast as it arrives.
- The main loop drains the channel every iteration.
- Agent output rate is human-readable (not gigabytes/sec).

#### 3. Writer is `Arc<Mutex<Box<dyn Write + Send>>>`

The writer needs to be accessible from the main thread (to forward keystrokes) while the reader runs on a blocking thread. The `Mutex` contention is negligible since writes are short and infrequent (only when the user types in Insert Mode).

#### 4. Environment variable inheritance

We explicitly inherit only safe variables (`HOME`, `USER`, `PATH`, `SHELL`, `LANG`, `LC_ALL`) rather than passing the entire environment. This prevents accidentally leaking Maestro-specific env vars to agents and gives us control over what the child sees.

The `TERM` variable is always set to `xterm-256color` to ensure Claude Code gets full color support.

#### 5. Slave PTY is dropped after spawn

The slave end of the PTY is consumed by the child process spawn. We explicitly drop our reference so the only holders are the child's stdin/stdout/stderr. When the child exits, the slave closes, causing our master read to return EOF.

### PTY Resize Flow

When the terminal window resizes or the layout changes:

1. `App` receives `AppEvent::Resize { cols, rows }`.
2. `App` recalculates the terminal pane dimensions for each visible agent.
3. For each affected agent, calls `agent_handle.resize(new_size)`.
4. `AgentHandle` calls `pty_controller.resize(new_size)`.
5. `PtyController` calls `master.resize(PtySize { ... })`.
6. Also update `vt100::Parser` dimensions: `parser.set_size(rows, cols)`.

This ensures the child process receives a `SIGWINCH` signal and can re-render at the correct size.

## Implementation Steps

1. **Implement `src/pty/spawner.rs`**
   - `SpawnConfig` struct with all fields.
   - `SpawnResult` struct returning master + child.
   - `spawn_in_pty()` function.
   - `default_pty_size()` helper.

2. **Implement `src/pty/controller.rs`**
   - `PtyController` struct with all fields.
   - `new()` — creates controller, starts read task.
   - `write()` — sends bytes to PTY.
   - `resize()` — updates PTY dimensions.
   - `shutdown()` — aborts read task.
   - `Drop` impl for cleanup.

3. **Update `src/pty/mod.rs`**
   - Re-export `PtyController`, `SpawnConfig`, `SpawnResult`, `spawn_in_pty`.

4. **Write integration test**
   - Spawn a simple command (e.g., `echo hello`) in a PTY.
   - Read output.
   - Verify EOF on process exit.

## Error Handling

| Error | Handling |
|---|---|
| PTY creation fails (`openpty`) | Return error with context. Caller (AgentManager) will report to user. |
| Command not found | `spawn_command` returns error. Include command name and cwd in error message. |
| CWD doesn't exist | `spawn_command` returns error. Include path in error message. |
| PTY read error (not BrokenPipe) | Log warning, send `PtyEof` event. Agent transitions to Errored state. |
| PTY read BrokenPipe | Normal — child process exited. Send `PtyEof` silently. |
| PTY write error | Return error to caller. In Insert Mode, the input handler should show a brief error in the status bar. |
| PTY resize error | Log warning, continue. Non-fatal — output may render oddly but won't crash. |
| Reader thread panics | Tokio catches it, logs it. Agent will appear to hang. State detector will eventually mark it Errored via process exit check. |
| Event bus channel closed | Reader exits its loop. Normal during shutdown. |

### Recovery Strategy

If a PTY controller encounters an unrecoverable error:
1. The read task exits (sends `PtyEof` if possible).
2. The state detector notices the child process has exited.
3. The agent transitions to `Errored` state.
4. The user can `r`estart the agent, which creates a fresh PTY.

## Testing Strategy

### Unit Tests

PTY operations are inherently system-level, so most tests are integration tests.

### Integration Tests (`tests/integration/pty_io.rs`)

```rust
use maestro::pty::{spawn_in_pty, SpawnConfig, PtyController};
use portable_pty::PtySize;
use std::collections::HashMap;
use tokio::sync::mpsc;

fn test_size() -> PtySize {
    PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[tokio::test]
async fn test_spawn_echo_command() {
    let config = SpawnConfig {
        command: "echo".into(),
        args: vec!["hello world".into()],
        cwd: std::env::temp_dir(),
        env: HashMap::new(),
        size: test_size(),
    };

    let result = spawn_in_pty(config).unwrap();

    // Child should exit quickly
    let mut child = result.child;
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn test_pty_controller_reads_output() {
    let config = SpawnConfig {
        command: "echo".into(),
        args: vec!["test output".into()],
        cwd: std::env::temp_dir(),
        env: HashMap::new(),
        size: test_size(),
    };

    let result = spawn_in_pty(config).unwrap();
    let agent_id = AgentId::new();
    let (tx, mut rx) = mpsc::unbounded_channel();

    let _controller = PtyController::new(agent_id, result.master, tx).unwrap();

    // Collect output events
    let mut output = Vec::new();
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(2));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(AppEvent::PtyOutput { data, .. }) => {
                        output.extend_from_slice(&data);
                    }
                    Some(AppEvent::PtyEof { .. }) => break,
                    None => break,
                    _ => {}
                }
            }
            _ = &mut timeout => {
                panic!("Timed out waiting for output");
            }
        }
    }

    let output_str = String::from_utf8_lossy(&output);
    assert!(output_str.contains("test output"));
}

#[tokio::test]
async fn test_pty_controller_write_to_cat() {
    let config = SpawnConfig {
        command: "cat".into(),
        args: vec![],
        cwd: std::env::temp_dir(),
        env: HashMap::new(),
        size: test_size(),
    };

    let result = spawn_in_pty(config).unwrap();
    let agent_id = AgentId::new();
    let (tx, mut rx) = mpsc::unbounded_channel();

    let controller = PtyController::new(agent_id, result.master, tx).unwrap();

    // Write to the PTY
    controller.write(b"hello from maestro\n").unwrap();

    // Read the echo back (cat echoes input)
    let mut output = Vec::new();
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(2));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(AppEvent::PtyOutput { data, .. }) => {
                        output.extend_from_slice(&data);
                        let s = String::from_utf8_lossy(&output);
                        if s.contains("hello from maestro") {
                            break;
                        }
                    }
                    Some(AppEvent::PtyEof { .. }) => break,
                    None => break,
                    _ => {}
                }
            }
            _ = &mut timeout => {
                panic!("Timed out waiting for echo");
            }
        }
    }

    let output_str = String::from_utf8_lossy(&output);
    assert!(output_str.contains("hello from maestro"));
}

#[tokio::test]
async fn test_pty_eof_on_process_exit() {
    let config = SpawnConfig {
        command: "true".into(),  // Exits immediately with 0
        args: vec![],
        cwd: std::env::temp_dir(),
        env: HashMap::new(),
        size: test_size(),
    };

    let result = spawn_in_pty(config).unwrap();
    let agent_id = AgentId::new();
    let (tx, mut rx) = mpsc::unbounded_channel();

    let _controller = PtyController::new(agent_id, result.master, tx).unwrap();

    // Should receive PtyEof
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(2));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(AppEvent::PtyEof { agent_id: id }) => {
                        assert_eq!(id, agent_id);
                        return; // Success
                    }
                    Some(_) => continue,
                    None => panic!("Channel closed without PtyEof"),
                }
            }
            _ = &mut timeout => {
                panic!("Timed out waiting for PtyEof");
            }
        }
    }
}

#[tokio::test]
async fn test_spawn_nonexistent_command_fails() {
    let config = SpawnConfig {
        command: "nonexistent_command_xyz_123".into(),
        args: vec![],
        cwd: std::env::temp_dir(),
        env: HashMap::new(),
        size: test_size(),
    };

    let result = spawn_in_pty(config);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pty_resize() {
    let config = SpawnConfig {
        command: "cat".into(),
        args: vec![],
        cwd: std::env::temp_dir(),
        env: HashMap::new(),
        size: test_size(),
    };

    let result = spawn_in_pty(config).unwrap();
    let agent_id = AgentId::new();
    let (tx, _rx) = mpsc::unbounded_channel();

    let controller = PtyController::new(agent_id, result.master, tx).unwrap();

    // Resize should not error
    let new_size = PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    };
    controller.resize(new_size).unwrap();
}
```

### Manual Verification

- Spawn `bash` in a PTY, type commands, see output.
- Spawn `claude` in a PTY, verify it starts correctly.
- Resize terminal while an agent is running — no crash or garbled output.

## Acceptance Criteria

- [ ] `spawn_in_pty()` successfully spawns `echo`, `cat`, `bash`, and `claude` commands.
- [ ] `PtyController` reads output from spawned processes and sends `PtyOutput` events.
- [ ] `PtyController::write()` sends input to the child process.
- [ ] `PtyEof` event is sent when the child process exits.
- [ ] PTY resize works without error.
- [ ] Environment variables `TERM`, `HOME`, `USER`, `PATH`, `SHELL` are set on the child.
- [ ] User-specified env vars override inherited ones.
- [ ] Non-existent commands return a descriptive error.
- [ ] Controller cleanup (Drop) aborts the read task.
- [ ] All integration tests pass.
- [ ] No resource leaks: after dropping controller, no lingering threads or file descriptors.
