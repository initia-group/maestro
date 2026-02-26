# Maestro Documentation

Maestro is a terminal UI dashboard for managing multiple [Claude Code](https://docs.anthropic.com/en/docs/claude-code) agent processes. It renders each agent's full TUI output inside split panes, with vim-style modal input, automatic state detection, and session persistence.

## Quick Links

| Document | Description |
|----------|-------------|
| [Getting Started](getting-started.md) | Installation, first run, essential workflow |
| [Configuration](configuration.md) | Complete TOML config reference |
| [Keybindings](keybindings.md) | All keyboard shortcuts across 8 input modes |
| [Commands](commands.md) | Command palette reference |
| [Theming](theming.md) | Built-in themes and visual customization |
| [Architecture](architecture.md) | System design, event flow, module map |
| [Deployment](deployment.md) | CI/CD, release process, Homebrew distribution |
| [Contributing](contributing.md) | Build, test, lint, PR workflow |

---

## Feature Specifications

Internal design documents covering every subsystem in detail.

| # | Feature | Document |
|---|---------|----------|
| 00 | Dependency Compatibility Matrix | [00-dependency-compatibility.md](features/00-dependency-compatibility.md) |
| 01 | Project Scaffold & Build System | [01-project-scaffold.md](features/01-project-scaffold.md) |
| 02 | Configuration System | [02-configuration-system.md](features/02-configuration-system.md) |
| 03 | Core Types & Event System | [03-core-types-event-system.md](features/03-core-types-event-system.md) |
| 04 | PTY Management | [04-pty-management.md](features/04-pty-management.md) |
| 05 | Agent State Machine & Detection | [05-agent-state-machine.md](features/05-agent-state-machine.md) |
| 06 | Agent Lifecycle Management | [06-agent-lifecycle-management.md](features/06-agent-lifecycle-management.md) |
| 07 | Input Handling & Modal System | [07-input-handling.md](features/07-input-handling.md) |
| 08 | UI Theme & Layout System | [08-theme-layout-system.md](features/08-theme-layout-system.md) |
| 09 | UI Sidebar Widget | [09-sidebar-widget.md](features/09-sidebar-widget.md) |
| 10 | UI Terminal Pane | [10-terminal-pane.md](features/10-terminal-pane.md) |
| 11 | UI Status Bar | [11-status-bar.md](features/11-status-bar.md) |
| 12 | Application Bootstrap & Main Event Loop | [12-app-bootstrap-main-loop.md](features/12-app-bootstrap-main-loop.md) |
| 13 | Split & Grid Views | [13-split-grid-views.md](features/13-split-grid-views.md) |
| 14 | Command Palette | [14-command-palette.md](features/14-command-palette.md) |
| 15 | Scrollback & Search | [15-scrollback-search.md](features/15-scrollback-search.md) |
| 16 | Mouse Support | [16-mouse-support.md](features/16-mouse-support.md) |
| 17 | Desktop Notifications | [17-desktop-notifications.md](features/17-desktop-notifications.md) |
| 18 | Session Persistence | [18-session-persistence.md](features/18-session-persistence.md) |
| 19 | Workspace Profiles & Agent Templates | [19-workspace-profiles-templates.md](features/19-workspace-profiles-templates.md) |
| 20 | Agent Output Export & Stream-JSON Mode | [20-output-export-stream-json.md](features/20-output-export-stream-json.md) |

---

## Source Map

Quick reference mapping each source module to its purpose and related documentation.

| Module | Purpose | Docs |
|--------|---------|------|
| `src/main.rs` | CLI entry point, terminal setup, panic hook | [Getting Started](getting-started.md), [Feature 12](features/12-app-bootstrap-main-loop.md) |
| `src/app.rs` | App state machine, main event loop, action dispatch | [Architecture](architecture.md), [Feature 12](features/12-app-bootstrap-main-loop.md) |
| `src/lib.rs` | Library re-exports | — |
| `src/agent/manager.rs` | AgentManager: spawn, kill, restart agents | [Feature 06](features/06-agent-lifecycle-management.md) |
| `src/agent/handle.rs` | AgentHandle: individual agent state and PTY | [Feature 06](features/06-agent-lifecycle-management.md) |
| `src/agent/state.rs` | AgentState enum (6 states), PromptType | [Architecture](architecture.md), [Feature 05](features/05-agent-state-machine.md) |
| `src/agent/detector.rs` | Regex-based state detection from screen content | [Configuration](configuration.md), [Feature 05](features/05-agent-state-machine.md) |
| `src/agent/restart.rs` | Auto-restart with exponential backoff | [Configuration](configuration.md), [Feature 06](features/06-agent-lifecycle-management.md) |
| `src/agent/scrollback.rs` | Scrollback buffer for terminal output history | [Feature 15](features/15-scrollback-search.md) |
| `src/agent/stream_json.rs` | Stream-JSON mode event parsing | [Feature 20](features/20-output-export-stream-json.md) |
| `src/config/settings.rs` | Config struct hierarchy (serde targets) | [Configuration](configuration.md), [Feature 02](features/02-configuration-system.md) |
| `src/config/loader.rs` | TOML loading, validation, tilde expansion | [Configuration](configuration.md), [Feature 02](features/02-configuration-system.md) |
| `src/config/profile.rs` | Workspace profile switching | [Feature 19](features/19-workspace-profiles-templates.md) |
| `src/event/bus.rs` | EventBus: tokio channel multiplexer | [Architecture](architecture.md), [Feature 03](features/03-core-types-event-system.md) |
| `src/event/types.rs` | AppEvent, InputEvent enums | [Architecture](architecture.md), [Feature 03](features/03-core-types-event-system.md) |
| `src/input/handler.rs` | Mode-aware key/mouse dispatch | [Keybindings](keybindings.md), [Feature 07](features/07-input-handling.md) |
| `src/input/action.rs` | Action enum (60+ variants) | [Feature 07](features/07-input-handling.md) |
| `src/input/mode.rs` | InputMode enum (8 modes) | [Keybindings](keybindings.md), [Feature 07](features/07-input-handling.md) |
| `src/ui/layout.rs` | Layout calculation engine | [Feature 08](features/08-theme-layout-system.md), [Feature 13](features/13-split-grid-views.md) |
| `src/ui/pane_manager.rs` | Pane state and layout transitions | [Feature 13](features/13-split-grid-views.md) |
| `src/ui/sidebar.rs` | Project tree widget with status indicators | [Feature 09](features/09-sidebar-widget.md) |
| `src/ui/terminal_pane.rs` | vt100-to-ratatui terminal renderer | [Feature 10](features/10-terminal-pane.md) |
| `src/ui/status_bar.rs` | Status bar widget | [Feature 11](features/11-status-bar.md) |
| `src/ui/command_palette.rs` | Fuzzy-search command overlay | [Commands](commands.md), [Feature 14](features/14-command-palette.md) |
| `src/ui/spawn_picker.rs` | Agent type selector overlay | [Feature 06](features/06-agent-lifecycle-management.md) |
| `src/ui/theme.rs` | Theme definitions (3 built-in) | [Theming](theming.md), [Feature 08](features/08-theme-layout-system.md) |
| `src/pty/spawner.rs` | PTY creation and process spawn | [Feature 04](features/04-pty-management.md) |
| `src/pty/controller.rs` | Async I/O bridge (PTY ↔ event bus) | [Feature 04](features/04-pty-management.md) |
| `src/session/mod.rs` | Save/restore session snapshots | [Feature 18](features/18-session-persistence.md) |
| `src/clipboard.rs` | System clipboard (arboard) | [Feature 07](features/07-input-handling.md) |
| `src/notification.rs` | Desktop notifications (notify-rust) | [Feature 17](features/17-desktop-notifications.md) |
| `src/export.rs` | Markdown output export | [Feature 20](features/20-output-export-stream-json.md) |
