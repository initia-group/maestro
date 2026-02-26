//! Action definitions — semantic user intents.
//!
//! The `Action` enum represents all possible user actions that
//! the input handler can produce from keyboard/mouse events.

use crate::agent::AgentId;

/// The kind of agent to spawn from the spawn picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnKind {
    /// Regular Claude Code (`claude`, no extra args).
    Claude,
    /// Claude Code with --dangerously-skip-permissions.
    ClaudeYolo,
    /// Claude Code YOLO + worktree (-w flag).
    ClaudeYoloWorktree,
    /// Plain terminal shell (user's default shell).
    Terminal,
}

/// Semantic actions produced by the input handler.
///
/// Each variant represents a discrete user intent. The `App` dispatches
/// these to the appropriate subsystem (agent manager, layout engine, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// No action — unbound key or ignored input.
    None,

    // ── Navigation ────────────────────────────────────────
    /// Select the next agent in the sidebar list.
    SelectNext,
    /// Select the previous agent in the sidebar list.
    SelectPrev,
    /// Jump to agent by 1-based index (keys 1-9).
    JumpToAgent(usize),
    /// Switch to the next project group.
    NextProject,
    /// Switch to the previous project group.
    PrevProject,
    /// Select a specific agent by ID.
    FocusAgent(AgentId),

    // ── Mode Switching ────────────────────────────────────
    /// Enter Insert mode (PTY interaction) for the selected agent.
    EnterInsertMode,
    /// Exit Insert mode, return to Normal.
    ExitInsertMode,
    /// Open the command palette (`:` or Ctrl+P).
    OpenCommandPalette,
    /// Close the command palette.
    CloseCommandPalette,
    /// Execute a command from the command palette with the given input string
    /// and the index of the currently selected suggestion.
    ExecuteCommand(String, usize),
    /// Enter search mode (`/`).
    EnterSearchMode,
    /// Open the spawn picker overlay.
    OpenSpawnPicker,
    /// Close the spawn picker without spawning.
    CloseSpawnPicker,

    // ── Agent Lifecycle ───────────────────────────────────
    /// Spawn a new agent (using defaults or prompting for template).
    SpawnAgent,
    /// Spawn an agent of the given kind (from the spawn picker).
    SpawnVariant(SpawnKind),

    // ── Project Lifecycle ────────────────────────────────
    /// Enter the two-step new-project dialog.
    EnterNewProjectMode,
    /// Advance from the Name step to the Path step in the new-project dialog.
    NewProjectAdvance,
    /// The path input changed — recompute directory completions.
    NewProjectPathChanged,
    /// Tab-complete the path input with the selected completion.
    NewProjectTabComplete,
    /// Create a new empty project at runtime.
    CreateProject { name: String, path: String },
    /// Enter rename-project mode for the currently selected project.
    EnterRenameProjectMode,
    /// Confirm the project rename with the given new name.
    ConfirmRenameProject { old_name: String, new_name: String },
    /// Cancel the project rename and return to Normal mode.
    CancelRenameProject,
    /// Delete the currently selected project (must be empty).
    RemoveProject,
    /// Move the selected agent one position up within its project.
    MoveAgentUp,
    /// Move the selected agent one position down within its project.
    MoveAgentDown,
    /// Kill the currently selected agent.
    KillAgent,
    /// Restart the currently selected agent.
    RestartAgent,
    /// Enter rename mode for the currently selected agent.
    EnterRenameMode,
    /// Confirm the rename with the given new name.
    ConfirmRename { agent_id: AgentId, new_name: String },
    /// Cancel the rename and return to Normal mode.
    CancelRename,
    /// Spawn an agent from a named template.
    SpawnFromTemplate {
        template_name: String,
        agent_name: String,
        project_name: String,
    },

    // ── PTY Interaction ───────────────────────────────────
    /// Send raw bytes to the focused agent's PTY.
    SendToPty(Vec<u8>),
    /// Resize the PTY to the given (cols, rows).
    ResizePty(u16, u16),

    // ── Layout ────────────────────────────────────────────
    /// Create a horizontal split.
    SplitHorizontal,
    /// Create a vertical split.
    SplitVertical,
    /// Cycle focus between panes.
    CyclePaneFocus,
    /// Close the current split.
    CloseSplit,

    // ── Scrollback ────────────────────────────────────────
    /// Scroll up half a page in the terminal pane.
    ScrollUp,
    /// Scroll down half a page in the terminal pane.
    ScrollDown,
    /// Scroll up a few lines (mouse wheel).
    MouseScrollUp,
    /// Scroll down a few lines (mouse wheel).
    MouseScrollDown,
    /// Jump to the next search match.
    SearchNext,
    /// Jump to the previous search match.
    SearchPrev,

    // ── Profiles ──────────────────────────────────────────
    /// Switch to a named workspace profile.
    SwitchProfile { profile_name: String },
    /// List available profiles (result shown in status bar or overlay).
    ListProfiles,
    /// Show the currently active profile name.
    ShowCurrentProfile,

    // ── Mouse ─────────────────────────────────────────────
    /// User clicked a specific row in the sidebar.
    SidebarClick { row: usize },
    /// User clicked a terminal pane to focus it.
    PaneFocusClick { pane_index: usize },

    // ── Selection & Copy ─────────────────────────────────
    /// Start a text selection at the given pane-relative position.
    StartSelection {
        pane_index: usize,
        row: u16,
        col: u16,
    },
    /// Update the text selection end position (mouse drag).
    UpdateSelection { row: u16, col: u16 },
    /// Finalize the selection (mouse release).
    FinalizeSelection,
    /// Clear any active text selection.
    ClearSelection,
    /// Copy the current selection to the system clipboard.
    CopySelection,

    // ── Application ───────────────────────────────────────
    /// Toggle the help overlay.
    ToggleHelp,
    /// Quit the application.
    Quit,
    /// Force quit without confirmation.
    ForceQuit,
    /// Clear saved session data (snapshot + scrollback files).
    ClearSession,
    /// Reload configuration from disk.
    ReloadConfig,
    /// Tick event for periodic updates (timers, animations).
    Tick,
    /// Terminal resize event.
    Resize(u16, u16),
}

impl Action {
    /// Returns `true` if this action is `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_none_predicate() {
        assert!(Action::None.is_none());
        assert!(!Action::Quit.is_none());
    }

    #[test]
    fn action_equality() {
        assert_eq!(Action::SelectNext, Action::SelectNext);
        assert_ne!(Action::SelectNext, Action::SelectPrev);
    }

    #[test]
    fn action_clone() {
        let action = Action::SendToPty(vec![0x03]);
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }

    #[test]
    fn create_project_carries_fields() {
        let action = Action::CreateProject {
            name: "myapp".into(),
            path: "/tmp/myapp".into(),
        };
        if let Action::CreateProject { name, path } = action {
            assert_eq!(name, "myapp");
            assert_eq!(path, "/tmp/myapp");
        } else {
            panic!("Expected CreateProject");
        }
    }

    #[test]
    fn enter_new_project_mode_action() {
        let action = Action::EnterNewProjectMode;
        assert_eq!(action, Action::EnterNewProjectMode);
    }

    #[test]
    fn jump_to_agent_carries_index() {
        let action = Action::JumpToAgent(5);
        if let Action::JumpToAgent(idx) = action {
            assert_eq!(idx, 5);
        } else {
            panic!("Expected JumpToAgent");
        }
    }
}
