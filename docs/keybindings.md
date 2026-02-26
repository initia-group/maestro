# Keybindings

Maestro uses vim-style modal input with 8 input modes. The current mode is shown in the status bar.

## Mode Overview

| Mode | Status Bar | How to Enter | How to Exit |
|------|-----------|--------------|-------------|
| Normal | `-- NORMAL --` | Default mode | — |
| Insert | `-- INSERT --` | `i` or `Enter` | `Ctrl+G` |
| Command | `-- COMMAND --` | `:` or `Ctrl+P` | `Esc` |
| Search | `-- SEARCH --` | `/` | `Esc` or `Enter` |
| Spawn Picker | `-- SPAWN --` | `n` | `Esc` |
| Rename | `-- RENAME --` | `R` | `Esc` or `Enter` |
| Rename Project | `-- RENAME PROJECT --` | `F2` | `Esc` or `Enter` |
| New Project | `-- NEW PROJECT --` | `P` | `Esc` or `Enter` |

---

## Global (All Modes)

| Key | Action |
|-----|--------|
| `Alt+C` | Copy text selection to clipboard |
| `Ctrl+Shift+C` | Copy text selection to clipboard (alternative) |

## Normal Mode

### Navigation

| Key | Action |
|-----|--------|
| `j` / `Down` | Select next agent in sidebar |
| `k` / `Up` | Select previous agent in sidebar |
| `J` (Shift+J) | Jump to next project group |
| `K` (Shift+K) | Jump to previous project group |
| `Alt+j` | Reorder agent down within project |
| `Alt+k` | Reorder agent up within project |
| `1`-`9` | Jump to agent by index |

### Mode Switching

| Key | Action |
|-----|--------|
| `i` / `Enter` | Enter Insert mode (interact with agent PTY) |
| `:` / `Ctrl+P` | Open command palette |
| `/` | Enter Search mode |

### Agent Lifecycle

| Key | Action |
|-----|--------|
| `n` | Open spawn picker (new agent) |
| `d` | Kill selected agent |
| `r` | Restart selected agent |
| `R` (Shift+R) | Rename selected agent |

### Project Management

| Key | Action |
|-----|--------|
| `P` (Shift+P) | Create new project |
| `F2` | Rename selected project |

### Layout

| Key | Action |
|-----|--------|
| `s` | Split horizontal (2 panes stacked) |
| `v` | Split vertical (2 panes side by side) |
| `Tab` | Cycle focus between panes |
| `Ctrl+W` | Close split (return to single pane) |

### Scrollback

| Key | Action |
|-----|--------|
| `Ctrl+U` | Scroll up half page |
| `Ctrl+D` | Scroll down half page |

### Application

| Key | Action |
|-----|--------|
| `X` (Shift+X) | Clear saved session |
| `?` | Toggle help overlay |
| `q` | Quit Maestro |

---

## Insert Mode

In Insert mode, all keys are forwarded directly to the agent's PTY. This means you interact with Claude Code (or whatever command the agent runs) exactly as if you were in a normal terminal.

| Key | Action |
|-----|--------|
| `Ctrl+G` | Exit Insert mode, return to Normal |
| All other keys | Forwarded to the agent PTY |

**Important:** `Esc` is forwarded to the PTY, not intercepted by Maestro. Claude Code uses `Esc` internally for its own UI. `Ctrl+G` is the only way to return to Normal mode.

---

## Command Mode

Opened via `:` or `Ctrl+P`. Shows the command palette overlay with fuzzy search.

| Key | Action |
|-----|--------|
| `Esc` | Close palette, return to Normal |
| `Enter` | Execute selected command |
| `Down` / `Ctrl+N` | Next suggestion |
| `Up` / `Ctrl+P` | Previous suggestion |
| `Backspace` | Delete character from query |
| Any printable character | Append to query |

See [Commands](commands.md) for the full command list.

---

## Search Mode

Opened via `/`. Searches the terminal output of the focused agent.

| Key | Action |
|-----|--------|
| `Esc` | Cancel search, return to Normal |
| `Enter` | Confirm search, jump to match |
| `n` | Next match |
| `N` (Shift+N) | Previous match |
| Any printable character | Append to search query |

---

## Spawn Picker Mode

Opened via `n`. Shows a quick-select overlay for agent types.

| Key | Action |
|-----|--------|
| `Esc` | Close picker |
| `j` / `Down` | Next option |
| `k` / `Up` | Previous option |
| `Enter` | Spawn selected variant |
| `1` | Claude Code (regular) |
| `2` | Claude YOLO (auto-approve tools) |
| `3` | Claude YOLO in worktree |
| `4` | Terminal (plain shell) |

---

## Rename Mode

Opened via `R` (Shift+R). Renames the selected agent.

| Key | Action |
|-----|--------|
| `Esc` | Cancel rename |
| `Enter` | Confirm rename |
| `Backspace` | Delete character |
| `Ctrl+U` | Clear input |
| Any printable character | Edit name |

---

## Rename Project Mode

Opened via `F2`. Renames the selected project.

| Key | Action |
|-----|--------|
| `Esc` | Cancel rename |
| `Enter` | Confirm rename |
| `Backspace` | Delete character |
| `Ctrl+U` | Clear input |
| Any printable character | Edit name |

---

## New Project Mode

Opened via `P` (Shift+P). Two-step dialog: first enter the project name, then the path.

| Key | Action |
|-----|--------|
| `Esc` | Cancel |
| `Enter` | Advance to next step / confirm |
| `Tab` | Autocomplete path (in path step) |
| `Backspace` | Delete character |
| Any printable character | Edit input |

---

## Mouse

| Action | Behavior |
|--------|----------|
| Left click on sidebar | Select agent |
| Left click on pane border | Focus that pane |
| Left click + drag in pane | Select text |
| Release left click | Finalize text selection |
| Scroll up/down over pane | Scroll terminal output |
