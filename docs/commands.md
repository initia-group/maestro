# Command Palette

The command palette provides fuzzy-searchable access to all Maestro commands.

## Opening

- Press `:` or `Ctrl+P` in Normal mode
- Type to fuzzy-search commands
- `Up`/`Down` or `Ctrl+P`/`Ctrl+N` to navigate suggestions
- `Enter` to execute, `Esc` to cancel

## Fuzzy Matching

The palette uses the Skim V2 algorithm (same as fzf) to match your query against both the command keyword and its description. Results are sorted by match score. An empty query shows all commands.

---

## Built-in Commands

| Command | Arguments | Description |
|---------|-----------|-------------|
| `new` | — | Open the spawn picker to create a new agent |
| `spawn` | `<template> <name> <project>` | Spawn an agent from a named template |
| `kill` | `<agent-name>` | Kill an agent by display name |
| `restart` | `<agent-name>` | Restart an agent by display name |
| `focus` | `<agent-name>` | Focus/select an agent by display name |
| `rename` | `<new-name>` | Rename the currently selected agent |
| `rename-project` | — | Enter rename mode for the selected project |
| `split` | `horizontal\|vertical\|grid\|single` | Change the pane layout |
| `project` | `<name> <path>` | Create a new project at runtime |
| `config` | `reload` | Reload configuration from disk |
| `session` | `clear` | Clear saved session data |
| `help` | — | Toggle the help overlay |
| `quit` | — | Quit Maestro |

**Total:** 13 base commands + 1 additional command per configured template.

---

## Command Details

### `spawn <template> <name> <project>`

Spawns a new agent from a template defined in your config.

- `template` must match a `[[template]]` name from your config
- `name` is the display name for the new agent
- `project` must match an existing project name

Example: `spawn code-review my-review web-app`

### `kill <agent-name>` / `restart <agent-name>` / `focus <agent-name>`

Operates on an agent by its display name. Searches across all projects. Returns an error if the agent is not found.

### `split <layout>`

Changes the pane layout. Accepts:

| Argument | Alias | Layout |
|----------|-------|--------|
| `horizontal` | `h` | Two panes stacked vertically |
| `vertical` | `v` | Two panes side by side |
| `grid` | — | 2x2 grid (4 panes) |
| `single` | `s` | Single pane |

### `project <name> <path>`

Creates a new empty project at runtime. The path supports spaces (remaining arguments after the name are joined).

Example: `project my-app /home/user/my app`

### `config reload`

Reloads the configuration from disk. Applies changes to UI settings, templates, and detection patterns without restarting.

### `session clear`

Removes all saved session data. On next restart, Maestro will start fresh instead of restoring the previous session.

---

## Template Commands

Each `[[template]]` defined in your config adds a convenience entry in the palette. For example, if you have:

```toml
[[template]]
name = "code-review"
command = "claude"
args = ["--review"]
description = "Run a code review"
```

Then `spawn code-review` appears as a separate palette entry with the description "Run a code review".
