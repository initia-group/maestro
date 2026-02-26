# Contributing

How to set up, build, test, and contribute to Maestro.

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) 1.75 or later
- Git

### Clone and Build

```sh
git clone https://github.com/initia-group/maestro.git
cd maestro
cargo build
```

### Verify

```sh
cargo test
./target/debug/maestro --version
```

## Project Structure

```
src/
  main.rs                 # CLI entry point, terminal setup
  app.rs                  # App state machine, main event loop
  lib.rs                  # Library re-exports

  agent/                  # Agent management subsystem
    mod.rs                #   Public interface, AgentId type
    manager.rs            #   AgentManager: spawn/kill/restart
    handle.rs             #   AgentHandle: individual agent state
    state.rs              #   AgentState enum (6 states)
    detector.rs           #   Regex-based state detection
    restart.rs            #   Auto-restart with backoff
    scrollback.rs         #   Scrollback buffer
    stream_json.rs        #   Stream-JSON mode parsing

  config/                 # Configuration subsystem
    mod.rs                #   Public interface
    settings.rs           #   Struct hierarchy (serde targets)
    loader.rs             #   TOML loading and validation
    profile.rs            #   Workspace profile switching

  event/                  # Event system
    mod.rs                #   Public interface
    bus.rs                #   EventBus (tokio mpsc channels)
    types.rs              #   AppEvent, InputEvent enums

  input/                  # Input handling
    mod.rs                #   Public interface
    handler.rs            #   Mode-aware key/mouse dispatch
    action.rs             #   Action enum (60+ variants)
    mode.rs               #   InputMode enum (8 modes)

  ui/                     # UI components
    mod.rs                #   Public interface
    layout.rs             #   Layout calculation engine
    pane_manager.rs       #   Pane state and transitions
    sidebar.rs            #   Project tree widget
    terminal_pane.rs      #   vt100 terminal renderer
    status_bar.rs         #   Status bar widget
    command_palette.rs    #   Fuzzy-search command overlay
    spawn_picker.rs       #   Agent type selector
    theme.rs              #   Theme definitions (3 built-in)

  pty/                    # PTY management
    mod.rs                #   Public interface
    spawner.rs            #   PTY creation and process spawn
    controller.rs         #   Async I/O bridge

  session/                # Session persistence
    mod.rs                #   Save/restore to disk

  clipboard.rs            # System clipboard (arboard)
  notification.rs         # Desktop notifications (notify-rust)
  export.rs               # Markdown output export
```

## Building

```sh
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Cross-compile (requires `cross` tool)
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-gnu
```

## Testing

```sh
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run tests for a specific module
cargo test agent::detector

# Run a specific test
cargo test test_detect_tool_approval
```

Tests are organized as inline `#[cfg(test)] mod tests` blocks within each source file. The project uses:
- Standard assertions for unit tests
- `insta` for snapshot testing
- `tempfile` for temporary directories in tests

## Code Quality

Both formatting and linting are enforced in CI. Run these before submitting a PR:

```sh
# Format code
cargo fmt

# Check formatting (CI uses this)
cargo fmt --check

# Run lints
cargo clippy -- -D warnings
```

## Code Conventions

### Structure
- Module doc comments (`//!`) at the top of every file
- Doc comments (`///`) on every public type and method
- Feature spec references in module docs (e.g., "See Feature 05 for the full spec")

### Error Handling
- `color-eyre` for `Result` types and error context
- `tracing` for structured logging (to file, not stdout)
- Log levels: `error` for failures, `warn` for recoverable issues, `info` for lifecycle events, `debug`/`trace` for development

### Async
- Tokio with full features
- `spawn_blocking` for synchronous I/O (PTY reads)
- `tokio::select!` for event multiplexing

### Config
- `#[serde(deny_unknown_fields)]` on config structs — typos are rejected
- `#[serde(default)]` for optional fields with defaults
- `#[serde(flatten)]` for composing nested configs (e.g., `RestartPolicy` in `AgentConfig`)

### State Management
- Event-driven: all state changes flow through `AppEvent` → `Action` → `App::dispatch_action()`
- Hysteresis: state detection preserves timestamps when re-detecting the same state
- Debounce: 2-tick threshold before state transitions to prevent flapping

## Adding a New Feature

1. Write a feature spec in `docs/features/NN-feature-name.md` (optional but recommended)
2. Add types first — event variants in `AppEvent`, action variants in `Action`
3. Implement the subsystem module
4. Wire into `App::run()` action dispatch in `app.rs`
5. Add tests in the module's `#[cfg(test)]` block
6. Update [keybindings](keybindings.md) if adding keyboard shortcuts
7. Update [configuration](configuration.md) if adding config fields
8. Run `cargo fmt` and `cargo clippy -- -D warnings`

## Pull Request Process

1. Branch from `main`
2. Make focused changes — one feature or fix per PR
3. Ensure CI passes: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
4. Write a clear PR description referencing related issues
5. Request review
