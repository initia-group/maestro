//! Input mode definitions.
//!
//! Defines the `InputMode` enum (Normal, Insert, Command, Search)
//! and mode transition logic.

/// Which step of the new-project dialog is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewProjectStep {
    /// Entering the project display name.
    Name,
    /// Entering the project directory path (with tab-completion).
    Path,
}

/// The current input mode determines how keyboard events are interpreted.
///
/// Maestro uses a vim-inspired modal system:
/// - **Normal**: Navigation, agent selection, keybindings
/// - **Insert**: All keys forwarded to the focused agent's PTY
/// - **Command**: `:` prefix, text input for command palette
/// - **Search**: `/` prefix, text input for scrollback search
/// - **NewProject**: Two-step dialog for creating a project
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum InputMode {
    /// Default mode — keybindings for navigation, agent lifecycle, etc.
    #[default]
    Normal,

    /// PTY interaction mode — all keys (except Esc/Ctrl+\) forwarded to agent.
    Insert {
        /// Name of the agent whose PTY is receiving input.
        agent_name: String,
    },

    /// Command palette mode — text input with fuzzy-matched suggestions.
    Command {
        /// Current text input buffer.
        input: String,
        /// Index of the currently selected suggestion.
        selected: usize,
    },

    /// Search mode — text input for scrollback search.
    Search {
        /// Current search query.
        query: String,
    },

    /// Rename mode — text input for renaming the selected agent.
    Rename {
        /// The agent being renamed.
        agent_id: crate::agent::AgentId,
        /// Current text input buffer (initialized to old name).
        input: String,
    },

    /// Rename project mode — text input for renaming the selected project.
    RenameProject {
        /// The original project name being renamed.
        old_name: String,
        /// Current text input buffer (initialized to old name).
        input: String,
    },

    /// Spawn picker mode — select what kind of agent to spawn.
    SpawnPicker {
        /// Index of the currently highlighted option (0-3).
        selected: usize,
    },

    /// New project dialog — two-step input (name, then path with completion).
    NewProject {
        /// Which step is active.
        step: NewProjectStep,
        /// Project display name (entered in step 1).
        name: String,
        /// Path input buffer (entered in step 2).
        path_input: String,
        /// Directory completions for the current path_input.
        completions: Vec<String>,
        /// Index of the currently highlighted completion.
        selected_completion: usize,
    },
}

impl InputMode {
    /// Returns a short label for display in the status bar.
    pub fn label(&self) -> &str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert { .. } => "INSERT",
            Self::Command { .. } => "COMMAND",
            Self::Search { .. } => "SEARCH",
            Self::Rename { .. } => "RENAME",
            Self::RenameProject { .. } => "RENAME PROJECT",
            Self::SpawnPicker { .. } => "SPAWN",
            Self::NewProject { .. } => "NEW PROJECT",
        }
    }

    /// Returns `true` if the mode is Normal.
    pub fn is_normal(&self) -> bool {
        matches!(self, Self::Normal)
    }

    /// Returns `true` if the mode is Insert.
    pub fn is_insert(&self) -> bool {
        matches!(self, Self::Insert { .. })
    }

    /// Returns `true` if the mode is Command.
    pub fn is_command(&self) -> bool {
        matches!(self, Self::Command { .. })
    }

    /// Returns `true` if the mode is Search.
    pub fn is_search(&self) -> bool {
        matches!(self, Self::Search { .. })
    }

    /// Returns `true` if the mode is Rename.
    pub fn is_rename(&self) -> bool {
        matches!(self, Self::Rename { .. })
    }

    /// Returns `true` if the mode is RenameProject.
    pub fn is_rename_project(&self) -> bool {
        matches!(self, Self::RenameProject { .. })
    }

    /// Returns `true` if the mode is SpawnPicker.
    pub fn is_spawn_picker(&self) -> bool {
        matches!(self, Self::SpawnPicker { .. })
    }

    /// Returns `true` if the mode is NewProject.
    pub fn is_new_project(&self) -> bool {
        matches!(self, Self::NewProject { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_normal() {
        assert_eq!(InputMode::default(), InputMode::Normal);
    }

    #[test]
    fn labels_are_correct() {
        assert_eq!(InputMode::Normal.label(), "NORMAL");
        assert_eq!(
            InputMode::Insert {
                agent_name: "test".into()
            }
            .label(),
            "INSERT"
        );
        assert_eq!(
            InputMode::Command {
                input: String::new(),
                selected: 0
            }
            .label(),
            "COMMAND"
        );
        assert_eq!(
            InputMode::Search {
                query: String::new()
            }
            .label(),
            "SEARCH"
        );
        assert_eq!(
            InputMode::Rename {
                agent_id: crate::agent::AgentId::new(),
                input: String::new(),
            }
            .label(),
            "RENAME"
        );
        assert_eq!(
            InputMode::RenameProject {
                old_name: String::new(),
                input: String::new(),
            }
            .label(),
            "RENAME PROJECT"
        );
        assert_eq!(
            InputMode::NewProject {
                step: NewProjectStep::Name,
                name: String::new(),
                path_input: String::new(),
                completions: vec![],
                selected_completion: 0,
            }
            .label(),
            "NEW PROJECT"
        );
    }

    #[test]
    fn mode_predicates() {
        assert!(InputMode::Normal.is_normal());
        assert!(!InputMode::Normal.is_insert());

        let insert = InputMode::Insert {
            agent_name: "a".into(),
        };
        assert!(insert.is_insert());
        assert!(!insert.is_normal());

        let cmd = InputMode::Command {
            input: String::new(),
            selected: 0,
        };
        assert!(cmd.is_command());

        let search = InputMode::Search {
            query: String::new(),
        };
        assert!(search.is_search());

        let rename = InputMode::Rename {
            agent_id: crate::agent::AgentId::new(),
            input: String::new(),
        };
        assert!(rename.is_rename());
        assert!(!rename.is_normal());

        let rename_project = InputMode::RenameProject {
            old_name: String::new(),
            input: String::new(),
        };
        assert!(rename_project.is_rename_project());
        assert!(!rename_project.is_normal());

        let new_project = InputMode::NewProject {
            step: NewProjectStep::Name,
            name: String::new(),
            path_input: String::new(),
            completions: vec![],
            selected_completion: 0,
        };
        assert!(new_project.is_new_project());
        assert!(!new_project.is_normal());
    }
}
