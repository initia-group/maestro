# Feature 01: Project Scaffold & Build System

## Overview

Set up the Rust project structure, Cargo.toml with all dependencies, module hierarchy, and minimal entry point stubs. This is the foundation that every other feature builds upon. After this feature is complete, `cargo build` should compile successfully with an empty TUI that enters/exits alternate screen mode.

## Dependencies

- **None** — this is the first feature to implement.

## Technical Specification

### Cargo.toml

The workspace root contains a single binary crate. All dependencies are declared upfront (even those used by later features) to catch version conflicts early.

```toml
[package]
name = "maestro"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
description = "TUI agent dashboard for Claude Code"
license = "MIT"
repository = "https://github.com/<org>/maestro"

[dependencies]
# TUI
ratatui = { version = "0.29", features = ["crossterm"] }
crossterm = "0.28"
tui-term = "0.3"
vt100 = "0.15"

# PTY
portable-pty = "0.9"

# Async
tokio = { version = "1", features = ["full"] }
futures = "0.3"

# Config & Serialization
serde = { version = "1", features = ["derive"] }
toml = "0.8"
dirs = "5"

# CLI & Error Handling
clap = { version = "4", features = ["derive"] }
color-eyre = "0.6"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"

# Utility
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
fuzzy-matcher = "0.3"

[dev-dependencies]
insta = "1"          # Snapshot testing for UI
tempfile = "3"       # Temp dirs for config tests
```

> **Note**: Exact versions for `ratatui`, `tui-term`, `vt100`, and `crossterm` must be validated for compatibility before implementation. See the compatibility matrix in the project root. If `tui-term 0.3` requires a different `ratatui` version, adjust accordingly.

### Module Hierarchy

Every module is created as a directory with `mod.rs` to allow future expansion. Each `mod.rs` re-exports the public API of its child modules.

```
src/
├── main.rs                   # Binary entry point
├── lib.rs                    # Library root — all module declarations
├── app.rs                    # (stub) App struct placeholder
│
├── agent/
│   ├── mod.rs                # pub mod state, manager, handle, detector;
│   ├── state.rs              # (stub)
│   ├── manager.rs            # (stub)
│   ├── handle.rs             # (stub)
│   └── detector.rs           # (stub)
│
├── pty/
│   ├── mod.rs                # pub mod controller, spawner;
│   ├── controller.rs         # (stub)
│   └── spawner.rs            # (stub)
│
├── project/
│   ├── mod.rs                # pub mod config;
│   └── config.rs             # (stub)
│
├── ui/
│   ├── mod.rs                # pub mod layout, sidebar, terminal_pane, status_bar, theme, command_palette;
│   ├── layout.rs             # (stub)
│   ├── sidebar.rs            # (stub)
│   ├── terminal_pane.rs      # (stub)
│   ├── status_bar.rs         # (stub)
│   ├── command_palette.rs    # (stub)
│   └── theme.rs              # (stub)
│
├── input/
│   ├── mod.rs                # pub mod handler, mode, action;
│   ├── handler.rs            # (stub)
│   ├── mode.rs               # (stub)
│   └── action.rs             # (stub)
│
├── event/
│   ├── mod.rs                # pub mod bus, types;
│   ├── bus.rs                # (stub)
│   └── types.rs              # (stub)
│
└── config/
    ├── mod.rs                # pub mod settings, loader;
    ├── settings.rs           # (stub)
    └── loader.rs             # (stub)
```

Additionally, create:
```
config/
└── default.toml              # Default configuration shipped with binary
```

### Stub Content Pattern

Every stub file should contain a doc comment explaining its purpose and a minimal compilable placeholder:

```rust
//! Agent state machine definitions.
//!
//! Defines the `AgentState` enum and state transition logic.
//! See Feature 05 (Agent State Machine & Detection) for full implementation.

// TODO: Feature 05 — implement AgentState, PromptType, transitions
```

### main.rs (Minimal Working Binary)

The entry point should:
1. Initialize `color-eyre` for error reporting.
2. Parse CLI args via `clap` (just `--version` and `--config` for now).
3. Enter crossterm alternate screen + raw mode.
4. Create a `ratatui::Terminal`.
5. Draw a single frame with a centered "Maestro v0.1.0 — Loading..." message.
6. Wait for any key press.
7. Restore terminal (leave alternate screen, disable raw mode).

This proves the full TUI pipeline works end-to-end.

```rust
use clap::Parser;
use color_eyre::eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use std::io::stdout;

#[derive(Parser)]
#[command(name = "maestro", version, about = "TUI agent dashboard for Claude Code")]
struct Cli {
    /// Path to config file (default: ~/.config/maestro/config.toml)
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let _cli = Cli::parse();

    // Terminal setup
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Draw placeholder
    terminal.draw(|frame| {
        let area = frame.area();
        let text = Text::from("Maestro v0.1.0 — Loading...\n\nPress any key to exit.")
            .centered();
        frame.render_widget(
            ratatui::widgets::Paragraph::new(text)
                .alignment(Alignment::Center),
            area,
        );
    })?;

    // Wait for key
    loop {
        if let Event::Key(_) = event::read()? {
            break;
        }
    }

    // Terminal teardown
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
```

### lib.rs

```rust
//! Maestro — TUI agent dashboard for Claude Code.

pub mod agent;
pub mod app;
pub mod config;
pub mod event;
pub mod input;
pub mod project;
pub mod pty;
pub mod ui;
```

### Default Configuration File

Create `config/default.toml` with sensible defaults (the actual struct deserialization is Feature 02):

```toml
[global]
claude_binary = "claude"
default_shell = "/bin/zsh"
max_agents = 15
log_dir = "~/.local/share/maestro/logs"
state_check_interval_ms = 250
idle_timeout_secs = 3

[ui]
fps = 30
sidebar_width = 28
default_layout = "single"
show_uptime = true

[ui.theme]
name = "default"
```

## Implementation Steps

1. **Create project directory structure**
   - Run `cargo init --name maestro` in the project root.
   - Replace generated `Cargo.toml` with the full dependency list above.

2. **Create all directories**
   - `src/agent/`, `src/pty/`, `src/project/`, `src/ui/`, `src/input/`, `src/event/`, `src/config/`
   - `config/`

3. **Write all stub files**
   - Each `mod.rs` declares its child modules with `pub mod`.
   - Each leaf file contains a doc comment and a TODO marker.
   - `src/app.rs` contains an empty `pub struct App;` placeholder.

4. **Write `src/lib.rs`**
   - Declare all top-level modules.

5. **Write `src/main.rs`**
   - Implement the minimal working binary described above.

6. **Write `config/default.toml`**
   - Copy the default config content.

7. **Verify compilation**
   - Run `cargo build` — must compile with zero errors.
   - Run `cargo clippy` — must pass with zero warnings.
   - Run `cargo run` — must show the placeholder screen and exit on key press.

## Error Handling

At this stage, error handling is minimal:
- `color-eyre` is initialized for panic/error reporting.
- Terminal setup/teardown is wrapped in `Result` and uses `?` propagation.
- A panic hook should ensure terminal is restored even on crash:

```rust
// In main(), before terminal setup:
let original_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |panic_info| {
    let _ = disable_raw_mode();
    let _ = stdout().execute(LeaveAlternateScreen);
    original_hook(panic_info);
}));
```

## Testing Strategy

### Unit Tests
- None needed at this stage (stubs only).

### Build Verification
- `cargo build` succeeds.
- `cargo clippy -- -D warnings` passes.
- `cargo test` runs (no tests yet, but test harness initializes).

### Manual Verification
- Run `cargo run` — see the "Loading..." screen.
- Press any key — terminal returns to normal.
- Resize terminal while on the screen — no crash.
- Press `Ctrl+C` — terminal returns to normal (panic hook works).

## Acceptance Criteria

- [ ] `Cargo.toml` contains all dependencies with pinned versions.
- [ ] All 28 source files exist with proper module declarations.
- [ ] `cargo build` compiles with zero errors.
- [ ] `cargo clippy -- -D warnings` passes.
- [ ] `cargo run` shows a placeholder TUI screen and exits cleanly on key press.
- [ ] Terminal is restored to normal state after exit (including on panic).
- [ ] `config/default.toml` exists with default values.
- [ ] Every stub file has a doc comment explaining its purpose.
