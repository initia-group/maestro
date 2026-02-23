# Maestro

A terminal user interface (TUI) for managing multiple [Claude Code](https://docs.anthropic.com/en/docs/claude-code) agents across projects.

Maestro replaces the workflow of juggling multiple terminal tabs with a single dashboard that lets you spawn, monitor, and interact with Claude Code agents — all from one screen.

## Features

- **Multi-agent management** — spawn, kill, and restart agents per project
- **Split/grid layouts** — view multiple agent terminals side by side
- **Vim-style navigation** — modal input with Normal, Insert, Command, and Search modes
- **State detection** — automatic detection of agent states (running, waiting, idle, errored)
- **Session persistence** — save and restore your agent sessions
- **Desktop notifications** — get notified when agents need attention
- **Command palette** — fuzzy-search commands with `:`
- **Configurable** — TOML-based config with workspace profiles

## Installation

### Homebrew (macOS / Linux)

```sh
brew tap initia-group/tap
brew install maestro
```

### Pre-built Binaries

Download the latest binary for your platform from [GitHub Releases](https://github.com/initia-group/maestro/releases).

### From Source

Requires [Rust](https://rustup.rs/) 1.75+:

```sh
cargo install --git https://github.com/initia-group/maestro
```

Or clone and build:

```sh
git clone https://github.com/initia-group/maestro.git
cd maestro
cargo build --release
./target/release/maestro
```

## Usage

```sh
# Start with default config (~/.config/maestro/config.toml)
maestro

# Start with a custom config file
maestro -c /path/to/config.toml

# Set log level
maestro --log-level debug
```

## Configuration

Create `~/.config/maestro/config.toml`:

```toml
[global]
claude_binary = "claude"
max_agents = 15

[ui]
fps = 30
sidebar_width = 28
default_layout = "single"
mouse_enabled = true

[[project]]
name = "my-app"
path = "/path/to/my-app"

[[project.agent]]
name = "backend"
command = "claude"
auto_start = true
```

See `config/default.toml` for all available options.

## Key Bindings

| Key | Mode | Action |
|---|---|---|
| `j` / `k` | Normal | Navigate agents |
| `i` | Normal | Enter insert mode (type into agent) |
| `Esc` | Insert | Return to normal mode |
| `:` | Normal | Open command palette |
| `s` | Normal | Spawn new agent |
| `x` | Normal | Kill selected agent |
| `r` | Normal | Restart selected agent |
| `1`-`4` | Normal | Switch layout (single/split-h/split-v/grid) |

## License

[MIT](LICENSE)
