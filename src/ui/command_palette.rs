//! Command palette overlay — fuzzy-search command runner.
//!
//! Provides a VS Code-style command palette with fuzzy matching
//! for agent management, profile switching, and other commands.
//! See Feature 14 (Command Palette) for full implementation.

use crate::agent::manager::AgentManager;
use crate::agent::AgentId;
use crate::config::settings::TemplateConfig;
use crate::input::action::Action;
use crate::ui::theme::Theme;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Clear, Widget};

// ─── Command Registry ─────────────────────────────────────

/// A command that can appear in the palette.
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    /// The command keyword (e.g., "spawn", "kill", "focus").
    pub keyword: String,
    /// Short description shown in the palette.
    pub description: String,
    /// The action template. `{}` placeholders for arguments.
    pub usage: String,
    /// Whether this command takes arguments.
    pub has_args: bool,
    /// Function to parse arguments and produce an Action.
    pub parser: CommandParser,
}

/// How to parse a command string into an Action.
#[derive(Debug, Clone)]
pub enum CommandParser {
    /// No arguments needed. Produces this action directly.
    NoArgs(Action),
    /// Takes arguments. Parsing function is called with the arg string.
    WithArgs {
        /// What kind of arguments (for validation hints).
        arg_type: ArgType,
    },
}

/// The kind of argument a command expects.
#[derive(Debug, Clone)]
pub enum ArgType {
    /// An agent name.
    AgentName,
    /// A template name, agent name, and project path.
    TemplateNameAndAgent,
    /// A layout name.
    LayoutName,
    /// A project name and filesystem path.
    ProjectNameAndPath,
}

/// Build the full list of available commands.
pub fn build_command_registry(templates: &[TemplateConfig]) -> Vec<PaletteCommand> {
    let mut commands = vec![
        PaletteCommand {
            keyword: "new".into(),
            description: "New Claude Code agent".into(),
            usage: "new".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::SpawnAgent),
        },
        PaletteCommand {
            keyword: "spawn".into(),
            description: "Spawn agent from template".into(),
            usage: "spawn <template> <name> <project>".into(),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::TemplateNameAndAgent,
            },
        },
        PaletteCommand {
            keyword: "kill".into(),
            description: "Kill an agent".into(),
            usage: "kill <agent-name>".into(),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::AgentName,
            },
        },
        PaletteCommand {
            keyword: "restart".into(),
            description: "Restart an agent".into(),
            usage: "restart <agent-name>".into(),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::AgentName,
            },
        },
        PaletteCommand {
            keyword: "focus".into(),
            description: "Focus on an agent".into(),
            usage: "focus <agent-name>".into(),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::AgentName,
            },
        },
        PaletteCommand {
            keyword: "rename".into(),
            description: "Rename the selected agent".into(),
            usage: "rename <new-name>".into(),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::AgentName,
            },
        },
        PaletteCommand {
            keyword: "split".into(),
            description: "Change layout".into(),
            usage: "split horizontal|vertical|grid|single".into(),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::LayoutName,
            },
        },
        PaletteCommand {
            keyword: "config".into(),
            description: "Reload configuration".into(),
            usage: "config reload".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::ReloadConfig),
        },
        PaletteCommand {
            keyword: "help".into(),
            description: "Show help".into(),
            usage: "help".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::ToggleHelp),
        },
        PaletteCommand {
            keyword: "project".into(),
            description: "Create a new project".into(),
            usage: "project <name> <path>".into(),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::ProjectNameAndPath,
            },
        },
        PaletteCommand {
            keyword: "rename-project".into(),
            description: "Rename the selected project".into(),
            usage: "rename-project".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::EnterRenameProjectMode),
        },
        PaletteCommand {
            keyword: "delete-project".into(),
            description: "Delete the selected empty project".into(),
            usage: "delete-project".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::RemoveProject),
        },
        PaletteCommand {
            keyword: "quit".into(),
            description: "Quit Maestro".into(),
            usage: "quit".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::Quit),
        },
        PaletteCommand {
            keyword: "session clear".into(),
            description: "Clear saved session data".into(),
            usage: "session clear".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::ClearSession),
        },
    ];

    // Add template-specific spawn commands for convenience
    for template in templates {
        commands.push(PaletteCommand {
            keyword: format!("spawn {}", template.name),
            description: template.description.clone().unwrap_or_default(),
            usage: format!("spawn {} <name> <project>", template.name),
            has_args: true,
            parser: CommandParser::WithArgs {
                arg_type: ArgType::TemplateNameAndAgent,
            },
        });
    }

    commands
}

// ─── Fuzzy Matching ───────────────────────────────────────

/// Fuzzy matcher for palette commands using the Skim V2 algorithm.
pub struct PaletteMatcher {
    matcher: SkimMatcherV2,
}

impl Default for PaletteMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl PaletteMatcher {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Match a query against the command list, returning scored results.
    ///
    /// Each entry is `(command_index, score)`. Results are sorted by score
    /// descending. When the query is empty, all commands are returned with
    /// a score of `0`.
    pub fn match_commands(&self, query: &str, commands: &[PaletteCommand]) -> Vec<(usize, i64)> {
        if query.is_empty() {
            // Show all commands when query is empty
            return (0..commands.len()).map(|i| (i, 0)).collect();
        }

        let mut scored: Vec<(usize, i64)> = commands
            .iter()
            .enumerate()
            .filter_map(|(i, cmd)| {
                // Match against keyword and description
                let keyword_score = self.matcher.fuzzy_match(&cmd.keyword, query);
                let desc_score = self.matcher.fuzzy_match(&cmd.description, query);

                let best = keyword_score.max(desc_score);
                best.map(|score| (i, score))
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored
    }
}

// ─── Command Palette Widget ───────────────────────────────

/// The command palette overlay widget.
pub struct CommandPalette<'a> {
    /// Current input text.
    input: &'a str,
    /// Matched suggestions: (command_index, score).
    suggestions: &'a [(usize, i64)],
    /// Selected suggestion index.
    selected: usize,
    /// Full command list (for looking up by index).
    commands: &'a [PaletteCommand],
    /// Theme.
    theme: &'a Theme,
}

impl<'a> CommandPalette<'a> {
    /// Create a new command palette widget.
    pub fn new(
        input: &'a str,
        suggestions: &'a [(usize, i64)],
        selected: usize,
        commands: &'a [PaletteCommand],
        theme: &'a Theme,
    ) -> Self {
        Self {
            input,
            suggestions,
            selected,
            commands,
            theme,
        }
    }
}

impl<'a> Widget for CommandPalette<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the area first (overlay)
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Command Palette ")
            .borders(Borders::ALL)
            .border_style(self.theme.palette_border)
            .style(
                ratatui::style::Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_fg),
            );

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 10 {
            return;
        }

        // Input line: "> query_text"
        let input_line = format!("> {}", self.input);
        buf.set_string(inner.x + 1, inner.y, &input_line, self.theme.palette_input);

        // Separator
        if inner.height > 1 {
            let sep = "\u{2500}".repeat(inner.width as usize);
            buf.set_string(inner.x, inner.y + 1, &sep, self.theme.palette_border);
        }

        // Suggestions
        let max_suggestions = (inner.height as usize).saturating_sub(2);
        for (i, &(cmd_idx, _score)) in self.suggestions.iter().take(max_suggestions).enumerate() {
            let y = inner.y + 2 + i as u16;
            let cmd = &self.commands[cmd_idx];
            let is_selected = i == self.selected;

            let style = if is_selected {
                self.theme.palette_selected
            } else {
                ratatui::style::Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_fg)
            };

            // Fill background
            for x in inner.x..inner.x + inner.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(style);
                }
            }

            // Command name
            buf.set_string(inner.x + 2, y, &cmd.keyword, style);

            // Description (after keyword)
            let desc_x = inner.x + 2 + cmd.keyword.len() as u16 + 2;
            if desc_x < inner.x + inner.width - 2 {
                buf.set_string(
                    desc_x,
                    y,
                    &cmd.description,
                    if is_selected {
                        style
                    } else {
                        self.theme.palette_description
                    },
                );
            }
        }
    }
}

// ─── Command Parsing ──────────────────────────────────────

/// Parse a command string into an Action.
///
/// The input is the raw text from the command palette. This function
/// splits it by whitespace and dispatches to the appropriate handler
/// based on the keyword.
pub fn parse_command(input: &str, agent_manager: &AgentManager) -> Result<Action, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty command".into());
    }

    let keyword = parts[0];
    let args = &parts[1..];

    match keyword {
        "new" => Ok(Action::SpawnAgent),
        "spawn" => {
            if args.len() < 3 {
                return Err("Usage: spawn <template> <name> <project>".into());
            }
            Ok(Action::SpawnFromTemplate {
                template_name: args[0].to_string(),
                agent_name: args[1].to_string(),
                project_name: args[2].to_string(),
            })
        }
        "kill" => {
            if args.is_empty() {
                return Err("Usage: kill <agent-name>".into());
            }
            match find_agent_by_name(agent_manager, args[0]) {
                Some(_id) => Ok(Action::KillAgent),
                None => Err(format!("Agent '{}' not found", args[0])),
            }
        }
        "restart" => {
            if args.is_empty() {
                return Err("Usage: restart <agent-name>".into());
            }
            match find_agent_by_name(agent_manager, args[0]) {
                Some(id) => Ok(Action::FocusAgent(id)),
                None => Err(format!("Agent '{}' not found", args[0])),
            }
        }
        "focus" => {
            if args.is_empty() {
                return Err("Usage: focus <agent-name>".into());
            }
            match find_agent_by_name(agent_manager, args[0]) {
                Some(id) => Ok(Action::FocusAgent(id)),
                None => Err(format!("Agent '{}' not found", args[0])),
            }
        }
        "rename" => Ok(Action::EnterRenameMode),
        "rename-project" => Ok(Action::EnterRenameProjectMode),
        "split" => {
            if args.is_empty() {
                return Err("Usage: split horizontal|vertical|grid|single".into());
            }
            match args[0] {
                "horizontal" | "h" => Ok(Action::SplitHorizontal),
                "vertical" | "v" => Ok(Action::SplitVertical),
                "single" | "s" => Ok(Action::CloseSplit),
                _ => Err(format!("Unknown layout: {}", args[0])),
            }
        }
        "project" => {
            if args.len() < 2 {
                return Err("Usage: project <name> <path>".into());
            }
            let name = args[0].to_string();
            let path = args[1..].join(" ");
            Ok(Action::CreateProject { name, path })
        }
        "quit" => Ok(Action::Quit),
        "help" => Ok(Action::ToggleHelp),
        "config" if args.first() == Some(&"reload") => Ok(Action::ReloadConfig),
        "session" if args.first() == Some(&"clear") => Ok(Action::ClearSession),
        _ => Err(format!("Unknown command: {}", keyword)),
    }
}

/// Find an agent by name across all projects.
fn find_agent_by_name(manager: &AgentManager, name: &str) -> Option<AgentId> {
    for (_project, ids) in manager.agents_by_project() {
        for &id in ids {
            if let Some(handle) = manager.get(id) {
                if handle.name() == name {
                    return Some(id);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_commands() -> Vec<PaletteCommand> {
        vec![
            PaletteCommand {
                keyword: "spawn".into(),
                description: "Spawn agent from template".into(),
                usage: "spawn <template> <name> <project>".into(),
                has_args: true,
                parser: CommandParser::WithArgs {
                    arg_type: ArgType::TemplateNameAndAgent,
                },
            },
            PaletteCommand {
                keyword: "split".into(),
                description: "Change layout".into(),
                usage: "split horizontal|vertical|grid|single".into(),
                has_args: true,
                parser: CommandParser::WithArgs {
                    arg_type: ArgType::LayoutName,
                },
            },
            PaletteCommand {
                keyword: "quit".into(),
                description: "Quit Maestro".into(),
                usage: "quit".into(),
                has_args: false,
                parser: CommandParser::NoArgs(Action::Quit),
            },
            PaletteCommand {
                keyword: "help".into(),
                description: "Show help".into(),
                usage: "help".into(),
                has_args: false,
                parser: CommandParser::NoArgs(Action::ToggleHelp),
            },
            PaletteCommand {
                keyword: "kill".into(),
                description: "Kill an agent".into(),
                usage: "kill <agent-name>".into(),
                has_args: true,
                parser: CommandParser::WithArgs {
                    arg_type: ArgType::AgentName,
                },
            },
            PaletteCommand {
                keyword: "config".into(),
                description: "Reload configuration".into(),
                usage: "config reload".into(),
                has_args: false,
                parser: CommandParser::NoArgs(Action::ReloadConfig),
            },
        ]
    }

    // ─── Fuzzy Matcher Tests ──────────────────────────────

    #[test]
    fn test_fuzzy_match_keyword() {
        let matcher = PaletteMatcher::new();
        let commands = make_test_commands();
        let results = matcher.match_commands("sp", &commands);
        // "spawn" and "split" should match, "quit" should not
        assert!(results.iter().any(|(i, _)| commands[*i].keyword == "spawn"));
        assert!(results.iter().any(|(i, _)| commands[*i].keyword == "split"));
        assert!(!results.iter().any(|(i, _)| commands[*i].keyword == "quit"));
    }

    #[test]
    fn test_fuzzy_match_empty_query_returns_all() {
        let matcher = PaletteMatcher::new();
        let commands = make_test_commands();
        let results = matcher.match_commands("", &commands);
        assert_eq!(results.len(), commands.len());
    }

    #[test]
    fn test_fuzzy_match_exact() {
        let matcher = PaletteMatcher::new();
        let commands = make_test_commands();
        let results = matcher.match_commands("quit", &commands);
        assert!(!results.is_empty());
        // First result should be "quit" (highest score)
        assert_eq!(commands[results[0].0].keyword, "quit");
    }

    #[test]
    fn test_fuzzy_match_no_results() {
        let matcher = PaletteMatcher::new();
        let commands = make_test_commands();
        let results = matcher.match_commands("zzzznothing", &commands);
        assert!(results.is_empty());
    }

    #[test]
    fn test_fuzzy_match_description() {
        let matcher = PaletteMatcher::new();
        let commands = make_test_commands();
        // "Reload" matches the description of "config"
        let results = matcher.match_commands("Reload", &commands);
        assert!(results
            .iter()
            .any(|(i, _)| commands[*i].keyword == "config"));
    }

    #[test]
    fn test_fuzzy_match_sorted_by_score() {
        let matcher = PaletteMatcher::new();
        let commands = make_test_commands();
        let results = matcher.match_commands("sp", &commands);
        // Scores should be in descending order
        for window in results.windows(2) {
            assert!(window[0].1 >= window[1].1);
        }
    }

    // ─── Default trait ────────────────────────────────────

    #[test]
    fn test_palette_matcher_default() {
        let matcher = PaletteMatcher::default();
        let commands = make_test_commands();
        let results = matcher.match_commands("q", &commands);
        assert!(!results.is_empty());
    }

    // ─── Command Registry Tests ───────────────────────────

    #[test]
    fn test_build_registry_no_templates() {
        let commands = build_command_registry(&[]);
        // Should have the 14 base commands (new, spawn, kill, restart, focus, rename, split, project, rename-project, delete-project, config, help, quit, session clear)
        assert_eq!(commands.len(), 14);
    }

    #[test]
    fn test_build_registry_with_templates() {
        let templates = vec![
            TemplateConfig {
                name: "code-review".into(),
                command: "claude".into(),
                args: vec!["--review".into()],
                description: Some("Run a code review".into()),
                default_project: None,
                env: std::collections::HashMap::new(),
                cwd: None,
                mode: Default::default(),
            },
            TemplateConfig {
                name: "test-runner".into(),
                command: "claude".into(),
                args: vec![],
                description: None,
                default_project: None,
                env: std::collections::HashMap::new(),
                cwd: None,
                mode: Default::default(),
            },
        ];
        let commands = build_command_registry(&templates);
        // 14 base + 2 template-specific
        assert_eq!(commands.len(), 16);

        // Template commands should be present
        assert!(commands.iter().any(|c| c.keyword == "spawn code-review"));
        assert!(commands.iter().any(|c| c.keyword == "spawn test-runner"));
    }

    #[test]
    fn test_registry_contains_all_base_commands() {
        let commands = build_command_registry(&[]);
        let keywords: Vec<&str> = commands.iter().map(|c| c.keyword.as_str()).collect();
        assert!(keywords.contains(&"spawn"));
        assert!(keywords.contains(&"kill"));
        assert!(keywords.contains(&"restart"));
        assert!(keywords.contains(&"focus"));
        assert!(keywords.contains(&"split"));
        assert!(keywords.contains(&"config"));
        assert!(keywords.contains(&"help"));
        assert!(keywords.contains(&"quit"));
    }

    // ─── Command Parsing Tests ────────────────────────────

    #[test]
    fn test_parse_spawn_command() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("spawn reviewer my-review myapp", &manager);
        assert!(matches!(result, Ok(Action::SpawnFromTemplate { .. })));
        if let Ok(Action::SpawnFromTemplate {
            template_name,
            agent_name,
            project_name,
        }) = result
        {
            assert_eq!(template_name, "reviewer");
            assert_eq!(agent_name, "my-review");
            assert_eq!(project_name, "myapp");
        }
    }

    #[test]
    fn test_parse_spawn_missing_args() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("spawn", &manager);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Usage"));
    }

    #[test]
    fn test_parse_empty_command() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("", &manager);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Empty command");
    }

    #[test]
    fn test_parse_quit_command() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("quit", &manager);
        assert_eq!(result, Ok(Action::Quit));
    }

    #[test]
    fn test_parse_help_command() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("help", &manager);
        assert_eq!(result, Ok(Action::ToggleHelp));
    }

    #[test]
    fn test_parse_config_reload() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("config reload", &manager);
        assert_eq!(result, Ok(Action::ReloadConfig));
    }

    #[test]
    fn test_parse_split_horizontal() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        assert_eq!(
            parse_command("split horizontal", &manager),
            Ok(Action::SplitHorizontal)
        );
        assert_eq!(
            parse_command("split h", &manager),
            Ok(Action::SplitHorizontal)
        );
    }

    #[test]
    fn test_parse_split_vertical() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        assert_eq!(
            parse_command("split vertical", &manager),
            Ok(Action::SplitVertical)
        );
        assert_eq!(
            parse_command("split v", &manager),
            Ok(Action::SplitVertical)
        );
    }

    #[test]
    fn test_parse_split_single() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        assert_eq!(
            parse_command("split single", &manager),
            Ok(Action::CloseSplit)
        );
        assert_eq!(parse_command("split s", &manager), Ok(Action::CloseSplit));
    }

    #[test]
    fn test_parse_split_unknown_layout() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("split diagonal", &manager);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown layout"));
    }

    #[test]
    fn test_parse_unknown_command() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("foobar", &manager);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown command"));
    }

    #[test]
    fn test_parse_focus_no_agent() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("focus nonexistent", &manager);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_parse_kill_no_args() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("kill", &manager);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Usage"));
    }

    #[test]
    fn test_build_registry_includes_project_command() {
        let commands = build_command_registry(&[]);
        assert!(commands.iter().any(|c| c.keyword == "project"));
    }

    #[test]
    fn test_parse_project_command() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("project myapp /tmp/myapp", &manager);
        assert!(matches!(result, Ok(Action::CreateProject { .. })));
        if let Ok(Action::CreateProject { name, path }) = result {
            assert_eq!(name, "myapp");
            assert_eq!(path, "/tmp/myapp");
        }
    }

    #[test]
    fn test_parse_project_missing_args() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("project", &manager);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Usage"));
    }

    #[test]
    fn test_parse_project_missing_path() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        let result = parse_command("project myapp", &manager);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_whitespace_trimming() {
        let config = crate::config::settings::MaestroConfig::default();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = AgentManager::new(&config, tx);

        assert_eq!(parse_command("  quit  ", &manager), Ok(Action::Quit));
    }

    // ─── Widget Rendering Tests ───────────────────────────

    #[test]
    fn test_command_palette_widget_renders() {
        let theme = Theme::default_dark();
        let commands = make_test_commands();
        let matcher = PaletteMatcher::new();
        let suggestions = matcher.match_commands("", &commands);

        let widget = CommandPalette::new("", &suggestions, 0, &commands, &theme);

        let area = Rect::new(10, 5, 60, 12);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Verify the title is rendered
        let content = buffer_to_string(&buf);
        assert!(content.contains("Command Palette"));
    }

    #[test]
    fn test_command_palette_widget_shows_input() {
        let theme = Theme::default_dark();
        let commands = make_test_commands();
        let suggestions: Vec<(usize, i64)> = vec![];

        let widget = CommandPalette::new("hello", &suggestions, 0, &commands, &theme);

        let area = Rect::new(0, 0, 60, 12);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(content.contains("> hello"));
    }

    #[test]
    fn test_command_palette_widget_tiny_area_does_not_panic() {
        let theme = Theme::default_dark();
        let commands = make_test_commands();
        let suggestions: Vec<(usize, i64)> = vec![];

        let widget = CommandPalette::new("test", &suggestions, 0, &commands, &theme);

        // Very small area — should not panic
        let area = Rect::new(0, 0, 5, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
    }

    /// Helper to convert a buffer to a string for assertion.
    fn buffer_to_string(buf: &Buffer) -> String {
        let mut s = String::new();
        for y in buf.area.y..buf.area.y + buf.area.height {
            for x in buf.area.x..buf.area.x + buf.area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    s.push_str(cell.symbol());
                }
            }
            s.push('\n');
        }
        s
    }
}
