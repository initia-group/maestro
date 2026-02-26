# Getting Started

Get Maestro up and running in 5 minutes.

## Prerequisites

- **Rust 1.75+** (only for building from source)
- **Claude Code CLI** installed and on `PATH` — verify with `claude --version`
- **macOS or Linux** (Intel or ARM)
- Minimum terminal size: **60 columns x 10 rows**

## Installation

### Homebrew (macOS / Linux)

```sh
brew tap initia-group/tap
brew install maestro
```

### Pre-built Binaries

Download the latest binary for your platform from [GitHub Releases](https://github.com/initia-group/maestro/releases).

Supported platforms:
- macOS ARM (Apple Silicon)
- macOS Intel (x86_64)
- Linux ARM (aarch64)
- Linux Intel (x86_64)

### From Source

```sh
# One-liner install
cargo install --git https://github.com/initia-group/maestro

# Or clone and build
git clone https://github.com/initia-group/maestro.git
cd maestro
cargo build --release
./target/release/maestro
```

## First Run

Run Maestro with no config — it uses built-in defaults:

```sh
maestro
```

You'll see an empty sidebar on the left, a terminal pane on the right, and a status bar at the bottom showing `-- NORMAL --`.

## Creating Your First Config

Create `~/.config/maestro/config.toml`:

```toml
[[project]]
name = "my-app"
path = "~/projects/my-app"

[[project.agent]]
name = "dev"
command = "claude"
auto_start = true
```

Restart Maestro. The `dev` agent spawns automatically under `my-app` in the sidebar.

## Essential Workflow

1. **Navigate** — `j`/`k` to move between agents in the sidebar
2. **Interact** — `i` or `Enter` to enter Insert mode, type directly into the agent
3. **Return** — `Ctrl+G` to exit Insert mode back to Normal
4. **Spawn** — `n` to open the spawn picker for a new agent
5. **Kill** — `d` to kill the selected agent
6. **Restart** — `r` to restart the selected agent
7. **Layouts** — `s` for horizontal split, `v` for vertical split, `Tab` to cycle panes
8. **Commands** — `:` to open the command palette
9. **Help** — `?` to toggle the help overlay
10. **Quit** — `q` to quit

See [Keybindings](keybindings.md) for the complete reference.

## CLI Options

```
maestro [OPTIONS]

Options:
  -c, --config <PATH>      Path to config file
      --log-level <LEVEL>   Log level: trace, debug, info, warn, error [default: info]
      --version             Show version
  -h, --help                Show help
```

## File Locations

| Purpose | Path |
|---------|------|
| Config | `~/.config/maestro/config.toml` |
| Logs | `~/.local/share/maestro/logs/maestro.log` (daily rolling) |
| Sessions | `~/.local/share/maestro/sessions/` |
| Exports | `~/.local/share/maestro/exports/` |

## Next Steps

- [Configuration Reference](configuration.md) — every config option explained
- [Keybindings](keybindings.md) — full keyboard reference
- [Commands](commands.md) — command palette reference
- [Architecture](architecture.md) — how Maestro works internally
