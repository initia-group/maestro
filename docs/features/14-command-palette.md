# Feature 14: Command Palette (v0.2)

## Overview

Implement a fuzzy-searchable command palette overlay (triggered by `:` or `Ctrl+p`) that provides a unified interface for all Maestro commands. Users type a command, get fuzzy-matched suggestions, and execute with Enter. This is the power-user interface for operations like spawning from templates, killing specific agents, and configuration management.

## Dependencies

- **Feature 07** (Input Handling) — Command Mode already defined.
- **Feature 08** (Theme & Layout) — `command_palette_area()` for overlay positioning.
- **Feature 12** (App Bootstrap) — `App` dispatches command results.

## Technical Specification

### Command Registry

All available commands are registered in a centralized registry.

```rust
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

#[derive(Debug, Clone)]
pub enum ArgType {
    AgentName,
    TemplateNameAndAgent,
    ProjectPath,
    LayoutName,
}

/// Build the full list of available commands.
pub fn build_command_registry(
    templates: &[TemplateConfig],
    agents: &[(String, String)], // (project, agent_name) pairs
) -> Vec<PaletteCommand> {
    let mut commands = vec![
        PaletteCommand {
            keyword: "spawn".into(),
            description: "Spawn agent from template".into(),
            usage: "spawn <template> <name> <project>".into(),
            has_args: true,
            parser: CommandParser::WithArgs { arg_type: ArgType::TemplateNameAndAgent },
        },
        PaletteCommand {
            keyword: "kill".into(),
            description: "Kill an agent".into(),
            usage: "kill <agent-name>".into(),
            has_args: true,
            parser: CommandParser::WithArgs { arg_type: ArgType::AgentName },
        },
        PaletteCommand {
            keyword: "restart".into(),
            description: "Restart an agent".into(),
            usage: "restart <agent-name>".into(),
            has_args: true,
            parser: CommandParser::WithArgs { arg_type: ArgType::AgentName },
        },
        PaletteCommand {
            keyword: "focus".into(),
            description: "Focus on an agent".into(),
            usage: "focus <agent-name>".into(),
            has_args: true,
            parser: CommandParser::WithArgs { arg_type: ArgType::AgentName },
        },
        PaletteCommand {
            keyword: "split".into(),
            description: "Change layout".into(),
            usage: "split horizontal|vertical|grid|single".into(),
            has_args: true,
            parser: CommandParser::WithArgs { arg_type: ArgType::LayoutName },
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
            keyword: "quit".into(),
            description: "Quit Maestro".into(),
            usage: "quit".into(),
            has_args: false,
            parser: CommandParser::NoArgs(Action::Quit),
        },
    ];

    // Add template-specific spawn commands for convenience
    for template in templates {
        commands.push(PaletteCommand {
            keyword: format!("spawn {}", template.name),
            description: template.description.clone().unwrap_or_default(),
            usage: format!("spawn {} <name> <project>", template.name),
            has_args: true,
            parser: CommandParser::WithArgs { arg_type: ArgType::TemplateNameAndAgent },
        });
    }

    commands
}
```

### Fuzzy Matching

Use `fuzzy-matcher` crate with the Skim V2 algorithm for fast, quality fuzzy matching:

```rust
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct PaletteMatcher {
    matcher: SkimMatcherV2,
}

impl PaletteMatcher {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Match a query against the command list, returning scored results.
    pub fn match_commands(
        &self,
        query: &str,
        commands: &[PaletteCommand],
    ) -> Vec<(usize, i64)> {
        if query.is_empty() {
            // Show all commands when query is empty
            return (0..commands.len()).map(|i| (i, 0)).collect();
        }

        let mut scored: Vec<(usize, i64)> = commands
            .iter()
            .enumerate()
            .filter_map(|(i, cmd)| {
                // Match against keyword, description, and usage
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
```

### Command Palette Widget (`src/ui/command_palette.rs`)

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};
use crate::ui::theme::Theme;

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

impl<'a> Widget for CommandPalette<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the area first (overlay)
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Command Palette ")
            .borders(Borders::ALL)
            .border_style(self.theme.palette_border)
            .style(ratatui::style::Style::default()
                .bg(self.theme.palette_bg)
                .fg(self.theme.palette_fg));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 10 {
            return;
        }

        // Input line: "> query_text"
        let input_line = format!("> {}", self.input);
        let cursor_x = inner.x + 2 + self.input.len() as u16;
        buf.set_string(inner.x + 1, inner.y, &input_line, self.theme.palette_input);

        // Separator
        if inner.height > 1 {
            let sep = "─".repeat(inner.width as usize);
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
                buf.get_mut(x, y).set_style(style);
            }

            // Command name
            buf.set_string(inner.x + 2, y, &cmd.keyword, style);

            // Description (right-aligned or after keyword)
            let desc_x = inner.x + 2 + cmd.keyword.len() as u16 + 2;
            if desc_x < inner.x + inner.width - 2 {
                buf.set_string(
                    desc_x, y,
                    &cmd.description,
                    if is_selected { style } else { self.theme.palette_description },
                );
            }
        }
    }
}
```

### Command Parsing

When the user presses Enter, the input string is parsed into an Action:

```rust
/// Parse a command string into an Action.
pub fn parse_command(
    input: &str,
    commands: &[PaletteCommand],
    agent_manager: &AgentManager,
) -> Result<Action, String> {
    let parts: Vec<&str> = input.trim().split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty command".into());
    }

    let keyword = parts[0];
    let args = &parts[1..];

    match keyword {
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
            // Find agent by name
            match find_agent_by_name(agent_manager, args[0]) {
                Some(id) => Ok(Action::FocusAgent(id)), // Focus first, then kill
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
        "quit" => Ok(Action::Quit),
        "help" => Ok(Action::ToggleHelp),
        "config" if args.first() == Some(&"reload") => Ok(Action::ReloadConfig),
        _ => Err(format!("Unknown command: {}", keyword)),
    }
}

fn find_agent_by_name(manager: &AgentManager, name: &str) -> Option<AgentId> {
    // Search across all projects for an agent with this name
    // Fuzzy match if no exact match found
    for (project, ids) in manager.agents_by_project() {
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
```

## Implementation Steps

1. **Create `src/ui/command_palette.rs`**
   - `PaletteCommand` struct and `CommandParser` enum.
   - `build_command_registry()` function.
   - `PaletteMatcher` with fuzzy matching.
   - `CommandPalette` widget.
   - `parse_command()` function.

2. **Integrate with `App`**
   - On `Action::OpenCommandPalette`: build command list, switch to Command mode.
   - On Command mode input events: update query, re-run fuzzy matching.
   - On Enter: parse command, dispatch action.
   - On Esc: close palette.

3. **Render overlay**
   - In `App::render()`, if in Command mode, render `CommandPalette` over the main area.

## Error Handling

| Scenario | Handling |
|---|---|
| Unknown command | Show error message in the palette (red text). |
| Missing arguments | Show usage hint inline. |
| Agent not found | Show "Agent 'X' not found" error. |
| Template not found | Show "Template 'X' not found" error. |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_fuzzy_match_keyword() {
    let matcher = PaletteMatcher::new();
    let commands = vec![
        PaletteCommand { keyword: "spawn".into(), .. },
        PaletteCommand { keyword: "split".into(), .. },
        PaletteCommand { keyword: "quit".into(), .. },
    ];
    let results = matcher.match_commands("sp", &commands);
    // "spawn" and "split" should match, "quit" should not
    assert!(results.iter().any(|(i, _)| commands[*i].keyword == "spawn"));
    assert!(results.iter().any(|(i, _)| commands[*i].keyword == "split"));
}

#[test]
fn test_parse_spawn_command() {
    let result = parse_command("spawn reviewer my-review myapp", &[], &mock_manager());
    assert!(matches!(result, Ok(Action::SpawnFromTemplate { .. })));
}

#[test]
fn test_parse_empty_command() {
    let result = parse_command("", &[], &mock_manager());
    assert!(result.is_err());
}
```

## Acceptance Criteria

- [ ] `:` or `Ctrl+p` opens the command palette as a centered overlay.
- [ ] Typing filters commands with fuzzy matching.
- [ ] Up/Down arrows (or Ctrl+n/p) navigate suggestions.
- [ ] Enter executes the selected command.
- [ ] Esc closes the palette without executing.
- [ ] All built-in commands are available: spawn, kill, restart, focus, split, config, help, quit.
- [ ] Template names appear as spawn shortcuts.
- [ ] Invalid commands show clear error messages.
- [ ] Palette renders correctly over the terminal pane.
