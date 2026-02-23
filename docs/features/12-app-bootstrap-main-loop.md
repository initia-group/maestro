# Feature 12: Application Bootstrap & Main Event Loop

## Overview

Implement the central `App` struct that ties everything together: the main event loop, the render cycle, action dispatch, and graceful startup/shutdown. This is the orchestrator — it owns the `AgentManager`, `InputHandler`, `EventBus`, `SidebarState`, and all UI state. It's the final feature needed for a working v0.1 MVP.

## Dependencies

- **All previous features (01-11)** — this feature integrates them all.

## Technical Specification

### App Struct (`src/app.rs`)

```rust
use crate::agent::manager::{AgentManager, StateCounts};
use crate::agent::AgentId;
use crate::config::settings::MaestroConfig;
use crate::event::bus::EventBus;
use crate::event::types::{AppEvent, InputEvent};
use crate::input::action::Action;
use crate::input::handler::InputHandler;
use crate::input::mode::InputMode;
use crate::ui::layout::{
    ActiveLayout, AppLayout, calculate_layout, is_terminal_large_enough, pane_to_pty_size,
};
use crate::ui::sidebar::{Sidebar, SidebarState};
use crate::ui::status_bar::StatusBar;
use crate::ui::terminal_pane::{TerminalPane, EmptyPane};
use crate::ui::theme::Theme;
use color_eyre::eyre::Result;
use ratatui::prelude::*;
use ratatui::Terminal;
use std::time::Duration;
use tracing::{info, warn, error, debug};

/// Application state and main event loop.
pub struct App {
    /// Configuration.
    config: MaestroConfig,

    /// Agent manager — owns all agent handles.
    agent_manager: AgentManager,

    /// Input handler — mode-aware key dispatch.
    input_handler: InputHandler,

    /// Sidebar UI state (selection, collapse).
    sidebar_state: SidebarState,

    /// Current layout mode.
    layout: ActiveLayout,

    /// Active theme.
    theme: Theme,

    /// Whether any component has changed since last render.
    dirty: bool,

    /// Whether the app is running (false = exit main loop).
    running: bool,

    /// Whether a quit confirmation is pending.
    quit_pending: bool,

    /// Whether the help overlay is visible.
    show_help: bool,

    /// Pane focus index (for split/grid views, which pane is active).
    focused_pane: usize,

    /// The last known terminal area (for resize detection).
    last_area: Rect,
}

impl App {
    /// Create a new App from configuration.
    pub fn new(config: MaestroConfig, event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>) -> Self {
        let theme = Theme::from_name(&config.ui.theme.name);
        let layout = match config.ui.default_layout {
            crate::config::settings::LayoutMode::Single => ActiveLayout::Single,
            crate::config::settings::LayoutMode::SplitH => ActiveLayout::SplitHorizontal,
            crate::config::settings::LayoutMode::SplitV => ActiveLayout::SplitVertical,
            crate::config::settings::LayoutMode::Grid => ActiveLayout::Grid,
        };

        Self {
            agent_manager: AgentManager::new(&config, event_tx),
            input_handler: InputHandler::new(),
            sidebar_state: SidebarState::new(),
            layout,
            theme,
            dirty: true,
            running: true,
            quit_pending: false,
            show_help: false,
            focused_pane: 0,
            last_area: Rect::default(),
            config,
        }
    }

    // ─── Main Loop ─────────────────────────────────────

    /// Run the main event loop.
    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
        event_bus: &mut EventBus,
    ) -> Result<()> {
        // Spawn auto-start agents
        let initial_area = terminal.size()?;
        self.last_area = initial_area;
        let pty_size = self.calculate_default_pty_size(initial_area);
        let errors = self.agent_manager.spawn_auto_start_agents(pty_size);
        for (name, err) in &errors {
            error!("Failed to start agent {}: {}", name, err);
        }

        // Build initial sidebar
        self.rebuild_sidebar();

        // Initial render
        terminal.draw(|frame| self.render(frame))?;

        // Main event loop
        while self.running {
            match event_bus.next().await {
                Some(event) => self.handle_event(event, terminal)?,
                None => {
                    warn!("Event bus closed unexpectedly");
                    break;
                }
            }
        }

        // Graceful shutdown
        info!("Shutting down...");
        self.agent_manager.shutdown_all(Duration::from_secs(5)).await;
        info!("All agents terminated");

        Ok(())
    }

    // ─── Event Handling ────────────────────────────────

    fn handle_event(
        &mut self,
        event: AppEvent,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        match event {
            AppEvent::Input(InputEvent::Key(key)) => {
                let action = self.input_handler.handle_key(key);
                self.dispatch_action(action)?;
            }

            AppEvent::PtyOutput { agent_id, data } => {
                if let Some(handle) = self.agent_manager.get_mut(agent_id) {
                    handle.process_output(&data);
                    self.dirty = true;
                }
            }

            AppEvent::PtyEof { agent_id } => {
                debug!("PTY EOF for agent {}", agent_id);
                // State detection will pick up the exit on next StateTick
                self.dirty = true;
            }

            AppEvent::AgentStateChanged { agent_id, old_state, new_state } => {
                info!("Agent {} state: {:?} → {:?}", agent_id, old_state, new_state);
                self.rebuild_sidebar();
                self.dirty = true;
            }

            AppEvent::StateTick => {
                let changes = self.agent_manager.detect_all_states();
                if !changes.is_empty() {
                    self.rebuild_sidebar();
                    self.dirty = true;
                }
            }

            AppEvent::RenderRequest => {
                if self.dirty {
                    terminal.draw(|frame| self.render(frame))?;
                    self.dirty = false;

                    // Clear dirty flags on visible agents
                    if let Some(id) = self.sidebar_state.selected_agent_id() {
                        if let Some(handle) = self.agent_manager.get_mut(id) {
                            handle.mark_clean();
                        }
                    }
                }
            }

            AppEvent::Resize { cols, rows } => {
                let new_area = Rect::new(0, 0, cols, rows);
                self.last_area = new_area;
                self.resize_visible_agents(new_area);
                self.dirty = true;
            }

            AppEvent::QuitRequested => {
                self.running = false;
            }
        }

        Ok(())
    }

    // ─── Action Dispatch ───────────────────────────────

    fn dispatch_action(&mut self, action: Action) -> Result<()> {
        match action {
            // Navigation
            Action::SelectNext => {
                self.sidebar_state.select_next();
                self.dirty = true;
            }
            Action::SelectPrev => {
                self.sidebar_state.select_prev();
                self.dirty = true;
            }
            Action::NextProject => {
                self.sidebar_state.next_project();
                self.dirty = true;
            }
            Action::PrevProject => {
                self.sidebar_state.prev_project();
                self.dirty = true;
            }
            Action::JumpToAgent(n) => {
                self.sidebar_state.jump_to_agent(n);
                self.dirty = true;
            }
            Action::FocusAgent(id) => {
                self.sidebar_state.select_agent(id);
                self.dirty = true;
            }

            // Mode switching
            Action::EnterInsertMode => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    if let Some(handle) = self.agent_manager.get(id) {
                        let agent_name = handle.name().to_string();
                        self.input_handler.set_mode(InputMode::Insert { agent_name });

                        // Resize agent to current pane if not already matching
                        let layout = calculate_layout(
                            self.last_area,
                            self.config.ui.sidebar_width,
                            &self.layout,
                        );
                        if let Some(pane) = layout.panes.first() {
                            let size = pane_to_pty_size(&pane.inner);
                            if let Some(handle) = self.agent_manager.get_mut(id) {
                                handle.resize(size);
                            }
                        }

                        self.dirty = true;
                    }
                }
            }
            Action::ExitInsertMode => {
                // Mode already changed by InputHandler
                self.dirty = true;
            }
            Action::OpenCommandPalette => {
                self.input_handler.set_mode(InputMode::Command {
                    input: String::new(),
                    selected: 0,
                });
                self.dirty = true;
            }
            Action::CloseCommandPalette => {
                // Mode already changed by InputHandler
                self.dirty = true;
            }

            // Agent lifecycle
            Action::SpawnAgent => {
                // For v0.1, spawn a basic claude agent in the first project
                // In v0.2, this opens the command palette with "spawn" pre-filled
                // TODO: implement spawn dialog
                self.dirty = true;
            }
            Action::KillAgent => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    if self.quit_pending {
                        // Double-press confirms kill
                        if let Err(e) = self.agent_manager.kill(id) {
                            warn!("Kill failed: {}", e);
                        }
                        self.rebuild_sidebar();
                        self.quit_pending = false;
                    } else {
                        // First press — set pending confirmation
                        self.quit_pending = true;
                        // TODO: show confirmation in status bar
                    }
                    self.dirty = true;
                }
            }
            Action::RestartAgent => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    let pty_size = self.calculate_default_pty_size(self.last_area);
                    match self.agent_manager.restart(id, pty_size) {
                        Ok(new_id) => {
                            self.sidebar_state.select_agent(new_id);
                            self.rebuild_sidebar();
                        }
                        Err(e) => warn!("Restart failed: {}", e),
                    }
                    self.dirty = true;
                }
            }

            // PTY interaction
            Action::SendToPty(bytes) => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    if let Some(handle) = self.agent_manager.get(id) {
                        if let Err(e) = handle.write_input(&bytes) {
                            warn!("PTY write failed: {}", e);
                        }
                    }
                }
            }

            // Application
            Action::ToggleHelp => {
                self.show_help = !self.show_help;
                self.dirty = true;
            }
            Action::Quit => {
                let alive = self.agent_manager.state_counts();
                let total_alive = alive.running + alive.waiting + alive.idle + alive.spawning;

                if total_alive == 0 || self.quit_pending {
                    self.running = false;
                } else {
                    // Show confirmation — press q again to quit
                    self.quit_pending = true;
                    self.dirty = true;
                    // TODO: show "Press q again to quit (N agents running)" in status bar
                }
            }

            // v0.2 actions — no-op for now
            Action::SplitHorizontal | Action::SplitVertical | Action::CloseSplit |
            Action::CyclePaneFocus | Action::EnterSearchMode | Action::SearchNext |
            Action::SearchPrev | Action::ScrollUp | Action::ScrollDown |
            Action::SpawnFromTemplate { .. } | Action::ReloadConfig => {
                // Not implemented in v0.1
            }

            Action::None => {}
        }

        // Reset quit_pending if user did something other than quit/kill
        match action {
            Action::Quit | Action::KillAgent => {}
            _ => { self.quit_pending = false; }
        }

        Ok(())
    }

    // ─── Rendering ─────────────────────────────────────

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Check minimum size
        if !is_terminal_large_enough(area) {
            let msg = format!(
                "Terminal too small. Need at least {}x{}, have {}x{}.",
                crate::ui::layout::MIN_COLS,
                crate::ui::layout::MIN_ROWS,
                area.width,
                area.height,
            );
            let paragraph = ratatui::widgets::Paragraph::new(msg)
                .alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
            return;
        }

        // Calculate layout
        let layout = calculate_layout(area, self.config.ui.sidebar_width, &self.layout);

        // Render sidebar
        let sidebar = Sidebar::new(&self.theme, self.config.ui.show_uptime);
        frame.render_stateful_widget(sidebar, layout.sidebar, &mut self.sidebar_state);

        // Render terminal pane(s)
        for (i, pane) in layout.panes.iter().enumerate() {
            let is_focused = i == self.focused_pane;
            let agent_id = self.get_pane_agent_id(i);

            match agent_id.and_then(|id| self.agent_manager.get(id)) {
                Some(handle) => {
                    let terminal_pane = TerminalPane::new(
                        handle.screen(),
                        handle.name(),
                        handle.project_name(),
                        handle.state(),
                        is_focused,
                        &self.theme,
                    );
                    frame.render_widget(terminal_pane, pane.area);

                    // Set cursor position in Insert Mode
                    if matches!(self.input_handler.mode(), InputMode::Insert { .. }) && is_focused {
                        if let Some((cx, cy)) = crate::ui::terminal_pane::cursor_position(
                            handle.screen(), &pane.inner,
                        ) {
                            frame.set_cursor_position((cx, cy));
                        }
                    }
                }
                None => {
                    let empty = EmptyPane::new(
                        &self.theme,
                        if self.agent_manager.agent_count() == 0 {
                            "No agents. Press 'n' to spawn one."
                        } else {
                            "Select an agent from the sidebar."
                        },
                    );
                    frame.render_widget(empty, pane.area);
                }
            }
        }

        // Render status bar
        let state_counts = self.agent_manager.state_counts();
        let status_bar = StatusBar::new(
            &state_counts,
            self.input_handler.mode(),
            &self.theme,
        );
        frame.render_widget(status_bar, layout.status_bar);

        // Render help overlay if active
        if self.show_help {
            self.render_help_overlay(frame, area);
        }

        // Render quit confirmation if pending
        if self.quit_pending {
            // TODO: render a small confirmation banner
        }
    }

    fn render_help_overlay(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = crate::ui::layout::help_overlay_area(area);

        // Clear the overlay area
        let clear = ratatui::widgets::Clear;
        frame.render_widget(clear, overlay_area);

        let help_text = vec![
            ("j/k", "Navigate agents"),
            ("J/K", "Navigate projects"),
            ("1-9", "Jump to agent"),
            ("Enter/i", "Enter Insert Mode"),
            ("Esc", "Return to Normal Mode"),
            ("n", "Spawn new agent"),
            ("d", "Kill agent (confirm)"),
            ("r", "Restart agent"),
            (":", "Command palette"),
            ("?", "Toggle this help"),
            ("q", "Quit (confirm if running)"),
        ];

        let lines: Vec<Line> = help_text
            .iter()
            .map(|(key, desc)| {
                Line::from(vec![
                    Span::styled(format!("{:>12}  ", key), self.theme.help_key),
                    Span::styled(*desc, self.theme.help_description),
                ])
            })
            .collect();

        let block = ratatui::widgets::Block::default()
            .title(" Help (? to close) ")
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(self.theme.help_key)
            .style(ratatui::style::Style::default().bg(self.theme.help_overlay_bg));

        let paragraph = ratatui::widgets::Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, overlay_area);
    }

    // ─── Helpers ───────────────────────────────────────

    /// Get the agent ID for a given pane index.
    /// For Single layout, pane 0 = selected agent.
    /// For split/grid, panes map to ordered agents starting from the selected one.
    fn get_pane_agent_id(&self, pane_index: usize) -> Option<AgentId> {
        match self.layout {
            ActiveLayout::Single => {
                self.sidebar_state.selected_agent_id()
            }
            _ => {
                // In multi-pane layouts, show agents starting from the selected one
                let all_ids = self.agent_manager.all_agent_ids_ordered();
                let selected_idx = self.sidebar_state.selected_index();

                // Find the flat index of the selected agent
                let mut agent_flat_idx = 0;
                let mut found = false;
                for (i, item) in self.sidebar_state.items().iter().enumerate() {
                    if matches!(item, crate::ui::sidebar::SidebarItem::Agent { .. }) {
                        if i == selected_idx {
                            found = true;
                            break;
                        }
                        agent_flat_idx += 1;
                    }
                }

                if found {
                    all_ids.get(agent_flat_idx + pane_index).copied()
                } else {
                    all_ids.get(pane_index).copied()
                }
            }
        }
    }

    fn rebuild_sidebar(&mut self) {
        self.sidebar_state.rebuild(
            self.agent_manager.agents_by_project(),
            &self.agent_manager,
        );
    }

    fn calculate_default_pty_size(&self, area: Rect) -> portable_pty::PtySize {
        let layout = calculate_layout(area, self.config.ui.sidebar_width, &self.layout);
        layout.panes.first()
            .map(|p| pane_to_pty_size(&p.inner))
            .unwrap_or(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
    }

    fn resize_visible_agents(&mut self, area: Rect) {
        let layout = calculate_layout(area, self.config.ui.sidebar_width, &self.layout);
        for (i, pane) in layout.panes.iter().enumerate() {
            if let Some(id) = self.get_pane_agent_id(i) {
                let size = pane_to_pty_size(&pane.inner);
                if let Some(handle) = self.agent_manager.get_mut(id) {
                    handle.resize(size);
                }
            }
        }
    }
}
```

### Bootstrap Flow (`src/main.rs`)

```rust
use clap::Parser;
use color_eyre::eyre::Result;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use maestro::app::App;
use maestro::config::loader::load_config;
use maestro::event::bus::EventBus;
use ratatui::prelude::*;
use std::io::stdout;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(name = "maestro", version, about = "TUI agent dashboard for Claude Code")]
struct Cli {
    /// Path to config file
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    // Load configuration
    let config = load_config(cli.config.as_deref())?;

    // Setup logging to file
    let log_dir = crate::config::loader::expand_tilde(&config.global.log_dir);
    std::fs::create_dir_all(&log_dir)?;
    let file_appender = tracing_appender::rolling::daily(&log_dir, "maestro.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&cli.log_level)))
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    info!("Maestro starting");
    info!("Config: {} projects, {} templates",
        config.project.len(),
        config.template.len(),
    );

    // Setup panic hook to restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        original_hook(info);
    }));

    // Terminal setup
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    // Create event bus and start background tasks
    let mut event_bus = EventBus::new();
    event_bus.start(config.ui.fps, config.global.state_check_interval_ms);

    // Create and run the app
    let event_tx = event_bus.sender();
    let mut app = App::new(config, event_tx);
    let result = app.run(&mut terminal, &mut event_bus).await;

    // Terminal teardown (always runs, even on error)
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    info!("Maestro exited");
    result
}
```

### Graceful Shutdown Sequence

1. User presses `q` (first press: confirmation if agents running, second press: confirm).
2. `App::running` set to `false`.
3. Main loop exits.
4. `shutdown_all(5s)` sends SIGTERM to all agents, waits up to 5 seconds.
5. After timeout, agents are force-killed.
6. Terminal is restored to normal state.

### Error Recovery

If the main loop encounters an error:
1. The error is logged.
2. Terminal is restored (in the `main.rs` cleanup).
3. The error is reported to the user via `color-eyre`.

The panic hook ensures terminal restoration even on unexpected panics.

## Implementation Steps

1. **Implement `src/app.rs`**
   - `App` struct with all state fields.
   - `new()` constructor from config.
   - `run()` main event loop.
   - `handle_event()` event dispatcher.
   - `dispatch_action()` action processor.
   - `render()` composing all widgets.
   - Helper methods: `rebuild_sidebar()`, `resize_visible_agents()`, etc.

2. **Update `src/main.rs`**
   - Full bootstrap: CLI args, config loading, logging setup, panic hook.
   - Terminal setup/teardown.
   - Event bus creation and start.
   - App creation and run.
   - Cleanup on exit.

3. **Wire everything together**
   - Ensure all modules are properly imported and connected.
   - Fix any compilation errors from integration.

4. **Manual end-to-end testing**
   - See verification plan below.

## Error Handling

| Scenario | Handling |
|---|---|
| Config file error | Print error to stderr and exit before entering TUI mode. |
| Log directory creation fails | Print warning to stderr, continue without file logging. |
| Terminal setup fails | Return error, no cleanup needed (wasn't set up yet). |
| Event bus closed | Main loop exits gracefully. |
| Render error | Log and attempt to continue. If persistent, exit. |
| Agent spawn error during auto-start | Log error, continue starting remaining agents. Show error in status bar. |
| Panic anywhere | Panic hook restores terminal. Error is printed to stderr. |
| SIGINT (Ctrl+C) | Caught by crossterm. Triggers quit sequence. |
| SIGTERM | Process exits. Cleanup may not run (OS behavior). |

## Testing Strategy

### Unit Tests

Limited for the App struct itself (it depends on terminal I/O). Focus on:

```rust
#[test]
fn test_dispatch_select_next() {
    // Would need to create App with mock terminal
    // For v0.1, this is tested via integration tests
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_app_starts_and_exits() {
    // Create a config with no auto-start agents
    // Create a mock terminal backend (ratatui::backend::TestBackend)
    // Create event bus, inject a Quit event
    // Run app — should exit cleanly
}
```

### End-to-End Verification Plan

This is the critical verification for v0.1 completion:

**1. Basic startup**
- [ ] `cargo run` — Maestro starts without errors.
- [ ] Sidebar shows configured projects and agents.
- [ ] Auto-start agents spawn and begin running.
- [ ] Status bar shows correct counts.

**2. Navigation**
- [ ] `j`/`k` moves selection in sidebar.
- [ ] Selected agent's terminal output appears in main pane.
- [ ] Switching agents updates the main pane.

**3. Insert Mode**
- [ ] `Enter` or `i` enters Insert Mode. Status bar shows "INSERT (name)".
- [ ] Keystrokes are forwarded to the agent's PTY.
- [ ] Can type messages to Claude Code.
- [ ] Can approve/deny tool approvals (`y`/`n`).
- [ ] `Esc` returns to Normal Mode.
- [ ] `Ctrl+C` is forwarded (not captured by Maestro).

**4. State detection**
- [ ] Running agents show green `●`.
- [ ] Agents waiting for input show yellow `?`.
- [ ] Idle agents show gray `-`.
- [ ] Completed agents show green `✓`.
- [ ] Errored agents show red `!`.

**5. Agent lifecycle**
- [ ] `d` (twice) kills the selected agent.
- [ ] `r` restarts the selected agent.
- [ ] Killed/completed agents stay in sidebar with appropriate icon.

**6. Resize**
- [ ] Resize terminal window → UI reflows correctly.
- [ ] Agent output re-renders at new dimensions.

**7. Quit**
- [ ] `q` with running agents → shows confirmation.
- [ ] `q` twice → exits, kills all agents, restores terminal.
- [ ] `q` with no running agents → exits immediately.

**8. Error handling**
- [ ] Missing config file → starts with defaults.
- [ ] Invalid config → clear error message before TUI.
- [ ] Agent spawn failure → error logged, other agents unaffected.

**9. Help**
- [ ] `?` toggles help overlay.
- [ ] Help shows all keybindings.

**10. Stress test**
- [ ] Start 10+ agents. No performance degradation. UI remains responsive.
- [ ] Navigate rapidly between agents. No crashes or glitches.

## Acceptance Criteria

- [ ] `cargo run` starts Maestro with configured agents.
- [ ] Main event loop handles all event types without errors.
- [ ] Action dispatch handles all v0.1 actions correctly.
- [ ] Rendering composes sidebar + terminal pane + status bar correctly.
- [ ] Insert Mode forwards keystrokes and shows cursor.
- [ ] Normal Mode navigation works (j/k/J/K/1-9/Enter/Esc).
- [ ] State detection runs every 250ms and updates sidebar.
- [ ] Kill and restart work correctly.
- [ ] Terminal is always restored on exit (normal, error, or panic).
- [ ] Logging writes to file (not stdout — that's the TUI).
- [ ] Help overlay toggles with `?`.
- [ ] Quit with confirmation when agents are running.
- [ ] Terminal resize updates PTY dimensions and re-renders.
- [ ] All end-to-end verification tests pass.
