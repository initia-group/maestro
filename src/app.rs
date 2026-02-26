//! Application state and main orchestrator.
//!
//! The `App` struct owns the agent manager, input handler, sidebar state,
//! and coordinates the main event loop with rendering and action dispatch.

use crate::agent::manager::AgentManager;
use crate::agent::state::AgentState;
use crate::agent::AgentId;
use crate::config::loader::expand_tilde;
use crate::config::profile::ProfileManager;
use crate::config::settings::MaestroConfig;
use crate::event::bus::EventBus;
use crate::event::types::{AppEvent, InputEvent};
use crate::input::action::{Action, SpawnKind};
use crate::input::handler::InputHandler;
use crate::input::mode::{InputMode, NewProjectStep};
use crate::session::{SavedAgent, SessionManager, SessionSnapshot};
use crate::ui::command_palette::{
    build_command_registry, parse_command, CommandPalette, PaletteCommand, PaletteMatcher,
};
use crate::ui::layout::{
    calculate_layout, command_palette_area, help_overlay_area, is_terminal_large_enough,
    pane_to_pty_size, spawn_picker_area, ActiveLayout, MIN_COLS, MIN_ROWS,
};
use crate::ui::pane_manager::PaneManager;
use crate::ui::sidebar::{ProjectAgents, Sidebar, SidebarState};
use crate::ui::spawn_picker::SpawnPicker;
use crate::ui::status_bar::StatusBar;
use crate::ui::terminal_pane::{
    cursor_position, extract_selected_text, EmptyPane, TerminalPane, TextSelection,
};
use crate::ui::theme::Theme;
use chrono::Utc;
use color_eyre::eyre::Result;
use ratatui::prelude::*;
use ratatui::Terminal;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Central application state.
pub struct App {
    /// Configuration.
    config: MaestroConfig,

    /// Agent manager — owns all agent handles.
    agent_manager: AgentManager,

    /// Input handler — mode-aware key dispatch.
    input_handler: InputHandler,

    /// Sidebar UI state (selection, collapse).
    sidebar_state: SidebarState,

    /// Pane manager — layout mode, focus, and agent-pane assignments.
    pane_manager: PaneManager,

    /// Profile manager — workspace profile switching.
    profile_manager: ProfileManager,

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

    /// The last known terminal area (for resize detection).
    last_area: Rect,

    /// Command palette: registered commands.
    palette_commands: Vec<PaletteCommand>,

    /// Command palette: fuzzy matcher.
    palette_matcher: PaletteMatcher,

    /// Command palette: current fuzzy-matched suggestions.
    palette_suggestions: Vec<(usize, i64)>,

    /// Session manager for persistence.
    session_manager: SessionManager,

    /// Active text selection (mouse drag).
    selection: Option<TextSelection>,

    /// Transient status message shown in the status bar (auto-expires).
    status_message: Option<(String, Instant)>,

    /// Pulse animation phase counter (0..7) for WaitingForInput indicators.
    pulse_phase: u8,
}

impl App {
    /// Create a new App from configuration.
    ///
    /// `event_tx` is an unbounded sender used by PTY controllers to send
    /// output events. The App bridges this to the bounded EventBus.
    pub fn new(config: MaestroConfig, event_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        let theme = Theme::from_name(&config.ui.theme.name);
        let initial_layout = match config.ui.default_layout {
            crate::config::settings::LayoutMode::Single => ActiveLayout::Single,
            crate::config::settings::LayoutMode::SplitH => ActiveLayout::SplitHorizontal,
            crate::config::settings::LayoutMode::SplitV => ActiveLayout::SplitVertical,
            crate::config::settings::LayoutMode::Grid => ActiveLayout::Grid,
        };

        let palette_commands = build_command_registry(&config.template);
        let palette_matcher = PaletteMatcher::new();
        let palette_suggestions = palette_matcher.match_commands("", &palette_commands);
        let profile_manager =
            ProfileManager::new(config.profile.clone(), config.active_profile.clone());

        let data_dir = expand_tilde(std::path::Path::new("~/.local/share/maestro"));
        let session_manager = SessionManager::new(&data_dir);

        Self {
            agent_manager: AgentManager::new(&config, event_tx),
            input_handler: InputHandler::new(),
            sidebar_state: SidebarState::new(),
            pane_manager: PaneManager::with_layout(initial_layout),
            profile_manager,
            theme,
            dirty: true,
            running: true,
            quit_pending: false,
            show_help: false,
            last_area: Rect::default(),
            palette_commands,
            palette_matcher,
            palette_suggestions,
            session_manager,
            config,
            selection: None,
            status_message: None,
            pulse_phase: 0,
        }
    }

    // ---- Main Loop ----

    /// Run the main event loop.
    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
        event_bus: &mut EventBus,
    ) -> Result<()> {
        // Compute PTY size from initial terminal dimensions
        let initial_area: Rect = terminal.size()?.into();
        self.last_area = initial_area;
        let pty_size = self.calculate_default_pty_size(initial_area);

        // Try to restore a saved session; fall back to config auto_start agents
        let session_restored = if self.config.session.enabled {
            self.restore_session(pty_size)
        } else {
            false
        };

        if !session_restored {
            let errors = self.agent_manager.spawn_auto_start_agents(pty_size);
            for (name, err) in &errors {
                error!("Failed to start agent {}: {}", name, err);
            }
        }

        // Build initial sidebar
        self.rebuild_sidebar();

        // Populate pane assignments (needed for split/grid layouts)
        self.populate_pane_agents();

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

        // Auto-save session before shutdown
        if self.config.session.enabled {
            self.save_session();
        }

        // Graceful shutdown
        info!("Shutting down...");
        self.agent_manager
            .shutdown_all(Duration::from_secs(5))
            .await;
        info!("All agents terminated");

        Ok(())
    }

    // ---- Event Handling ----

    fn handle_event(
        &mut self,
        event: AppEvent,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        match event {
            AppEvent::Input(InputEvent::Key(key)) => {
                let action = self.input_handler.handle_key(key);
                self.dispatch_action(action)?;

                // In Command mode, every keystroke potentially changes fuzzy
                // suggestions, so update them and mark the frame dirty.
                if let InputMode::Command { ref input, .. } = self.input_handler.mode() {
                    self.palette_suggestions = self
                        .palette_matcher
                        .match_commands(input, &self.palette_commands);
                    self.dirty = true;
                }

                // In Search mode, update the search incrementally as the user types.
                if let InputMode::Search { ref query } = self.input_handler.mode() {
                    let query = query.clone();
                    if let Some(id) = self.get_focused_agent_id() {
                        if let Some(handle) = self.agent_manager.get_mut(id) {
                            handle.start_search(&query);
                        }
                    }
                    self.dirty = true;
                }

                // In Rename mode, every keystroke changes the overlay content.
                if self.input_handler.mode().is_rename() {
                    self.dirty = true;
                }

                // In RenameProject mode, every keystroke changes the overlay content.
                if self.input_handler.mode().is_rename_project() {
                    self.dirty = true;
                }

                // In SpawnPicker mode, every keystroke changes the selection.
                if self.input_handler.mode().is_spawn_picker() {
                    self.dirty = true;
                }

                // In NewProject mode, every keystroke changes the overlay content.
                if self.input_handler.mode().is_new_project() {
                    self.dirty = true;
                }
            }

            AppEvent::Input(InputEvent::Mouse(mouse)) => {
                if self.config.ui.mouse_enabled {
                    let layout = calculate_layout(
                        self.last_area,
                        self.config.ui.sidebar_width,
                        self.pane_manager.layout(),
                    );
                    let action = self.input_handler.handle_mouse(mouse, &layout);
                    self.dispatch_action(action)?;
                }
            }

            AppEvent::PtyOutput { agent_id, data } => {
                if let Some(handle) = self.agent_manager.get_mut(agent_id) {
                    handle.process_output(&data);
                    self.dirty = true;
                }
            }

            AppEvent::PtyEof { agent_id } => {
                debug!("PTY EOF for agent {}", agent_id);
                self.dirty = true;
            }

            AppEvent::AgentStateChanged {
                agent_id,
                old_state,
                new_state,
            } => {
                info!(
                    "Agent {} state: {:?} -> {:?}",
                    agent_id, old_state, new_state
                );
                self.rebuild_sidebar();
                self.dirty = true;
            }

            AppEvent::StateTick => {
                let changes = self.agent_manager.detect_all_states();
                let mut needs_rebuild = !changes.is_empty();

                // Auto-retry Claude agents that failed due to a stale --resume session ID.
                let stale_ids: Vec<AgentId> = changes
                    .iter()
                    .filter(|(_, _, new)| matches!(new, AgentState::Errored { .. }))
                    .filter_map(|(id, _, _)| {
                        self.agent_manager
                            .get(*id)
                            .and_then(|h| h.is_stale_resume_failure(10).then_some(*id))
                    })
                    .collect();

                for id in stale_ids {
                    let pty_size = self.calculate_default_pty_size(self.last_area);
                    match self.agent_manager.retry_without_resume(id, pty_size) {
                        Ok(_new_id) => {
                            info!("Auto-retried stale session agent {}", id);
                            needs_rebuild = true;
                        }
                        Err(e) => {
                            warn!("Failed to auto-retry stale session agent {}: {}", id, e);
                        }
                    }
                }

                if needs_rebuild {
                    self.rebuild_sidebar();
                    self.populate_pane_agents();
                    self.dirty = true;
                }

                // Advance pulse phase and force re-render if any agent is waiting.
                let counts = self.agent_manager.state_counts();
                if counts.waiting > 0 {
                    self.pulse_phase = (self.pulse_phase + 1) % 8;
                    self.dirty = true;
                } else {
                    self.pulse_phase = 0;
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

    // ---- Action Dispatch ----

    fn dispatch_action(&mut self, action: Action) -> Result<()> {
        // Clear selection on view-changing actions (not on selection-related ones)
        if self.selection.is_some() {
            match action {
                Action::StartSelection { .. }
                | Action::UpdateSelection { .. }
                | Action::FinalizeSelection
                | Action::ClearSelection
                | Action::CopySelection
                | Action::None => {}
                _ => {
                    self.selection = None;
                    self.dirty = true;
                }
            }
        }

        match action {
            // Navigation
            Action::SelectNext => {
                self.sidebar_state.select_next();
                self.mark_selected_agent_read();
                self.dirty = true;
            }
            Action::SelectPrev => {
                self.sidebar_state.select_prev();
                self.mark_selected_agent_read();
                self.dirty = true;
            }
            Action::MoveAgentUp => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    self.agent_manager.move_agent_up(id);
                    self.rebuild_sidebar();
                    self.sidebar_state.select_agent(id);
                    self.dirty = true;
                }
            }
            Action::MoveAgentDown => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    self.agent_manager.move_agent_down(id);
                    self.rebuild_sidebar();
                    self.sidebar_state.select_agent(id);
                    self.dirty = true;
                }
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
                self.mark_selected_agent_read();
                self.dirty = true;
            }
            Action::FocusAgent(id) => {
                self.sidebar_state.select_agent(id);
                self.mark_selected_agent_read();
                self.dirty = true;
            }

            // Mode switching
            Action::EnterInsertMode => {
                // In split/grid mode, use the focused pane's agent.
                let agent_id = self.get_pane_agent_id(self.pane_manager.focused_pane());
                if let Some(id) = agent_id {
                    if let Some(handle) = self.agent_manager.get(id) {
                        let agent_name = handle.name().to_string();
                        self.input_handler
                            .set_mode(InputMode::Insert { agent_name });

                        // Resize agent to current focused pane dimensions
                        let focused = self.pane_manager.focused_pane();
                        let layout = calculate_layout(
                            self.last_area,
                            self.config.ui.sidebar_width,
                            self.pane_manager.layout(),
                        );
                        if let Some(pane) = layout.panes.get(focused) {
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
                self.dirty = true;
            }
            Action::OpenCommandPalette => {
                self.input_handler.set_mode(InputMode::Command {
                    input: String::new(),
                    selected: 0,
                });
                self.palette_suggestions = self
                    .palette_matcher
                    .match_commands("", &self.palette_commands);
                self.dirty = true;
            }
            Action::CloseCommandPalette => {
                self.palette_suggestions = self
                    .palette_matcher
                    .match_commands("", &self.palette_commands);
                self.dirty = true;
            }
            Action::ExecuteCommand(ref input, selected) => {
                // Resolve the command to execute: use the selected suggestion's
                // keyword if available, otherwise fall back to the raw input.
                let command =
                    if let Some(&(cmd_idx, _score)) = self.palette_suggestions.get(selected) {
                        if let Some(cmd) = self.palette_commands.get(cmd_idx) {
                            // If the command takes no args, use just the keyword.
                            // If it takes args, replace the first word (typed keyword)
                            // with the selected keyword and keep remaining args.
                            let parts: Vec<&str> = input.split_whitespace().collect();
                            if parts.len() > 1 {
                                format!("{} {}", cmd.keyword, parts[1..].join(" "))
                            } else {
                                cmd.keyword.clone()
                            }
                        } else {
                            input.clone()
                        }
                    } else {
                        input.clone()
                    };
                self.palette_suggestions = self
                    .palette_matcher
                    .match_commands("", &self.palette_commands);
                self.dirty = true;
                match parse_command(&command, &self.agent_manager) {
                    Ok(inner_action) => {
                        return self.dispatch_action(inner_action);
                    }
                    Err(err) => {
                        warn!("Command palette error: {}", err);
                    }
                }
            }

            // Spawn picker
            Action::OpenSpawnPicker => {
                self.input_handler
                    .set_mode(InputMode::SpawnPicker { selected: 0 });
                self.dirty = true;
            }
            Action::CloseSpawnPicker => {
                self.dirty = true;
            }
            Action::SpawnVariant(ref kind) => {
                self.spawn_agent_variant(kind.clone());
                self.dirty = true;
            }

            // Agent lifecycle
            Action::SpawnAgent => {
                self.spawn_quick_agent();
                self.dirty = true;
            }
            Action::KillAgent => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    if self.quit_pending {
                        if let Err(e) = self.agent_manager.kill(id) {
                            warn!("Kill failed: {}", e);
                        }
                        self.agent_manager.remove(id);
                        self.rebuild_sidebar();
                        self.quit_pending = false;
                    } else {
                        self.quit_pending = true;
                    }
                    self.dirty = true;
                }
            }
            Action::RestartAgent => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    let pty_size = self.calculate_default_pty_size(self.last_area);
                    match self.agent_manager.restart(id, pty_size) {
                        Ok(new_id) => {
                            self.rebuild_sidebar();
                            self.sidebar_state.select_agent(new_id);
                            self.populate_pane_agents();
                        }
                        Err(e) => warn!("Restart failed: {}", e),
                    }
                    self.dirty = true;
                }
            }

            // Agent rename
            Action::EnterRenameMode => {
                if let Some(id) = self.sidebar_state.selected_agent_id() {
                    if let Some(handle) = self.agent_manager.get(id) {
                        let current_name = handle.name().to_string();
                        self.input_handler.set_mode(InputMode::Rename {
                            agent_id: id,
                            input: current_name,
                        });
                        self.dirty = true;
                    }
                }
            }
            Action::ConfirmRename {
                ref agent_id,
                ref new_name,
            } => {
                let id = *agent_id;
                let name = new_name.clone();
                match self.agent_manager.rename(id, name) {
                    Ok(old_name) => {
                        info!("Renamed agent '{}' -> '{}'", old_name, new_name);
                        self.rebuild_sidebar();
                    }
                    Err(e) => warn!("Rename failed: {}", e),
                }
                self.dirty = true;
            }
            Action::CancelRename => {
                self.dirty = true;
            }

            // Project rename
            Action::EnterRenameProjectMode => {
                if let Some(project_name) = self.sidebar_state.selected_project_name() {
                    let current_name = project_name.to_string();
                    self.input_handler.set_mode(InputMode::RenameProject {
                        old_name: current_name.clone(),
                        input: current_name,
                    });
                    self.dirty = true;
                }
            }
            Action::ConfirmRenameProject {
                ref old_name,
                ref new_name,
            } => {
                let old = old_name.clone();
                let new = new_name.clone();
                match self.agent_manager.rename_project(&old, &new) {
                    Ok(()) => {
                        // Update the config to keep it in sync
                        if let Some(proj) = self.config.project.iter_mut().find(|p| p.name == old) {
                            proj.name = new.clone();
                        }
                        info!("Renamed project '{}' -> '{}'", old, new);
                        self.rebuild_sidebar();
                    }
                    Err(e) => warn!("Project rename failed: {}", e),
                }
                self.dirty = true;
            }
            Action::CancelRenameProject => {
                self.dirty = true;
            }
            Action::RemoveProject => {
                if let Some(project_name) = self.sidebar_state.selected_project_name() {
                    let name = project_name.to_string();
                    match self.agent_manager.remove_project(&name) {
                        Ok(()) => {
                            self.config.project.retain(|p| p.name != name);
                            self.rebuild_sidebar();
                        }
                        Err(e) => warn!("Delete project failed: {}", e),
                    }
                }
                self.dirty = true;
            }

            // Project lifecycle
            Action::EnterNewProjectMode => {
                self.input_handler.set_mode(InputMode::NewProject {
                    step: NewProjectStep::Name,
                    name: String::new(),
                    path_input: "~/".to_string(),
                    completions: vec![],
                    selected_completion: 0,
                });
                self.dirty = true;
            }
            Action::NewProjectAdvance => {
                // Switch from Name step to Path step and compute initial completions
                if let InputMode::NewProject {
                    ref mut step,
                    ref mut completions,
                    ref path_input,
                    ..
                } = self.input_handler.mode_mut()
                {
                    *step = NewProjectStep::Path;
                    *completions = compute_dir_completions(path_input);
                }
                self.dirty = true;
            }
            Action::NewProjectPathChanged => {
                if let InputMode::NewProject {
                    ref mut completions,
                    ref mut selected_completion,
                    ref path_input,
                    ..
                } = self.input_handler.mode_mut()
                {
                    *completions = compute_dir_completions(path_input);
                    *selected_completion = 0;
                }
                self.dirty = true;
            }
            Action::NewProjectTabComplete => {
                if let InputMode::NewProject {
                    ref mut path_input,
                    ref mut completions,
                    ref mut selected_completion,
                    ..
                } = self.input_handler.mode_mut()
                {
                    if !completions.is_empty() {
                        let idx = *selected_completion;
                        *path_input = format!("{}/", completions[idx]);
                        *completions = compute_dir_completions(path_input);
                        *selected_completion = 0;
                    }
                }
                self.dirty = true;
            }
            Action::CreateProject { ref name, ref path } => {
                let project_name = name.clone();
                let project_path = expand_tilde(std::path::Path::new(path));

                if !project_path.exists() {
                    warn!("Project path does not exist: {}", project_path.display());
                } else {
                    match self.agent_manager.add_empty_project(&project_name) {
                        Ok(()) => {
                            let spawn_cwd = project_path.clone();
                            self.config
                                .project
                                .push(crate::config::settings::ProjectConfig {
                                    name: project_name.clone(),
                                    path: project_path,
                                    agent: vec![],
                                });
                            info!("Created project '{}'", project_name);

                            // Auto-spawn a Terminal shell into the new project
                            let pty_size = self.calculate_default_pty_size(self.last_area);
                            match self.agent_manager.spawn(
                                "Terminal".to_string(),
                                project_name.clone(),
                                self.config.global.default_shell.clone(),
                                vec![],
                                spawn_cwd,
                                std::collections::HashMap::new(),
                                pty_size,
                            ) {
                                Ok(id) => {
                                    info!("Auto-spawned Terminal in project '{}'", project_name);
                                    self.sidebar_state.select_agent(id);
                                    self.populate_pane_agents();
                                }
                                Err(e) => {
                                    warn!("Failed to auto-spawn Terminal: {}", e);
                                }
                            }

                            self.rebuild_sidebar();
                        }
                        Err(e) => warn!("Failed to create project: {}", e),
                    }
                }
                self.dirty = true;
            }

            // PTY interaction — send to the focused pane's agent
            Action::SendToPty(ref bytes) => {
                let agent_id = self.get_pane_agent_id(self.pane_manager.focused_pane());
                debug!(
                    ?agent_id,
                    bytes = ?bytes,
                    focused_pane = self.pane_manager.focused_pane(),
                    "SendToPty action"
                );
                if let Some(id) = agent_id {
                    if let Some(handle) = self.agent_manager.get(id) {
                        if let Err(e) = handle.write_input(bytes) {
                            warn!("PTY write failed: {}", e);
                        }
                    } else {
                        debug!("No agent handle found for id {:?}", id);
                    }
                } else {
                    debug!("No agent in focused pane");
                }
            }

            // Application
            Action::ToggleHelp => {
                self.show_help = !self.show_help;
                self.dirty = true;
            }
            Action::Quit => {
                let counts = self.agent_manager.state_counts();
                let total_alive = counts.running + counts.waiting + counts.idle + counts.spawning;

                if total_alive == 0 || self.quit_pending {
                    self.running = false;
                } else {
                    self.quit_pending = true;
                    self.dirty = true;
                }
            }
            Action::ForceQuit => {
                self.running = false;
            }
            Action::ClearSession => {
                match self.session_manager.clear() {
                    Ok(()) => info!("Session data cleared"),
                    Err(e) => warn!("Failed to clear session: {}", e),
                }
                self.dirty = true;
            }

            // Layout: split / grid / cycle / close
            Action::SplitHorizontal => {
                self.pane_manager.set_layout(ActiveLayout::SplitHorizontal);
                self.populate_pane_agents();
                self.resize_visible_agents(self.last_area);
                self.dirty = true;
            }
            Action::SplitVertical => {
                self.pane_manager.set_layout(ActiveLayout::SplitVertical);
                self.populate_pane_agents();
                self.resize_visible_agents(self.last_area);
                self.dirty = true;
            }
            Action::CyclePaneFocus => {
                self.pane_manager.cycle_focus();
                self.dirty = true;
            }
            Action::CloseSplit => {
                self.pane_manager.close_focused_pane();
                self.resize_visible_agents(self.last_area);
                self.dirty = true;
            }

            // Mouse actions
            Action::SidebarClick { row } => {
                let scroll_offset = self.sidebar_state.scroll_offset();
                let item_index = scroll_offset + row;
                self.sidebar_state.set_selected(item_index);
                self.mark_selected_agent_read();
                self.dirty = true;
            }
            Action::PaneFocusClick { pane_index } => {
                self.pane_manager.set_focused_pane(pane_index);
                // If in insert mode and clicking a different pane, exit insert
                if matches!(self.input_handler.mode(), InputMode::Insert { .. }) {
                    self.input_handler.set_mode(InputMode::Normal);
                }
                self.dirty = true;
            }

            // Scrollback
            Action::ScrollUp => {
                if let Some(id) = self.get_focused_agent_id() {
                    let page_height = self.get_pane_height();
                    if let Some(handle) = self.agent_manager.get_mut(id) {
                        handle.scroll_up(page_height);
                    }
                    self.dirty = true;
                }
            }
            Action::ScrollDown => {
                if let Some(id) = self.get_focused_agent_id() {
                    let page_height = self.get_pane_height();
                    if let Some(handle) = self.agent_manager.get_mut(id) {
                        handle.scroll_down(page_height);
                    }
                    self.dirty = true;
                }
            }
            Action::MouseScrollUp => {
                if let Some(id) = self.get_focused_agent_id() {
                    if let Some(handle) = self.agent_manager.get_mut(id) {
                        handle.mouse_scroll_up(3);
                    }
                    self.dirty = true;
                }
            }
            Action::MouseScrollDown => {
                if let Some(id) = self.get_focused_agent_id() {
                    if let Some(handle) = self.agent_manager.get_mut(id) {
                        handle.mouse_scroll_down(3);
                    }
                    self.dirty = true;
                }
            }

            // Search
            Action::EnterSearchMode => {
                self.input_handler.set_mode(InputMode::Search {
                    query: String::new(),
                });
                self.dirty = true;
            }
            Action::SearchNext => {
                if let Some(id) = self.get_focused_agent_id() {
                    if let Some(handle) = self.agent_manager.get_mut(id) {
                        handle.search_next();
                    }
                    self.dirty = true;
                }
            }
            Action::SearchPrev => {
                if let Some(id) = self.get_focused_agent_id() {
                    if let Some(handle) = self.agent_manager.get_mut(id) {
                        handle.search_prev();
                    }
                    self.dirty = true;
                }
            }

            // Template spawning
            Action::SpawnFromTemplate {
                ref template_name,
                ref agent_name,
                ref project_name,
            } => {
                self.spawn_from_template(template_name, agent_name, project_name);
                self.dirty = true;
            }

            // Profile management
            Action::SwitchProfile { ref profile_name } => {
                let name = profile_name.clone();
                self.switch_profile(&name);
                self.dirty = true;
            }
            Action::ListProfiles => {
                let names = self.profile_manager.available_names();
                let active = self.profile_manager.active_name();
                if names.is_empty() {
                    info!("No profiles defined");
                } else {
                    let list: Vec<String> = names
                        .iter()
                        .map(|n| {
                            if Some(*n) == active {
                                format!("{} (active)", n)
                            } else {
                                n.to_string()
                            }
                        })
                        .collect();
                    info!("Available profiles: {}", list.join(", "));
                }
                self.dirty = true;
            }
            Action::ShowCurrentProfile => {
                match self.profile_manager.active_name() {
                    Some(name) => info!("Active profile: {}", name),
                    None => info!("No active profile (using top-level config)"),
                }
                self.dirty = true;
            }

            // Remaining v0.3+ actions — no-op for now
            Action::ReloadConfig
            | Action::ResizePty(_, _)
            | Action::Tick
            | Action::Resize(_, _) => {}

            // Text selection & copy
            Action::StartSelection {
                pane_index,
                row,
                col,
            } => {
                self.pane_manager.set_focused_pane(pane_index);
                self.selection = Some(TextSelection::new(pane_index, row, col));
                self.dirty = true;
            }
            Action::UpdateSelection { row, col } => {
                if let Some(ref mut sel) = self.selection {
                    sel.end = (row, col);
                    self.dirty = true;
                }
            }
            Action::FinalizeSelection => {
                if let Some(ref sel) = self.selection {
                    if sel.is_empty() {
                        // Just a click (no drag) — clear selection
                        self.selection = None;
                    } else {
                        // Auto-copy to clipboard on selection finalize.
                        // On macOS, Command+C is intercepted by the terminal emulator
                        // and never reaches the app, so auto-copy is the most reliable path.
                        let sel = sel.clone();
                        self.copy_selection_to_clipboard(&sel);
                    }
                }
                self.dirty = true;
            }
            Action::ClearSelection => {
                self.selection = None;
                self.dirty = true;
            }
            Action::CopySelection => {
                if let Some(ref sel) = self.selection {
                    let sel = sel.clone();
                    if self.copy_selection_to_clipboard(&sel) {
                        self.selection = None;
                    }
                }
                self.dirty = true;
            }

            Action::None => {}
        }

        // Reset quit_pending if user did something other than quit/kill
        if !matches!(action, Action::Quit | Action::KillAgent) {
            self.quit_pending = false;
        }

        Ok(())
    }

    /// Set a transient status message that auto-expires after 2 seconds.
    fn set_status_message(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), Instant::now()));
        self.dirty = true;
    }

    /// Copy the given selection's text to the system clipboard.
    /// Handles scrollback view synchronization so the correct content is extracted.
    /// Returns `true` if text was successfully copied.
    fn copy_selection_to_clipboard(&mut self, sel: &TextSelection) -> bool {
        let Some(id) = self.get_pane_agent_id(sel.pane_index) else {
            return false;
        };

        let scroll_offset = self
            .agent_manager
            .get(id)
            .map(|h| h.scroll_offset())
            .unwrap_or(0);

        // Temporarily set scrollback view to match what the user sees,
        // so screen.contents() returns the visually displayed content.
        if scroll_offset > 0 {
            if let Some(handle) = self.agent_manager.get_mut(id) {
                handle.set_scrollback_view(scroll_offset);
            }
        }

        let copied = if let Some(handle) = self.agent_manager.get(id) {
            let text = extract_selected_text(handle.screen(), sel);
            if !text.is_empty() {
                match crate::clipboard::copy_to_clipboard(&text) {
                    Ok(()) => {
                        self.set_status_message("Copied to clipboard");
                        true
                    }
                    Err(msg) => {
                        self.set_status_message(&format!("Copy failed: {}", msg));
                        false
                    }
                }
            } else {
                false
            }
        } else {
            false
        };

        // Reset scrollback view to live
        if scroll_offset > 0 {
            if let Some(handle) = self.agent_manager.get_mut(id) {
                handle.set_scrollback_view(0);
            }
        }

        copied
    }

    /// Expire old status messages (called during render).
    fn expire_status_message(&mut self) {
        if let Some((_, created_at)) = &self.status_message {
            if created_at.elapsed() > Duration::from_secs(2) {
                self.status_message = None;
                self.dirty = true;
            }
        }
    }

    // ---- Rendering ----

    fn render(&mut self, frame: &mut Frame) {
        // Expire old status messages
        self.expire_status_message();

        let area = frame.area();

        // Check minimum size
        if !is_terminal_large_enough(area) {
            let msg = format!(
                "Terminal too small. Need at least {}x{}, have {}x{}.",
                MIN_COLS, MIN_ROWS, area.width, area.height,
            );
            let paragraph = ratatui::widgets::Paragraph::new(msg).alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
            return;
        }

        // Calculate layout
        let layout = calculate_layout(
            area,
            self.config.ui.sidebar_width,
            self.pane_manager.layout(),
        );

        // Render sidebar
        let sidebar = Sidebar::new(&self.theme, self.config.ui.show_uptime, self.pulse_phase);
        frame.render_stateful_widget(sidebar, layout.sidebar, &mut self.sidebar_state);

        // Render terminal pane(s)
        for (i, pane) in layout.panes.iter().enumerate() {
            let is_focused = i == self.pane_manager.focused_pane();
            let agent_id = self.get_pane_agent_id(i);

            match agent_id {
                Some(id) => {
                    // Read scroll offset and set vt100 scrollback view (requires &mut)
                    let scroll_offset = self
                        .agent_manager
                        .get(id)
                        .map(|h| h.scroll_offset())
                        .unwrap_or(0);
                    if scroll_offset > 0 {
                        if let Some(handle) = self.agent_manager.get_mut(id) {
                            handle.set_scrollback_view(scroll_offset);
                        }
                    }

                    // Render the terminal pane widget (immutable borrow)
                    if let Some(handle) = self.agent_manager.get(id) {
                        let terminal_pane = TerminalPane::new(
                            handle.screen(),
                            handle.name(),
                            handle.project_name(),
                            handle.state(),
                            is_focused,
                            &self.theme,
                        )
                        .with_scroll_offset(handle.scroll_offset())
                        .with_search(handle.scrollback().search())
                        .with_selection(self.selection.as_ref().filter(|s| s.pane_index == i))
                        .with_pulse_phase(self.pulse_phase);
                        frame.render_widget(terminal_pane, pane.area);

                        // Set cursor position in Insert Mode (only when not scrolled)
                        if matches!(self.input_handler.mode(), InputMode::Insert { .. })
                            && is_focused
                            && scroll_offset == 0
                        {
                            if let Some((cx, cy)) = cursor_position(handle.screen(), &pane.inner) {
                                frame.set_cursor_position((cx, cy));
                            }
                        }
                    }

                    // Reset vt100 scrollback to live view
                    if scroll_offset > 0 {
                        if let Some(handle) = self.agent_manager.get_mut(id) {
                            handle.set_scrollback_view(0);
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
        let manager_counts = self.agent_manager.state_counts();
        let ui_counts = crate::ui::status_bar::StateCounts {
            spawning: manager_counts.spawning,
            running: manager_counts.running,
            waiting: manager_counts.waiting,
            idle: manager_counts.idle,
            completed: manager_counts.completed,
            errored: manager_counts.errored,
        };
        let flash_msg = self.status_message.as_ref().map(|(msg, _)| msg.as_str());
        let status_bar = StatusBar::new(&ui_counts, self.input_handler.mode(), &self.theme)
            .with_flash_message(flash_msg);
        frame.render_widget(status_bar, layout.status_bar);

        // Render command palette overlay if in Command mode
        if let InputMode::Command {
            ref input,
            selected,
        } = self.input_handler.mode().clone()
        {
            let overlay_area = command_palette_area(area);
            let palette_widget = CommandPalette::new(
                input,
                &self.palette_suggestions,
                selected,
                &self.palette_commands,
                &self.theme,
            );
            frame.render_widget(palette_widget, overlay_area);
        }

        // Render spawn picker overlay if in SpawnPicker mode
        if let InputMode::SpawnPicker { selected } = self.input_handler.mode().clone() {
            let overlay_area = spawn_picker_area(area);
            let picker_widget = SpawnPicker::new(selected, &self.theme);
            frame.render_widget(picker_widget, overlay_area);
        }

        // Render rename overlay if in Rename mode
        if self.input_handler.mode().is_rename() {
            self.render_rename_overlay(frame, area);
        }

        // Render rename-project overlay if in RenameProject mode
        if self.input_handler.mode().is_rename_project() {
            self.render_rename_project_overlay(frame, area);
        }

        // Render new-project overlay if in NewProject mode
        if self.input_handler.mode().is_new_project() {
            self.render_new_project_overlay(frame, area);
        }

        // Render help overlay if active
        if self.show_help {
            self.render_help_overlay(frame, area);
        }
    }

    fn render_help_overlay(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = help_overlay_area(area);

        let clear = ratatui::widgets::Clear;
        frame.render_widget(clear, overlay_area);

        let help_text = vec![
            ("j/k", "Navigate agents"),
            ("J/K", "Navigate projects"),
            ("Alt+J/K", "Reorder agent down/up"),
            ("1-9", "Jump to agent"),
            ("Enter/i", "Enter Insert Mode (type to agent)"),
            ("Ctrl+G", "Exit Insert Mode → Normal"),
            ("n", "Spawn agent (pick type)"),
            ("P", "New project"),
            ("d", "Kill agent (press twice)"),
            ("r", "Restart agent"),
            ("R", "Rename agent"),
            ("F2", "Rename project"),
            ("s/v", "Split horizontal / vertical"),
            ("Tab", "Cycle pane focus"),
            ("Ctrl+W", "Close split"),
            ("Mouse drag", "Select text (auto-copies)"),
            ("Alt+C", "Copy selection"),
            ("Ctrl+Shift+C", "Copy selection"),
            ("Ctrl+U/D", "Scroll up / down"),
            ("/", "Search in output"),
            (":", "Command palette"),
            ("X", "Clear saved session"),
            ("?", "Toggle this help"),
            ("q", "Quit (press twice if agents running)"),
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

    // ---- Session Persistence ----

    /// Save the current session state to disk for later restoration.
    fn save_session(&self) {
        let max_scrollback = self.config.session.max_scrollback_bytes;
        let mut saved_agents = Vec::new();

        for (_project_name, ids) in self.agent_manager.agents_by_project() {
            for &id in ids {
                if let Some(handle) = self.agent_manager.get(id) {
                    // Skip killed/completed agents — they should not be restored
                    if handle.state().is_terminal() {
                        continue;
                    }

                    let params = handle.restart_params();
                    let was_running = handle.state().is_alive();
                    let last_state = handle.state().label().to_string();

                    // Save scrollback bytes (truncated to max_scrollback_bytes)
                    let raw_bytes = handle.scrollback_raw_bytes();
                    let scrollback_to_save = if raw_bytes.len() > max_scrollback {
                        &raw_bytes[raw_bytes.len() - max_scrollback..]
                    } else {
                        raw_bytes
                    };

                    let scrollback_file = if !scrollback_to_save.is_empty() {
                        match self.session_manager.save_scrollback(
                            &params.project_name,
                            &params.name,
                            scrollback_to_save,
                        ) {
                            Ok(filename) => Some(filename),
                            Err(e) => {
                                warn!(
                                    "Failed to save scrollback for {}/{}: {}",
                                    params.project_name, params.name, e
                                );
                                None
                            }
                        }
                    } else {
                        None
                    };

                    saved_agents.push(SavedAgent {
                        name: params.name,
                        project_name: params.project_name,
                        command: params.command,
                        args: params.args,
                        cwd: params.cwd,
                        env: params.env,
                        was_running,
                        scrollback_file,
                        last_state,
                        session_id: handle.session_id().map(String::from),
                    });
                }
            }
        }

        let layout_str = match self.pane_manager.layout() {
            ActiveLayout::Single => "single",
            ActiveLayout::SplitHorizontal => "split-h",
            ActiveLayout::SplitVertical => "split-v",
            ActiveLayout::Grid => "grid",
        };

        let snapshot = SessionSnapshot {
            saved_at: Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            layout: layout_str.to_string(),
            agents: saved_agents,
            config_path: None,
        };

        if let Err(e) = self.session_manager.save(&snapshot) {
            error!("Failed to save session: {}", e);
        }
    }

    /// Attempt to restore a previously saved session.
    ///
    /// Returns `true` if a session was successfully restored (at least one agent
    /// spawned), `false` otherwise (caller should fall back to config auto_start).
    fn restore_session(&mut self, pty_size: portable_pty::PtySize) -> bool {
        let snapshot = match self.session_manager.load() {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                debug!("No saved session found");
                return false;
            }
            Err(e) => {
                warn!("Failed to load saved session: {}", e);
                return false;
            }
        };

        if snapshot.agents.is_empty() {
            info!("Saved session has no agents, using config defaults");
            return false;
        }

        info!(
            "Restoring session from {} with {} agents (layout: {})",
            snapshot.saved_at,
            snapshot.agents.len(),
            snapshot.layout
        );

        // Restore layout
        let layout = match snapshot.layout.as_str() {
            "single" => ActiveLayout::Single,
            "split-h" => ActiveLayout::SplitHorizontal,
            "split-v" => ActiveLayout::SplitVertical,
            "grid" => ActiveLayout::Grid,
            other => {
                warn!(
                    "Unknown layout '{}' in session, defaulting to single",
                    other
                );
                ActiveLayout::Single
            }
        };
        self.pane_manager.set_layout(layout);

        // Restore each agent
        let mut restored_count = 0;
        for saved in &snapshot.agents {
            // Ensure the project exists in display order (ignore duplicate errors)
            let _ = self.agent_manager.add_empty_project(&saved.project_name);

            // Ensure config.project is populated so path lookup works for new agents
            if !self
                .config
                .project
                .iter()
                .any(|p| p.name == saved.project_name)
            {
                self.config
                    .project
                    .push(crate::config::settings::ProjectConfig {
                        name: saved.project_name.clone(),
                        path: saved.cwd.clone(),
                        agent: vec![],
                    });
            }

            // Resume the previous Claude Code conversation if this is a claude command.
            // Use --resume <session-id> for exact session matching when available,
            // falling back to --continue for legacy sessions without a session ID.
            let mut args = saved.args.clone();
            if saved.command.ends_with("claude") {
                // Remove any --session-id flag from the original args (we'll use --resume instead)
                if let Some(pos) = args.iter().position(|a| a == "--session-id") {
                    args.remove(pos); // remove --session-id
                    if pos < args.len() {
                        args.remove(pos); // remove the UUID value
                    }
                }

                if let Some(ref sid) = saved.session_id {
                    // Resume the exact conversation by session ID
                    if !args.iter().any(|a| a == "--resume" || a == "-r") {
                        args.insert(0, sid.clone());
                        args.insert(0, "--resume".to_string());
                    }
                } else if !args.iter().any(|a| a == "--continue" || a == "-c") {
                    // Legacy fallback: resume the most recent conversation in this directory
                    args.insert(0, "--continue".to_string());
                }
            }

            // Spawn the agent with its saved parameters
            let agent_id = match self.agent_manager.spawn(
                saved.name.clone(),
                saved.project_name.clone(),
                saved.command.clone(),
                args,
                saved.cwd.clone(),
                saved.env.clone(),
                pty_size,
            ) {
                Ok(id) => {
                    info!(
                        "Restored agent '{}/{}' (was {})",
                        saved.project_name, saved.name, saved.last_state
                    );
                    restored_count += 1;
                    id
                }
                Err(e) => {
                    warn!(
                        "Failed to restore agent '{}/{}': {}",
                        saved.project_name, saved.name, e
                    );
                    continue;
                }
            };

            // Restore scrollback if available
            if let Some(ref scrollback_file) = saved.scrollback_file {
                match self.session_manager.load_scrollback(scrollback_file) {
                    Ok(data) => {
                        if let Some(handle) = self.agent_manager.get_mut(agent_id) {
                            handle.load_scrollback_history(&data);
                            debug!(
                                "Restored {} bytes of scrollback for '{}'",
                                data.len(),
                                saved.name
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to load scrollback for '{}/{}': {}",
                            saved.project_name, saved.name, e
                        );
                    }
                }
            }
        }

        if restored_count > 0 {
            info!(
                "Session restore complete: {}/{} agents restored",
                restored_count,
                snapshot.agents.len()
            );
            true
        } else {
            warn!("Session restore failed: no agents could be spawned");
            false
        }
    }

    // ---- Helpers ----

    /// Get the agent ID for a given pane index.
    ///
    /// In single-pane mode, returns the sidebar-selected agent.
    /// In split/grid mode, first checks the `PaneManager` for an explicit
    /// assignment, then falls back to the ordered agent list.
    fn get_pane_agent_id(&self, pane_index: usize) -> Option<AgentId> {
        match self.pane_manager.layout() {
            ActiveLayout::Single => self.sidebar_state.selected_agent_id(),
            _ => {
                // Check the pane manager for an explicit assignment first
                if let Some(id) = self.pane_manager.agent_in_pane(pane_index) {
                    return Some(id);
                }

                // Fallback: derive from sidebar selection + agent order
                let all_ids = self.agent_manager.all_agent_ids_ordered();
                let selected_idx = self.sidebar_state.selected_index();

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

    /// Populate pane agent assignments based on the current sidebar
    /// selection and available agents.
    ///
    /// Pane 0 gets the currently selected agent, pane 1 the next, etc.
    /// If there aren't enough agents, extra panes remain empty (None).
    fn populate_pane_agents(&mut self) {
        let all_ids = self.agent_manager.all_agent_ids_ordered();
        let selected_idx = self.sidebar_state.selected_index();

        // Find the flat agent index of the currently selected sidebar item
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

        let start = if found { agent_flat_idx } else { 0 };

        for pane_idx in 0..self.pane_manager.pane_count() {
            let agent_id = all_ids.get(start + pane_idx).copied();
            self.pane_manager.assign_to_pane(pane_idx, agent_id);
        }
    }

    /// Mark the currently selected agent's result as read (clears unread indicator).
    fn mark_selected_agent_read(&mut self) {
        if let Some(id) = self.sidebar_state.selected_agent_id() {
            if let Some(handle) = self.agent_manager.get_mut(id) {
                if handle.has_unread_result() {
                    handle.mark_result_read();
                    self.dirty = true;
                }
            }
        }
    }

    fn rebuild_sidebar(&mut self) {
        let projects: Vec<ProjectAgents> = self
            .agent_manager
            .agents_by_project()
            .iter()
            .map(|(project_name, ids)| {
                let agents = ids
                    .iter()
                    .filter_map(|id| {
                        self.agent_manager.get(*id).map(|handle| {
                            (
                                *id,
                                handle.name().to_string(),
                                handle.state().clone(),
                                handle.uptime(),
                                handle.has_unread_result(),
                            )
                        })
                    })
                    .collect();
                (project_name.clone(), agents)
            })
            .collect();

        self.sidebar_state.rebuild(&projects);
    }

    fn calculate_default_pty_size(&self, area: Rect) -> portable_pty::PtySize {
        let layout = calculate_layout(
            area,
            self.config.ui.sidebar_width,
            self.pane_manager.layout(),
        );
        layout
            .panes
            .first()
            .map(|p| pane_to_pty_size(&p.inner))
            .unwrap_or(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
    }

    fn resize_visible_agents(&mut self, area: Rect) {
        let layout = calculate_layout(
            area,
            self.config.ui.sidebar_width,
            self.pane_manager.layout(),
        );
        for (i, pane) in layout.panes.iter().enumerate() {
            if let Some(id) = self.get_pane_agent_id(i) {
                let size = pane_to_pty_size(&pane.inner);
                if let Some(handle) = self.agent_manager.get_mut(id) {
                    handle.resize(size);
                }
            }
        }
    }

    /// Get the agent ID for the currently focused pane.
    fn get_focused_agent_id(&self) -> Option<AgentId> {
        self.get_pane_agent_id(self.pane_manager.focused_pane())
    }

    /// Get the height of the focused pane in rows.
    fn get_pane_height(&self) -> usize {
        let layout = calculate_layout(
            self.last_area,
            self.config.ui.sidebar_width,
            self.pane_manager.layout(),
        );
        let focused = self.pane_manager.focused_pane();
        layout
            .panes
            .get(focused)
            .map(|p| p.inner.height as usize)
            .unwrap_or(24)
    }

    /// Spawn a new Claude Code agent quickly.
    ///
    /// Spawns into the currently selected project in the sidebar. If a
    /// project header or an agent under a project is selected, uses that
    /// project. Falls back to the first configured project or a default.
    fn spawn_quick_agent(&mut self) {
        // Determine the target project from the sidebar selection
        let selected_project = self.sidebar_state.selected_project_name().map(String::from);

        let (project_name, project_path) = if let Some(ref sel_name) = selected_project {
            // Look up the path for the selected project in config
            if let Some(proj) = self.config.project.iter().find(|p| p.name == *sel_name) {
                (proj.name.clone(), proj.path.clone())
            } else {
                // Project exists at runtime (created via UI) but not in config —
                // use current directory as the working directory
                (
                    sel_name.clone(),
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
                )
            }
        } else if let Some(proj) = self.config.project.first() {
            (proj.name.clone(), proj.path.clone())
        } else {
            (
                "default".to_string(),
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            )
        };

        // Auto-generate a unique agent name: "claude-1", "claude-2", etc.
        let mut counter = self.agent_manager.agent_count() + 1;
        let mut name = format!("claude-{}", counter);
        while self
            .agent_manager
            .find_by_name(&project_name, &name)
            .is_some()
        {
            counter += 1;
            name = format!("claude-{}", counter);
        }

        let command = self.config.global.claude_binary.clone();
        let pty_size = self.calculate_default_pty_size(self.last_area);

        match self.agent_manager.spawn(
            name.clone(),
            project_name,
            command,
            vec![],
            project_path,
            std::collections::HashMap::new(),
            pty_size,
        ) {
            Ok(id) => {
                info!("Quick-spawned agent '{}'", name);
                self.rebuild_sidebar();
                self.sidebar_state.select_agent(id);
                self.populate_pane_agents();
            }
            Err(e) => warn!("Failed to spawn agent: {}", e),
        }
    }

    /// Spawn an agent variant from the spawn picker.
    fn spawn_agent_variant(&mut self, kind: SpawnKind) {
        // Determine target project (same logic as spawn_quick_agent)
        let selected_project = self.sidebar_state.selected_project_name().map(String::from);

        let (project_name, project_path) = if let Some(ref sel_name) = selected_project {
            if let Some(proj) = self.config.project.iter().find(|p| p.name == *sel_name) {
                (proj.name.clone(), proj.path.clone())
            } else {
                (
                    sel_name.clone(),
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
                )
            }
        } else if let Some(proj) = self.config.project.first() {
            (proj.name.clone(), proj.path.clone())
        } else {
            (
                "default".to_string(),
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            )
        };

        // Determine command, args, and name prefix based on SpawnKind
        let (command, args, name_prefix) = match kind {
            SpawnKind::Claude => (self.config.global.claude_binary.clone(), vec![], "claude"),
            SpawnKind::ClaudeYolo => (
                self.config.global.claude_binary.clone(),
                vec!["--dangerously-skip-permissions".to_string()],
                "claudeyolo",
            ),
            SpawnKind::ClaudeYoloWorktree => (
                self.config.global.claude_binary.clone(),
                vec![
                    "--dangerously-skip-permissions".to_string(),
                    "-w".to_string(),
                ],
                "claudeyolo-w",
            ),
            SpawnKind::Terminal => (self.config.global.default_shell.clone(), vec![], "term"),
        };

        // Auto-generate unique name
        let mut counter = self.agent_manager.agent_count() + 1;
        let mut name = format!("{}-{}", name_prefix, counter);
        while self
            .agent_manager
            .find_by_name(&project_name, &name)
            .is_some()
        {
            counter += 1;
            name = format!("{}-{}", name_prefix, counter);
        }

        let pty_size = self.calculate_default_pty_size(self.last_area);

        match self.agent_manager.spawn(
            name.clone(),
            project_name,
            command,
            args,
            project_path,
            std::collections::HashMap::new(),
            pty_size,
        ) {
            Ok(id) => {
                info!("Spawned agent '{}' via spawn picker", name);
                self.rebuild_sidebar();
                self.sidebar_state.select_agent(id);
                self.populate_pane_agents();
            }
            Err(e) => warn!("Failed to spawn agent: {}", e),
        }
    }

    /// Spawn an agent from a named template.
    fn spawn_from_template(&mut self, template_name: &str, agent_name: &str, project_name: &str) {
        let template = self
            .config
            .template
            .iter()
            .find(|t| t.name == template_name);
        match template {
            Some(t) => {
                let pty_size = self.calculate_default_pty_size(self.last_area);
                match self.agent_manager.spawn(
                    agent_name.to_string(),
                    project_name.to_string(),
                    t.command.clone(),
                    t.args.clone(),
                    t.cwd
                        .clone()
                        .unwrap_or_else(|| std::path::PathBuf::from(".")),
                    t.env.clone(),
                    pty_size,
                ) {
                    Ok(id) => {
                        self.rebuild_sidebar();
                        self.sidebar_state.select_agent(id);
                        self.populate_pane_agents();
                    }
                    Err(e) => warn!("Failed to spawn from template '{}': {}", template_name, e),
                }
            }
            None => warn!("Template '{}' not found", template_name),
        }
    }

    /// Render the rename dialog overlay.
    fn render_rename_overlay(&self, frame: &mut Frame, area: Rect) {
        // Small centered overlay
        let overlay_width = 50u16.min(area.width.saturating_sub(4));
        let overlay_height = 3u16;
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

        let clear = ratatui::widgets::Clear;
        frame.render_widget(clear, overlay_area);

        let block = ratatui::widgets::Block::default()
            .title(" Rename Agent ")
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(self.theme.palette_border)
            .style(
                ratatui::style::Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_fg),
            );

        let inner = block.inner(overlay_area);
        block.render(overlay_area, frame.buffer_mut());

        if inner.width < 5 || inner.height < 1 {
            return;
        }

        if let InputMode::Rename { ref input, .. } = self.input_handler.mode() {
            let display = format!(" > {}\u{2588}", input);
            frame
                .buffer_mut()
                .set_string(inner.x, inner.y, &display, self.theme.palette_input);
        }
    }

    /// Render the rename-project dialog overlay.
    fn render_rename_project_overlay(&self, frame: &mut Frame, area: Rect) {
        let overlay_width = 50u16.min(area.width.saturating_sub(4));
        let overlay_height = 3u16;
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

        let clear = ratatui::widgets::Clear;
        frame.render_widget(clear, overlay_area);

        let block = ratatui::widgets::Block::default()
            .title(" Rename Project ")
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(self.theme.palette_border)
            .style(
                ratatui::style::Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_fg),
            );

        let inner = block.inner(overlay_area);
        block.render(overlay_area, frame.buffer_mut());

        if inner.width < 5 || inner.height < 1 {
            return;
        }

        if let InputMode::RenameProject { ref input, .. } = self.input_handler.mode() {
            let display = format!(" > {}\u{2588}", input);
            frame
                .buffer_mut()
                .set_string(inner.x, inner.y, &display, self.theme.palette_input);
        }
    }

    /// Render the new-project dialog overlay.
    fn render_new_project_overlay(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = command_palette_area(area);

        let clear = ratatui::widgets::Clear;
        frame.render_widget(clear, overlay_area);

        let block = ratatui::widgets::Block::default()
            .title(" New Project ")
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(self.theme.palette_border)
            .style(
                ratatui::style::Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_fg),
            );

        let inner = block.inner(overlay_area);
        block.render(overlay_area, frame.buffer_mut());

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        if let InputMode::NewProject {
            ref step,
            ref name,
            ref path_input,
            ref completions,
            selected_completion,
            ..
        } = self.input_handler.mode()
        {
            let mut y = inner.y;

            // Name field
            let name_label = match step {
                NewProjectStep::Name => format!("  Name: {}\u{2588}", name),
                NewProjectStep::Path => format!("  Name: {}", name),
            };
            let name_style = if *step == NewProjectStep::Name {
                self.theme.palette_input
            } else {
                self.theme.palette_description
            };
            frame
                .buffer_mut()
                .set_string(inner.x, y, &name_label, name_style);
            y += 1;

            // Only show path if we're on the Path step
            if *step == NewProjectStep::Path {
                if y < inner.y + inner.height {
                    let path_label = format!("  Path: {}\u{2588}", path_input);
                    frame.buffer_mut().set_string(
                        inner.x,
                        y,
                        &path_label,
                        self.theme.palette_input,
                    );
                    y += 1;
                }

                // Separator
                if y < inner.y + inner.height {
                    let sep = "\u{2500}".repeat(inner.width as usize);
                    frame
                        .buffer_mut()
                        .set_string(inner.x, y, &sep, self.theme.palette_border);
                    y += 1;
                }

                // Completions
                let max_items = (inner.y + inner.height).saturating_sub(y) as usize;
                for (i, completion) in completions.iter().take(max_items).enumerate() {
                    let is_selected = i == *selected_completion;
                    let style = if is_selected {
                        self.theme.palette_selected
                    } else {
                        ratatui::style::Style::default()
                            .bg(self.theme.palette_bg)
                            .fg(self.theme.palette_fg)
                    };

                    let row = y + i as u16;
                    // Fill background for the row
                    for x in inner.x..inner.x + inner.width {
                        if let Some(cell) = frame.buffer_mut().cell_mut((x, row)) {
                            cell.set_style(style);
                        }
                    }

                    let prefix = if is_selected { "  > " } else { "    " };
                    let display = format!("{}{}", prefix, completion);
                    frame.buffer_mut().set_string(inner.x, row, &display, style);
                }
            }
        }
    }

    /// Switch to a named workspace profile.
    fn switch_profile(&mut self, _profile_name: &str) {
        warn!("Profile switching not yet fully implemented");
    }
}

/// Compute directory completions for a partial path input.
///
/// Expands `~`, splits into parent + prefix, lists directories
/// in the parent that match the prefix, and returns display paths
/// (using `~/` prefix for paths under home).
fn compute_dir_completions(partial: &str) -> Vec<String> {
    let expanded = expand_tilde(std::path::Path::new(partial));
    let home_dir = dirs::home_dir();

    // Determine parent directory and prefix to match
    let (parent, prefix) = if partial.ends_with('/') {
        (expanded.as_path(), "")
    } else {
        let parent = expanded.parent().unwrap_or(expanded.as_path());
        let prefix = expanded.file_name().and_then(|f| f.to_str()).unwrap_or("");
        (parent, prefix)
    };

    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(_) => return vec![],
    };

    let mut results: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            // Skip hidden directories
            !name_str.starts_with('.') && name_str.starts_with(prefix)
        })
        .map(|e| {
            let full = e.path();
            // Convert back to ~/... display format if possible
            if let Some(ref home) = home_dir {
                if let Ok(rel) = full.strip_prefix(home) {
                    return format!("~/{}", rel.display());
                }
            }
            full.display().to_string()
        })
        .collect();

    results.sort();
    results.truncate(20);
    results
}
