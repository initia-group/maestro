# Configuration Reference

Maestro reads its configuration from a TOML file. All settings have sensible defaults, so a config file is optional.

## Config File Location

Resolution order (first found wins):

1. CLI flag: `maestro -c /path/to/config.toml`
2. Default path: `~/.config/maestro/config.toml`
3. Built-in defaults (no file needed)

## Validation

- **Unknown fields are rejected.** A typo like `fsp = 30` instead of `fps = 30` will cause a startup error.
- Regex patterns (in `[detection]`) are compiled at load time. Invalid patterns are skipped with a warning in the log.
- Project names, agent names (within a project), template names, and profile names must be unique.

---

## `[global]`

Global runtime settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `claude_binary` | string | `"claude"` | Path to the `claude` CLI binary. If not absolute, resolved via `PATH`. |
| `default_shell` | string | `"/bin/zsh"` | Default shell for PTY sessions. |
| `max_agents` | integer | `15` | Maximum number of concurrent agents. |
| `log_dir` | path | `"~/.local/share/maestro/logs"` | Log directory. Supports `~` expansion. |
| `state_check_interval_ms` | integer | `250` | State detection tick interval in milliseconds. |
| `idle_timeout_secs` | integer | `3` | Seconds of silence before marking an agent as Idle. |

## `[ui]`

UI appearance and behavior.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `fps` | integer | `30` | Target render frames per second. |
| `sidebar_width` | integer | `28` | Sidebar width in terminal columns. |
| `default_layout` | string | `"single"` | Initial layout mode: `"single"`, `"split-h"`, `"split-v"`, `"grid"`. |
| `show_uptime` | boolean | `true` | Show agent uptime in the sidebar. |
| `mouse_enabled` | boolean | `true` | Enable mouse input (click, drag, scroll). |

## `[ui.theme]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | `"default"` | Built-in theme: `"default"`, `"dark"`, `"light"`, `"gruvbox"`. `"default"` and `"dark"` are the same. |

See [Theming](theming.md) for details on each theme.

## `[[project]]`

Defines a project (directory) containing one or more agents. Can appear multiple times.

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | yes | Display name for the project. Must be unique. |
| `path` | path | yes | Absolute path to the project directory. Supports `~` expansion. |

### `[[project.agent]]`

Defines an agent within a project. Can appear multiple times under a `[[project]]`.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | required | Display name. Must be unique within the project. |
| `command` | string | `global.claude_binary` | Command to run. |
| `args` | array of strings | `[]` | CLI arguments passed to the command. |
| `auto_start` | boolean | `false` | Start this agent automatically when Maestro launches. |
| `cwd` | path | project path | Working directory override. |
| `env` | table | `{}` | Environment variables: `{ KEY = "value" }`. |
| `mode` | string | `"interactive"` | Agent mode: `"interactive"` or `"stream-json"`. |
| `auto_restart` | boolean | `false` | Auto-restart the agent on exit. |
| `max_restarts` | integer | `3` | Maximum number of auto-restart attempts. |
| `restart_delay_secs` | integer | `5` | Base delay in seconds before the first restart. |
| `restart_backoff_multiplier` | float | `2.0` | Exponential backoff multiplier. Delay is `base * multiplier^count`, capped at 300s. |

## `[[template]]`

Reusable agent templates for the command palette. Can appear multiple times.

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | yes | Template name (used in `:spawn <template>`). Must be unique. |
| `command` | string | yes | Command to run. |
| `args` | array of strings | `[]` | CLI arguments. |
| `description` | string | no | Human-readable description shown in the command palette. |
| `default_project` | string | no | Default project name for agents from this template. |
| `env` | table | `{}` | Environment variables. |
| `cwd` | path | no | Working directory override. |
| `mode` | string | `"interactive"` | Agent mode: `"interactive"` or `"stream-json"`. |

## `[[profile]]`

Workspace profiles for switching between different project/agent configurations.

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | yes | Profile name. Must be unique. |
| `description` | string | no | Human-readable description. |
| `project` | array | no | Projects for this profile (same schema as top-level `[[project]]`). |

Switching profiles kills all current agents and spawns the new profile's agents.

## `[detection]`

Custom regex patterns for agent state detection, added alongside the built-in patterns.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `tool_approval_patterns` | array of strings | `[]` | Additional regex patterns for tool approval detection. |
| `error_patterns` | array of strings | `[]` | Additional regex patterns for error detection. |
| `input_prompt_patterns` | array of strings | `[]` | Additional regex patterns for input prompt detection. |
| `ask_user_question_patterns` | array of strings | `[]` | Additional regex patterns for AskUserQuestion detection. |
| `scan_lines` | integer | `10` | Number of bottom screen lines to scan for patterns. |

### Built-in Detection Patterns

These are always active and cannot be disabled:

**Tool Approval** (3 patterns):
- `Allow\s+(\w+)` — matches "Allow Edit to ..."
- `\[Y/n\]` — yes/no prompts (default yes)
- `\[y/N\]` — yes/no prompts (default no)

**Error** (6 patterns):
- `(?i)error:`, `(?i)api\s+error`, `(?i)rate\s+limit`
- `(?i)connection\s+refused`, `(?i)ECONNREFUSED`, `(?i)timeout`

**Input Prompt** (2 patterns):
- `^>\s*$` — bare `>` prompt
- `^\$\s*$` — bare `$` prompt

**AskUserQuestion** (2 patterns):
- `^\s*❯?\s*\d+[.:]\s+.+` — numbered option lines
- `(?i)type something else|other.*free.text` — "Other" option

## `[notifications]`

Desktop notification settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | boolean | `true` | Enable desktop notifications. |
| `cooldown_secs` | integer | `10` | Minimum seconds between notifications for the same agent. |
| `notify_on_input_prompt` | boolean | `false` | Notify when an agent shows an input prompt. Can be noisy. |

## `[session]`

Session persistence settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | boolean | `true` | Enable session save/restore. |
| `autosave_interval_secs` | integer | `60` | Autosave frequency in seconds. |
| `max_scrollback_bytes` | integer | `5242880` | Maximum scrollback buffer size per agent (default 5 MiB). |

## `active_profile`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `active_profile` | string | none | Set the active workspace profile by name. When set, uses that profile's projects instead of the top-level `[[project]]` definitions. |

---

## Complete Example

```toml
[global]
claude_binary = "claude"
default_shell = "/bin/zsh"
max_agents = 15
log_dir = "~/.local/share/maestro/logs"
state_check_interval_ms = 250
idle_timeout_secs = 3

[ui]
fps = 30
sidebar_width = 28
default_layout = "single"
show_uptime = true
mouse_enabled = true

[ui.theme]
name = "gruvbox"

[[project]]
name = "web-app"
path = "~/projects/web-app"

[[project.agent]]
name = "frontend"
command = "claude"
args = ["--model", "opus"]
auto_start = true

[[project.agent]]
name = "backend"
auto_start = false
auto_restart = true
max_restarts = 5
restart_delay_secs = 10

[[project]]
name = "api-server"
path = "~/projects/api"

[[template]]
name = "code-review"
command = "claude"
args = ["--review"]
description = "Run a code review agent"

[[template]]
name = "yolo"
command = "claude"
args = ["--dangerously-skip-permissions"]
description = "Claude with auto-approved tools"

[detection]
tool_approval_patterns = ["approve\\?"]
error_patterns = ["FATAL"]
scan_lines = 10

[notifications]
enabled = true
cooldown_secs = 10
notify_on_input_prompt = false

[session]
enabled = true
autosave_interval_secs = 60
max_scrollback_bytes = 5242880
```
