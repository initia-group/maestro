# Architecture

Maestro is a Rust TUI application built with Ratatui, Tokio, and portable-pty. It manages multiple Claude Code agent processes through pseudo-terminals, rendering their full TUI output inside split panes.

## High-Level Overview

```
┌─────────────────────────────────────────────────┐
│                   main.rs                        │
│  CLI parsing, terminal setup, panic hook         │
│                     │                            │
│                     ▼                            │
│  ┌──────────────────────────────────────┐       │
│  │              App (app.rs)             │       │
│  │                                      │       │
│  │  ┌──────────┐  ┌──────────────────┐ │       │
│  │  │  Agent    │  │   InputHandler   │ │       │
│  │  │  Manager  │  │   (8 modes)     │ │       │
│  │  └──────────┘  └──────────────────┘ │       │
│  │  ┌──────────┐  ┌──────────────────┐ │       │
│  │  │  EventBus │  │   UI Renderer   │ │       │
│  │  │  (tokio)  │  │   (ratatui)     │ │       │
│  │  └──────────┘  └──────────────────┘ │       │
│  │  ┌──────────┐  ┌──────────────────┐ │       │
│  │  │  Session  │  │   Notification  │ │       │
│  │  │  Manager  │  │   Manager      │ │       │
│  │  └──────────┘  └──────────────────┘ │       │
│  └──────────────────────────────────────┘       │
└─────────────────────────────────────────────────┘
```

## Module Map

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `src/main.rs` | CLI parsing, terminal setup/teardown, panic hook, event bus wiring | `Cli` |
| `src/app.rs` | Central state machine, main event loop, action dispatch, rendering | `App` |
| `src/agent/manager.rs` | Owns all agent handles, spawn/kill/restart operations | `AgentManager` |
| `src/agent/handle.rs` | Individual agent state: PTY, vt100 parser, scrollback | `AgentHandle` |
| `src/agent/state.rs` | State enum and prompt type classification | `AgentState`, `PromptType` |
| `src/agent/detector.rs` | Regex-based state detection from screen content | `DetectionPatterns`, `DetectionDebounce` |
| `src/agent/restart.rs` | Auto-restart with exponential backoff | `RestartPolicy`, `RestartTracker` |
| `src/agent/scrollback.rs` | Scrollback buffer for terminal output history | `ScrollbackBuffer` |
| `src/agent/stream_json.rs` | Stream-JSON mode event parsing | — |
| `src/config/settings.rs` | Config struct hierarchy (serde deserialize targets) | `MaestroConfig`, `GlobalConfig`, `UiConfig` |
| `src/config/loader.rs` | TOML loading, validation, tilde expansion | `load_config()` |
| `src/config/profile.rs` | Workspace profile switching | `ProfileManager` |
| `src/event/bus.rs` | Tokio channel multiplexer for all event sources | `EventBus` |
| `src/event/types.rs` | Event and action enums | `AppEvent`, `InputEvent` |
| `src/input/handler.rs` | Mode-aware keyboard/mouse dispatch | `InputHandler` |
| `src/input/action.rs` | Semantic action enum (60+ variants) | `Action` |
| `src/input/mode.rs` | Input mode enum (8 modes) | `InputMode` |
| `src/ui/layout.rs` | Screen region calculation for all layout modes | `AppLayout`, `ActiveLayout` |
| `src/ui/pane_manager.rs` | Pane state, layout transitions, agent-to-pane assignments | `PaneManager` |
| `src/ui/sidebar.rs` | Project tree widget with agent status indicators | `SidebarItem`, `SidebarState` |
| `src/ui/terminal_pane.rs` | vt100-to-ratatui terminal rendering widget | `TerminalPane` |
| `src/ui/status_bar.rs` | Agent state counts, mode indicator, keybinding hints | `StatusBar` |
| `src/ui/command_palette.rs` | Fuzzy-search command overlay | `CommandPalette`, `PaletteCommand` |
| `src/ui/spawn_picker.rs` | Quick-select overlay for spawn variants | `SpawnPicker` |
| `src/ui/theme.rs` | Color scheme management (3 built-in themes, pulse animations) | `Theme` |
| `src/pty/spawner.rs` | PTY creation and process spawning | `spawn_in_pty()` |
| `src/pty/controller.rs` | Async I/O bridge between PTY and event bus | `PtyController` |
| `src/session/mod.rs` | Save/restore session snapshots to disk | `SessionManager` |
| `src/notification.rs` | OS desktop notifications via notify-rust | `NotificationManager` |
| `src/clipboard.rs` | System clipboard via arboard | `copy_to_clipboard()` |
| `src/export.rs` | Markdown export of agent terminal output | `OutputExporter` |

For detailed implementation specifications, see the [Feature Specs](features/).

## Event Flow

Maestro is event-driven. Three sources feed into a single `EventBus`:

```
┌──────────────────┐
│ Crossterm        │──→ InputEvent (key/mouse)
│ (event-stream)   │
└──────────────────┘
                        ┌──────────┐     ┌─────────────┐
┌──────────────────┐    │          │     │             │
│ PTY Controllers  │──→ │ EventBus │──→  │  App::run() │
│ (spawn_blocking) │    │ (mpsc)   │     │  (select!)  │
└──────────────────┘    │          │     │             │
                        └──────────┘     └─────────────┘
┌──────────────────┐
│ Timers           │──→ RenderRequest (30 FPS)
│ (tokio::interval)│──→ StateTick (250ms)
└──────────────────┘
```

The main loop in `App::run()` uses `tokio::select!` to receive events from the bus and dispatch them:

1. **InputEvent** → `InputHandler::handle_key()` → `Action` → `App::dispatch_action()`
2. **PtyOutput** → feed bytes to vt100 parser → update screen
3. **PtyEof** → process exit → update agent state
4. **StateTick** → run state detection on all agents
5. **RenderRequest** → redraw the UI

## Terminal-in-Terminal Rendering

The core technical challenge: rendering Claude Code's full TUI inside Maestro's panes.

```
Claude Code (full TUI app)
    │ (ANSI escape sequences)
    ▼
PTY Slave FD ←→ PTY Master FD
                    │ (raw bytes via spawn_blocking)
                    ▼
              vt100::Parser
                    │ (processes ANSI, maintains cell grid)
                    ▼
              vt100::Screen
                    │ (cell-by-cell rendering)
                    ▼
           tui_term::PseudoTerminal (Ratatui widget)
                    │
                    ▼
           Ratatui Frame Buffer
                    │
                    ▼
           Crossterm Backend → Real Terminal
```

Each agent has its own `vt100::Parser` that maintains a virtual terminal screen. The `TerminalPane` widget reads this screen cell-by-cell and maps the vt100 attributes (colors, bold, etc.) to Ratatui styles.

When a pane is resized (layout change), Maestro sends a `SIGWINCH` to the PTY so the child process can adapt.

## Agent State Machine

Agents cycle through 6 states, detected automatically:

```
  Spawning ──→ Running ←──→ WaitingForInput
                  │                │
                  ▼                ▼
                Idle  ←────────────┘
                  │
                  ▼
          Completed / Errored
```

| State | Symbol | Trigger |
|-------|--------|---------|
| Spawning | `○` | Agent just started, no output yet |
| Running | `●` | Agent is producing output |
| WaitingForInput | `?` | Screen matches a prompt pattern |
| Idle | `-` | No output for `idle_timeout_secs` |
| Completed | `✓` | Process exited with code 0 |
| Errored | `!` | Process exited with non-zero code or signal |

**Detection priority** (highest to lowest):
1. **Process exit** → Completed or Errored (authoritative)
2. **Screen patterns** → WaitingForInput (regex heuristic)
3. **Output timing** → Idle or Running (fallback)

**WaitingForInput subtypes** (`PromptType`):
- `ToolApproval` — "Allow Edit to ...? [Y/n]"
- `AskUserQuestion` — numbered option prompts
- `Question` — line ending with `?`
- `InputPrompt` — bare `>` or `$` prompt

**Anti-flapping:** A 2-tick debounce prevents rapid state toggling. Terminal states (Completed/Errored) bypass debounce.

See [Feature 05](features/05-agent-state-machine.md) for the full spec.

## Input Mode System

8 modes, each with its own key dispatch:

| Mode | Purpose |
|------|---------|
| Normal | Navigation and commands |
| Insert | Forward keystrokes to agent PTY |
| Command | Command palette overlay |
| Search | Search agent terminal output |
| SpawnPicker | Select agent spawn variant |
| Rename | Rename an agent |
| RenameProject | Rename a project |
| NewProject | Create a new project (two-step) |

`InputHandler` owns the current mode and translates each `KeyEvent` into a semantic `Action`. Mode transitions happen internally within the handler.

**Insert mode uses `Ctrl+G` to exit** (not `Esc`). This is because `Esc` must be forwarded to the PTY — Claude Code uses `Esc` for its own UI.

See [Feature 07](features/07-input-handling.md) for details.

## Configuration Pipeline

```
CLI path → MAESTRO_CONFIG env → ~/.config/maestro/config.toml → built-in defaults
                                        │
                                        ▼
                                  TOML parsing (serde)
                                        │
                                        ▼
                                  MaestroConfig struct
                                        │
                                        ▼
                                  Validation (uniqueness, regex compilation)
                                        │
                                        ▼
                                  Tilde expansion on all path fields
```

Hot reload via `:config reload` re-reads the file and applies changes without restarting.

See [Feature 02](features/02-configuration-system.md) for details.

## Session Persistence

- **Autosave**: every N seconds (configurable, default 60s)
- **What is saved**: agent configs, layout mode, scrollback buffers (raw PTY bytes)
- **What is NOT saved**: running processes — they are re-spawned on restore
- **Storage**: JSON files in `~/.local/share/maestro/sessions/`
- **Session resume**: passes `--resume` to Claude CLI for conversation continuity

See [Feature 18](features/18-session-persistence.md) for details.

## Key Design Decisions

1. **portable-pty + spawn_blocking** — PTY reads are blocking I/O. Wrapping them in `tokio::task::spawn_blocking` bridges sync I/O into the async runtime without blocking the event loop.

2. **vt100 + tui-term** — Instead of passing raw ANSI to the terminal, each agent's output is parsed into a virtual screen. This enables scrollback, search highlighting, text selection, and multi-pane rendering.

3. **Vim-style modal input** — Modal input avoids modifier key conflicts with terminal applications. Claude Code uses many key combinations internally, so a clear modal separation prevents conflicts.

4. **tokio::select! event loop** — A single multiplexed loop handles all event sources. This is simpler and more efficient than actor-based message passing for this use case.

5. **TOML configuration** — TOML's table/array syntax maps naturally to the project/agent hierarchy. `deny_unknown_fields` catches typos early.
