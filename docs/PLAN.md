# Maestro - TUI Agent Dashboard for Claude Code

## Context

Managing multiple Claude Code agents across projects currently means juggling terminal tabs manually - opening tabs, cd-ing to projects, running `claude`, and mentally tracking which tab is which agent doing what task. This becomes unmanageable at 5-15 agents across 3-5 projects.

**Maestro** is a purpose-built Rust TUI dashboard that replaces this workflow. It provides a single-window "mission control" view where you can see all your agents, their status at a glance, and interact with any of them - all with vim-style keyboard navigation.

---

## Specifications

| Attribute | Value |
|-----------|-------|
| **Language** | Rust |
| **TUI Framework** | Ratatui 0.29 + crossterm 0.28 |
| **Terminal Backend** | portable-pty (PTY management) + vt100 (virtual terminal) + tui-term (rendering) |
| **Async Runtime** | Tokio |
| **Config Format** | TOML at `~/.config/maestro/config.toml` |
| **Log Location** | `~/.local/share/maestro/logs/` |
| **Target Scale** | 5-15 concurrent Claude Code agents across 3-5 projects |
| **Agent Type** | Claude Code CLI (`claude` command) |
| **Interaction** | Full interactive PTY (send messages, approve tools, provide input) |
| **Primary Feature** | Agent status at a glance |

---

## UI Layout

### Default View (sidebar + single agent focus)

```
+--[Maestro]-----------------------------------------------------------+
|                                                                       |
| PROJECTS           | Agent: backend-refactor @ myapp            [R]  |
|                    | ───────────────────────────────────────────────  |
| ▼ myapp (3)        |                                                 |
|   ● backend-refac  |  $ claude                                       |
|   ? test-runner    |  I'll start by reviewing the auth module...     |
|   - docs-writer    |                                                 |
|                    |  Allow Edit to src/auth.rs? [Y/n]               |
| ▼ webui (2)        |  _                                              |
|   ● frontend-fix   |                                                 |
|   ✓ api-tests      |                                                 |
|                    |                                                 |
| ▸ infra (1)        |                                                 |
|   ! deploy-staging |                                                 |
|                    |                                                 |
|--------------------+-------------------------------------------------|
| ● 2 running  ? 1 waiting  - 1 idle  ✓ 1 done  ! 1 err  -- NORMAL -- |
+----------------------------------------------------------------------+
```

**Components:**
- **Left sidebar** (~28 cols): Project tree with agents. Each agent shows a colored status indicator. Selected agent is highlighted.
- **Main panel**: Full interactive terminal output of the selected agent. Renders Claude Code's complete TUI via vt100 + tui-term.
- **Status bar** (bottom): Aggregate counts of agent states + current input mode + keybinding hints.

### Split View (v0.2 - horizontal split, 2 agents)

```
+--[Maestro]-----------------------------------------------------------+
| PROJECTS           | [backend-refactor @ myapp]                 [R]  |
|                    | Refactoring auth module...                       |
| ▼ myapp (3)        | Allow Bash(cargo test)? [Y/n] _                  |
|   ● backend-refac  |------------------------------------------------ |
|   ? test-runner    | [test-runner @ myapp]                       [?]  |
|   - docs-writer    | Waiting for your input...                        |
|                    | > Which test suite? _                            |
|--------------------+-------------------------------------------------|
| ● 2 running  ? 1 waiting  -- INSERT (backend-refactor) --            |
+----------------------------------------------------------------------+
```

### Grid View (v0.2 - 2x2, 4 agents)

```
+--[Maestro]-----------------------------------------------------------+
| PROJECTS           | [backend-refactor] [R]   | [test-runner]    [?]  |
|                    | Refactoring auth...      | Waiting for input     |
| ▼ myapp            | Allow Edit? [Y/n] _      | > Which tests? _      |
|   ● backend-refac  |--------------------------|---------------------- |
|   ? test-runner    | [frontend-fix]     [R]   | [deploy-staging] [!]  |
|                    | npm run build...         | Error: timeout        |
|   ● frontend-fix   | Compiling TS...          | at deploy.sh:42       |
|--------------------+--------------------------+---------------------- |
| ● 2 running  ? 1 waiting  ! 1 errored  -- NORMAL --                  |
+----------------------------------------------------------------------+
```

### Command Palette (triggered by `:` or `Ctrl+p`)

```
              +--[Command Palette]---------------------------+
              |  > spawn backend auth-fix myapp              |
              |  ------------------------------------------- |
              |    spawn <template> <name> <project>         |
              |    kill <agent-name>                         |
              |    restart <agent-name>                      |
              |    focus <agent-name>                        |
              |    split horizontal|vertical                 |
              |    project add <name> <path>                 |
              |    config reload                             |
              +----------------------------------------------+
```

### Status Indicators

| Symbol | Color        | State           | Meaning |
|--------|-------------|-----------------|---------|
| `●`    | Green       | Running         | Agent actively producing output |
| `?`    | Yellow      | WaitingForInput | Agent needs user action (tool approval, question, input prompt) |
| `-`    | Gray        | Idle            | No output for N seconds |
| `✓`    | Bright Green| Completed       | Agent process exited successfully |
| `!`    | Red         | Errored         | Agent process exited with error |

---

## Keyboard Navigation

### Normal Mode (default - navigating sidebar/UI)

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection down/up in sidebar |
| `J` / `K` | Jump to next/prev project group |
| `Enter` or `i` | Enter Insert Mode (interact with selected agent) |
| `n` | Spawn new agent |
| `d` | Kill selected agent (with confirmation) |
| `r` | Restart selected agent |
| `1`-`9` | Jump to agent by index |
| `:` / `Ctrl+p` | Open command palette |
| `s` | Horizontal split (v0.2) |
| `v` | Vertical split (v0.2) |
| `Tab` | Cycle focus between split panes (v0.2) |
| `Ctrl+w` | Close current split (v0.2) |
| `/` | Search agent output (v0.2) |
| `?` | Toggle help overlay |
| `q` | Quit (with confirmation if agents running) |

### Insert Mode (typing into agent terminal)

| Key | Action |
|-----|--------|
| All keys | Forwarded to the agent's PTY |
| `Esc` | Return to Normal Mode |
| `Ctrl+\` | Force return to Normal Mode (escape hatch) |

Mode indicator in status bar: `-- NORMAL --` or `-- INSERT (agent-name) --`

---

## Architecture

### High-Level Architecture

```
main.rs ──→ App (app.rs)
              ├── AgentManager ──→ AgentHandle[]
              │                       ├── PtyController (async read/write)
              │                       ├── vt100::Parser (virtual screen)
              │                       ├── AgentState
              │                       └── Metadata (name, project, uptime)
              │
              ├── UI Renderer
              │    ├── sidebar.rs (project tree + status indicators)
              │    ├── terminal_pane.rs (tui-term widget)
              │    └── status_bar.rs (aggregate counts + mode)
              │
              ├── InputHandler (mode-aware key dispatch)
              │    ├── Normal Mode → Actions (navigate, spawn, kill, etc.)
              │    └── Insert Mode → Raw bytes to PTY
              │
              └── EventBus (tokio::select! multiplexer)
                   ├── PTY output channels (per-agent mpsc)
                   ├── Crossterm keyboard events
                   ├── Tick timer (state detection, every 250ms)
                   └── Render timer (30 FPS)
```

### Terminal-in-Terminal Rendering

This is the core technical approach. Claude Code runs as a full TUI application, and Maestro must render its output inside a Ratatui widget:

1. **Claude Code** runs inside a PTY (pseudo-terminal). Its ANSI escape sequences go to the PTY slave fd, completely isolated from Maestro's host terminal.
2. **PtyController** reads raw bytes from the PTY master fd via `tokio::task::spawn_blocking` and sends them through an `mpsc` channel.
3. **vt100::Parser** processes those bytes into a virtual `Screen` - a grid of cells with characters and attributes. This handles all ANSI cursor movement, colors, clearing, scrolling, etc.
4. **tui-term** takes the `vt100::Screen` and renders it as a Ratatui widget, mapping each `vt100::Cell` to a `ratatui::buffer::Cell`.

```
Claude Code → PTY slave → PTY master → PtyController → vt100::Parser → tui-term → Ratatui frame
```

### Async PTY I/O

`portable-pty` is synchronous. Each agent gets a dedicated blocking read task:

```rust
// Simplified - each agent has one of these
let read_task = tokio::task::spawn_blocking(move || {
    let mut buf = [0u8; 4096];
    loop {
        match pty_reader.read(&mut buf) {
            Ok(0) => break,           // EOF - process exited
            Ok(n) => { output_tx.blocking_send(buf[..n].to_vec()); }
            Err(_) => break,
        }
    }
});
```

### PTY Resize

When the terminal window or split layout changes, PTY dimensions must be updated:

```rust
fn resize_agent(&mut self, agent_id: &AgentId, rows: u16, cols: u16) {
    handle.master_pty.resize(PtySize { rows, cols, .. });
    handle.parser.set_size(rows, cols);
}
```

### Main Event Loop

```rust
loop {
    tokio::select! {
        // PTY output from any agent
        Some((agent_id, bytes)) = pty_events.recv() => {
            agent.parser.process(&bytes);
            agent.dirty = true;
        }
        // Render at 30 FPS
        _ = render_interval.tick() => {
            terminal.draw(|frame| app.render(frame));
        }
        // State detection at 250ms
        _ = state_check.tick() => {
            for agent in agents {
                detector.detect(agent.screen(), agent.child.try_wait());
            }
        }
        // Keyboard input
        Some(key) = crossterm_events.recv() => {
            let action = input_handler.handle(key, &mode);
            app.update(action);
        }
    }
}
```

---

## Agent State Machine

### States

```
Spawning → Running ⇄ WaitingForInput
              ↓              ↓
            Idle ←───────────┘
              ↓
    Completed | Errored
```

```rust
pub enum AgentState {
    Spawning,
    Running { since: DateTime<Utc> },
    WaitingForInput { prompt_type: PromptType, since: DateTime<Utc> },
    Idle { since: DateTime<Utc> },
    Completed { at: DateTime<Utc>, exit_code: Option<i32> },
    Errored { at: DateTime<Utc>, exit_code: Option<i32>, error_hint: Option<String> },
}

pub enum PromptType {
    ToolApproval { tool_name: String },
    Question,
    InputPrompt,
    Unknown,
}
```

### State Detection Strategy (`detector.rs`)

Detection runs every 250ms, checking signals in priority order:

1. **Process exit** (most reliable): `child.try_wait()` returns exit status → Completed or Errored
2. **Screen content patterns**: Scan bottom 5 lines of `vt100::Screen` for:
   - Tool approval: `Allow <ToolName>` + `[Y/n]` → `WaitingForInput(ToolApproval)`
   - Question: lines ending with `?` after agent output → `WaitingForInput(Question)`
   - Input prompt: Claude Code's `>` prompt on empty line → `WaitingForInput(InputPrompt)`
   - Error: `Error:`, `API error`, `rate limit` → `Errored`
3. **Output timing**: No bytes for `idle_timeout_secs` (default 3s) → Idle
4. **Active output**: Bytes arriving recently → Running

Patterns are configurable in TOML so users can update them when Claude Code changes.

---

## Project Structure

```
maestro/
├── Cargo.toml
├── config/
│   └── default.toml              # Default configuration shipped with binary
├── docs/
│   └── PLAN.md                   # This file
├── src/
│   ├── main.rs                   # Entry point, CLI args (clap), bootstrap
│   ├── app.rs                    # Core App state, main event loop, update/render
│   ├── lib.rs                    # Library root, module re-exports
│   │
│   ├── agent/
│   │   ├── mod.rs                # Agent module root
│   │   ├── state.rs              # AgentState enum + PromptType + transitions
│   │   ├── manager.rs            # AgentManager: spawn/kill/restart/list agents
│   │   ├── handle.rs             # AgentHandle: bundles PTY + vt100 + state + metadata
│   │   └── detector.rs           # State detection heuristics from vt100::Screen
│   │
│   ├── pty/
│   │   ├── mod.rs                # PTY module root
│   │   ├── controller.rs         # Async read/write wrapper (spawn_blocking + mpsc)
│   │   └── spawner.rs            # Process spawning into PTY with env/cwd setup
│   │
│   ├── project/
│   │   ├── mod.rs                # Project module root
│   │   └── config.rs             # ProjectConfig struct, agent grouping
│   │
│   ├── ui/
│   │   ├── mod.rs                # Top-level render() function
│   │   ├── layout.rs             # Layout calculations (single, split-h, split-v, grid)
│   │   ├── sidebar.rs            # Sidebar widget: project tree with agent statuses
│   │   ├── terminal_pane.rs      # Terminal pane widget: wraps tui-term PseudoTerminal
│   │   ├── status_bar.rs         # Bottom status bar: counts + mode + keybinding hints
│   │   ├── command_palette.rs    # Fuzzy command overlay (v0.2)
│   │   └── theme.rs              # Color palette and style definitions
│   │
│   ├── input/
│   │   ├── mod.rs                # Input module root
│   │   ├── handler.rs            # Mode-aware input dispatcher (key → action)
│   │   ├── mode.rs               # InputMode enum: Normal, Insert, Command, Search
│   │   └── action.rs             # Action enum: all user-triggered actions
│   │
│   ├── event/
│   │   ├── mod.rs                # Event module root
│   │   ├── bus.rs                # Central event bus (tokio channels)
│   │   └── types.rs              # Event enum: Input, PtyOutput, StateChange, Tick
│   │
│   └── config/
│       ├── mod.rs                # Config module root
│       ├── settings.rs           # MaestroConfig struct (deserialized from TOML)
│       └── loader.rs             # Config file discovery, loading, validation
│
└── tests/
    └── integration/
        ├── agent_lifecycle.rs    # Spawn/kill/restart tests
        ├── pty_io.rs             # PTY read/write tests
        └── ui_render.rs          # Snapshot tests for UI rendering
```

---

## Dependencies

### Core

| Crate | Version | Purpose |
|-------|---------|---------|
| `ratatui` | 0.29 | TUI rendering framework (compatible with tui-term) |
| `crossterm` | 0.28 | Terminal backend: raw mode, alternate screen, event stream |
| `tokio` | 1 (full) | Async runtime for concurrent PTY I/O and event handling |
| `portable-pty` | 0.9 | Cross-platform PTY creation (used by WezTerm, battle-tested) |
| `vt100` | 0.15 | Virtual terminal state machine - parses ANSI from agent output |
| `tui-term` | 0.3 | Ratatui widget that renders vt100::Screen |

### Configuration & Serialization

| Crate | Version | Purpose |
|-------|---------|---------|
| `serde` | 1 (derive) | Struct serialization/deserialization |
| `toml` | 0.8 | TOML config file parsing |
| `dirs` | 5 | Platform config directory resolution (~/.config/) |

### CLI & Error Handling

| Crate | Version | Purpose |
|-------|---------|---------|
| `clap` | 4 (derive) | CLI argument parsing |
| `color-eyre` | 0.6 | Error reporting with context |
| `tracing` | 0.1 | Structured logging |
| `tracing-subscriber` | 0.3 | Log formatting and filtering |
| `tracing-appender` | 0.2 | File-based log output |

### Utility

| Crate | Version | Purpose |
|-------|---------|---------|
| `uuid` | 1 (v4) | Unique agent IDs |
| `chrono` | 0.4 | Timestamps for agent events |
| `fuzzy-matcher` | 0.3 | Command palette fuzzy search (v0.2) |
| `futures` | 0.3 | StreamExt for crossterm EventStream |

---

## Configuration Format

File: `~/.config/maestro/config.toml`

```toml
[global]
# Path to claude CLI (auto-detected if on PATH)
claude_binary = "claude"
# Default shell for PTY sessions
default_shell = "/bin/zsh"
# Maximum concurrent agents
max_agents = 15
# Log directory
log_dir = "~/.local/share/maestro/logs"
# State check interval (ms)
state_check_interval_ms = 250
# Seconds of no output before marking agent Idle
idle_timeout_secs = 3

[ui]
# Target frames per second
fps = 30
# Sidebar width in columns
sidebar_width = 28
# Default layout: "single", "split-h", "split-v", "grid"
default_layout = "single"
# Show agent uptime in sidebar
show_uptime = true

[ui.theme]
# Built-in: "default", "dark", "light", "gruvbox"
name = "default"

# ─── Projects ──────────────────────────────────────────────

[[project]]
name = "myapp"
path = "/Users/me/dev/myapp"

[[project.agent]]
name = "backend-refactor"
command = "claude"
args = ["--model", "opus"]
auto_start = true

[[project.agent]]
name = "test-runner"
command = "claude"
args = ["--append-system-prompt", "Focus on writing and running tests."]
auto_start = false

[[project]]
name = "webui"
path = "/Users/me/dev/webui"

[[project.agent]]
name = "frontend-fix"
command = "claude"
auto_start = true

# ─── Agent Templates (for command palette spawning) ────────

[[template]]
name = "reviewer"
command = "claude"
args = ["--append-system-prompt", "You are a code reviewer."]
description = "Code review specialist"

[[template]]
name = "tester"
command = "claude"
args = ["--append-system-prompt", "Focus on tests and coverage."]
description = "Test writing specialist"

# ─── State Detection Patterns (advanced) ───────────────────

[detection]
# Additional regex patterns (supplements built-in defaults)
# tool_approval_patterns = ["Allow .+ to .+\\?"]
# error_patterns = ["FATAL:", "Traceback"]
```

---

## Implementation Order (MVP - v0.1)

Build order follows dependency chain - each step builds on previous ones:

| Step | Module | Files | Description |
|------|--------|-------|-------------|
| 1 | Scaffold | `Cargo.toml`, `src/main.rs`, `src/lib.rs` | Project setup, dependencies, empty main |
| 2 | Config | `src/config/settings.rs`, `src/config/loader.rs` | Config structs, TOML loading, defaults |
| 3 | Types | `src/event/types.rs`, `src/input/action.rs`, `src/input/mode.rs` | Event/Action/Mode enums |
| 4 | Agent State | `src/agent/state.rs` | AgentState enum, PromptType, transitions |
| 5 | PTY | `src/pty/spawner.rs`, `src/pty/controller.rs` | PTY creation, async read/write |
| 6 | Agent Handle | `src/agent/handle.rs` | Bundle PTY + vt100 + state + metadata |
| 7 | Detector | `src/agent/detector.rs` | State detection heuristics |
| 8 | Manager | `src/agent/manager.rs` | Spawn/kill/restart/list agents |
| 9 | Theme | `src/ui/theme.rs` | Color palette definitions |
| 10 | Sidebar | `src/ui/sidebar.rs` | Project tree with status indicators |
| 11 | Terminal Pane | `src/ui/terminal_pane.rs` | Terminal rendering via tui-term |
| 12 | Status Bar | `src/ui/status_bar.rs` | Aggregate counts + mode indicator |
| 13 | Layout | `src/ui/layout.rs` | Layout calculations |
| 14 | Input | `src/input/handler.rs` | Mode-aware key dispatch |
| 15 | Event Bus + App | `src/event/bus.rs`, `src/app.rs` | Event wiring, main loop |
| 16 | Bootstrap | `src/main.rs`, `config/default.toml` | CLI args, graceful shutdown |

---

## Roadmap

### v0.1 - MVP (Core Dashboard)

**Goal**: Replace manual terminal tabs with a single dashboard. One visible agent at a time, basic navigation, full interactive PTY.

- Configuration loading from TOML
- Single-pane terminal view with sidebar
- Agent spawning, killing, restarting
- PTY-based interactive terminal (full Claude Code TUI rendering)
- State detection (Running, WaitingForInput, Idle, Completed, Errored)
- Vim-style sidebar navigation (j/k/Enter/Esc)
- Normal/Insert mode switching
- Status bar with aggregate counts
- Graceful shutdown (SIGTERM all agents, wait, SIGKILL after timeout)
- File-based logging

**Done when**: You can define 3 projects with 2-3 agents each in TOML, launch Maestro, navigate between agents, interact with Claude Code naturally, and see agent status at a glance.

### v0.2 - Enhanced UX

- **Split view**: Horizontal/vertical splits (2-4 visible panes)
- **Grid view**: 2x2 layout for monitoring 4 agents simultaneously
- **Command palette**: `:` to open, fuzzy-matched commands
- **Scrollback**: `Ctrl+u`/`Ctrl+d` for terminal history
- **Search**: `/` to search agent output, `n`/`N` for next/prev
- **Mouse support**: Click sidebar items, click panes to focus
- **PTY resize**: Correct dimensions when layout changes
- **Agent templates**: Spawn from predefined templates via command palette

### v0.3 - Advanced Features

- **Desktop notifications**: Notify when agent needs input, errors, or completes (useful when Maestro is in background)
- **Session persistence**: Save/restore agent sessions across Maestro restarts (scrollback to disk, re-attach on recovery)
- **Workspace profiles**: Named config sets ("dev", "review", "deploy")
- **Agent output export**: Export conversation to markdown
- **Stream-JSON mode**: Optional `claude -p --output-format stream-json` for background agents with precise state tracking
- **Auto-restart**: Configurable restart policy with backoff

### v1.0 - Production Release

- Error recovery and edge case handling
- Homebrew formula (`brew install maestro`)
- Shell completions (bash/zsh/fish via clap)
- Comprehensive documentation
- CI/CD for cross-platform builds (macOS, Linux)

---

## Key Technical Challenges

### 1. Terminal-in-Terminal

**Problem**: Rendering Claude Code's TUI (itself a complex terminal app) inside Ratatui.
**Solution**: `vt100` parses all ANSI output into a virtual screen. `tui-term` maps that screen to Ratatui cells. The PTY provides complete isolation.

### 2. Async PTY in Tokio

**Problem**: `portable-pty` is synchronous/blocking.
**Solution**: `tokio::task::spawn_blocking` for each agent's read loop, communicating via `mpsc` channels. Write side stays sync (fast enough).

### 3. State Detection Accuracy

**Problem**: Inferring agent state from terminal output is heuristic and fragile.
**Solution**: Layered approach - process exit (authoritative) > output patterns (configurable regex) > timing (fallback). Conservative defaults - better to show "Running" than false "WaitingForInput".

### 4. Input Mode Correctness

**Problem**: Must cleanly separate Maestro's UI navigation from PTY input forwarding.
**Solution**: Strict Normal/Insert mode split. `Esc` always returns to Normal. `Ctrl+\` as escape hatch. Mode clearly shown in status bar.

### 5. Performance at Scale

**Problem**: 15 agents each producing output, 30 FPS rendering.
**Solution**: Only render visible agents. Batch PTY output. State detection at 250ms (not every byte). Dirty flags to skip unchanged widgets.

---

## Verification Plan

1. **Build**: `cargo build` compiles without errors
2. **Smoke test**: 1 project, 1 agent:
   - Sidebar shows project + agent
   - Claude Code TUI renders correctly in main pane
   - Can enter Insert Mode and interact with Claude
   - Status updates (Running → WaitingForInput → Running)
   - Can kill and restart the agent
3. **Multi-agent**: 2 projects, 3 agents each:
   - Navigate between all agents
   - All render correctly when selected
   - Status indicators accurate
4. **Stress test**: 10+ agents - no performance degradation or crashes
5. **Graceful shutdown**: Quit → all agent processes terminated → terminal restored to normal
