# Feature 03: Core Types & Event System

## Overview

Define the fundamental types that form the communication backbone of Maestro: events (things that happen), actions (things the user wants to do), input modes (UI state), and the central event bus that multiplexes all async sources into a single stream. This feature establishes the vocabulary that every other module speaks.

## Dependencies

- **Feature 01** (Project Scaffold) — module structure must exist.
- **Feature 02** (Configuration System) — needed for config-driven intervals (FPS, state check interval).

## Technical Specification

### Event Types (`src/event/types.rs`)

Events are things that **happen** — they flow from the outside world (keyboard, PTY output, timers) into the application.

```rust
use crossterm::event::KeyEvent;
use uuid::Uuid;

/// All events that the application can receive.
#[derive(Debug)]
pub enum AppEvent {
    /// A keyboard/mouse event from the terminal.
    Input(InputEvent),

    /// Raw bytes received from an agent's PTY.
    PtyOutput {
        agent_id: Uuid,
        data: Vec<u8>,
    },

    /// An agent's PTY has closed (process exited or PTY error).
    PtyEof {
        agent_id: Uuid,
    },

    /// An agent's state has changed (detected by the state detector).
    AgentStateChanged {
        agent_id: Uuid,
        old_state: crate::agent::state::AgentState,
        new_state: crate::agent::state::AgentState,
    },

    /// Periodic tick for state detection.
    /// Fires every `state_check_interval_ms` (default 250ms).
    StateTick,

    /// Render tick — signals that a frame should be drawn.
    /// Only fires when something is dirty (not a fixed timer).
    RenderRequest,

    /// Terminal window was resized.
    Resize {
        cols: u16,
        rows: u16,
    },

    /// Request to quit the application.
    QuitRequested,
}

/// Input events from crossterm, abstracted slightly.
#[derive(Debug)]
pub enum InputEvent {
    Key(KeyEvent),
    // Mouse support reserved for v0.2
    // Mouse(MouseEvent),
}
```

### Action Types (`src/input/action.rs`)

Actions are the **result** of processing input events through the current mode. They represent user intent.

```rust
use uuid::Uuid;

/// Actions that can be triggered by user input or commands.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // ─── Navigation ────────────────────────────────
    /// Move sidebar selection down by one.
    SelectNext,
    /// Move sidebar selection up by one.
    SelectPrev,
    /// Jump to the next project group.
    NextProject,
    /// Jump to the previous project group.
    PrevProject,
    /// Jump to agent by index (1-9).
    JumpToAgent(usize),
    /// Select a specific agent by ID.
    FocusAgent(Uuid),

    // ─── Mode Switching ────────────────────────────
    /// Enter Insert Mode for the currently selected agent.
    EnterInsertMode,
    /// Return to Normal Mode.
    ExitInsertMode,
    /// Open the command palette.
    OpenCommandPalette,
    /// Close the command palette.
    CloseCommandPalette,

    // ─── Agent Lifecycle ───────────────────────────
    /// Spawn a new agent (triggers spawn dialog or command).
    SpawnAgent,
    /// Kill the currently selected agent (with confirmation).
    KillAgent,
    /// Restart the currently selected agent.
    RestartAgent,
    /// Spawn an agent from a named template.
    SpawnFromTemplate {
        template_name: String,
        agent_name: String,
        project_name: String,
    },

    // ─── PTY Interaction ───────────────────────────
    /// Send raw bytes to the focused agent's PTY.
    SendToPty(Vec<u8>),

    // ─── Layout (v0.2) ────────────────────────────
    /// Split the main pane horizontally.
    SplitHorizontal,
    /// Split the main pane vertically.
    SplitVertical,
    /// Close the currently focused split pane.
    CloseSplit,
    /// Cycle focus to the next pane.
    CyclePaneFocus,

    // ─── Search (v0.2) ────────────────────────────
    /// Enter search mode.
    EnterSearchMode,
    /// Search result navigation.
    SearchNext,
    SearchPrev,

    // ─── Scrollback (v0.2) ────────────────────────
    /// Scroll up half a page.
    ScrollUp,
    /// Scroll down half a page.
    ScrollDown,

    // ─── Application ──────────────────────────────
    /// Show/hide the help overlay.
    ToggleHelp,
    /// Quit Maestro (with confirmation if agents are running).
    Quit,
    /// Reload configuration from disk.
    ReloadConfig,

    // ─── No-op ────────────────────────────────────
    /// Input was consumed but no action is needed.
    None,
}
```

### Input Modes (`src/input/mode.rs`)

```rust
/// The current input mode determines how keystrokes are interpreted.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    /// Normal mode — vim-style navigation.
    /// Keystrokes map to Actions via the keybinding table.
    Normal,

    /// Insert mode — all keystrokes go to the agent's PTY.
    /// Only Esc and Ctrl+\ are intercepted.
    Insert {
        /// The name of the agent receiving input (shown in status bar).
        agent_name: String,
    },

    /// Command palette is open.
    /// Keystrokes go to the palette's text input.
    Command {
        /// Current input buffer.
        input: String,
        /// Index of the selected suggestion.
        selected: usize,
    },

    /// Search mode (v0.2) — typing a search query.
    Search {
        /// Current search query.
        query: String,
        /// Number of matches found.
        match_count: usize,
        /// Current match index.
        current_match: usize,
    },
}

impl InputMode {
    /// Returns the display string for the status bar.
    pub fn status_text(&self) -> String {
        match self {
            InputMode::Normal => "-- NORMAL --".to_string(),
            InputMode::Insert { agent_name } => format!("-- INSERT ({}) --", agent_name),
            InputMode::Command { .. } => "-- COMMAND --".to_string(),
            InputMode::Search { query, current_match, match_count } => {
                format!("/{} ({}/{})", query, current_match, match_count)
            }
        }
    }

    /// Whether this mode forwards most keys to a PTY or text input.
    pub fn is_text_input(&self) -> bool {
        matches!(self, InputMode::Insert { .. } | InputMode::Command { .. } | InputMode::Search { .. })
    }
}

impl Default for InputMode {
    fn default() -> Self {
        InputMode::Normal
    }
}
```

### Event Bus (`src/event/bus.rs`)

The event bus is the central multiplexer. It uses `tokio::select!` to merge multiple async sources into a single `AppEvent` stream.

```rust
use crate::event::types::{AppEvent, InputEvent};
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

/// Central event bus that multiplexes all event sources.
pub struct EventBus {
    /// Receiver for the merged event stream.
    rx: mpsc::UnboundedReceiver<AppEvent>,
    /// Sender cloned to each event source.
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self { tx, rx }
    }

    /// Get a sender handle for PTY output producers to send events.
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }

    /// Start all background event source tasks.
    /// This spawns tokio tasks for:
    /// - Crossterm input events
    /// - State detection tick
    /// - Render scheduling
    pub fn start(&self, fps: u32, state_check_interval_ms: u64) {
        self.start_input_reader();
        self.start_state_tick(state_check_interval_ms);
        self.start_render_scheduler(fps);
    }

    /// Receive the next event. Blocks until one is available.
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    /// Start the crossterm event reader task.
    fn start_input_reader(&self) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            loop {
                match reader.next().await {
                    Some(Ok(CrosstermEvent::Key(key))) => {
                        // Only handle key press events (not release/repeat)
                        if key.kind == KeyEventKind::Press {
                            let _ = tx.send(AppEvent::Input(InputEvent::Key(key)));
                        }
                    }
                    Some(Ok(CrosstermEvent::Resize(cols, rows))) => {
                        let _ = tx.send(AppEvent::Resize { cols, rows });
                    }
                    Some(Ok(_)) => {
                        // Ignore other events (mouse, focus, paste) for now
                    }
                    Some(Err(e)) => {
                        tracing::error!("Crossterm event error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
        });
    }

    /// Start the state detection tick timer.
    fn start_state_tick(&self, interval_ms: u64) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
            loop {
                interval.tick().await;
                if tx.send(AppEvent::StateTick).is_err() {
                    break; // Channel closed, app shutting down
                }
            }
        });
    }

    /// Start the render scheduler.
    /// Uses dirty-flag approach: instead of rendering at fixed FPS,
    /// we send RenderRequest at most `fps` times per second,
    /// but only when something has changed.
    fn start_render_scheduler(&self, fps: u32) {
        let tx = self.tx.clone();
        let frame_duration = Duration::from_secs_f64(1.0 / fps as f64);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(frame_duration);
            loop {
                interval.tick().await;
                if tx.send(AppEvent::RenderRequest).is_err() {
                    break;
                }
            }
        });
    }
}
```

#### Dirty-Flag Rendering Enhancement

The render scheduler sends `RenderRequest` at the configured FPS rate, but the `App` (Feature 12) will only actually render if a dirty flag is set. This flag is set by:
- `PtyOutput` events (new terminal content)
- `AgentStateChanged` events (sidebar update needed)
- `Input` events (selection changed, mode changed)
- `Resize` events (layout recalculation needed)

The `RenderRequest` event is a "you may render now" signal, not a "you must render" command. This saves CPU when nothing is happening.

### Agent ID Type

For type safety, wrap `Uuid` in a newtype:

```rust
// In src/agent/mod.rs or a shared types module
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentId(Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.to_string()[..8]) // Short ID for display
    }
}
```

Update `AppEvent` and `Action` to use `AgentId` instead of raw `Uuid`.

## Implementation Steps

1. **Implement `src/input/mode.rs`**
   - Define `InputMode` enum with all variants.
   - Implement `status_text()`, `is_text_input()`, `Default`.

2. **Implement `src/input/action.rs`**
   - Define `Action` enum with all variants.
   - Group variants logically with comments.
   - v0.2 actions are included but documented as such.

3. **Implement `AgentId` newtype**
   - Add to `src/agent/mod.rs` or create `src/agent/id.rs`.
   - Derive necessary traits: `Debug, Clone, Copy, PartialEq, Eq, Hash`.

4. **Implement `src/event/types.rs`**
   - Define `AppEvent` and `InputEvent` enums.
   - Use `AgentId` for agent identification.

5. **Implement `src/event/bus.rs`**
   - Create `EventBus` struct with `mpsc` channel.
   - Implement `start()` with background tasks for input, state tick, and render.
   - Implement `next()` for consuming events.

6. **Update `src/event/mod.rs`**
   - Re-export `EventBus`, `AppEvent`, `InputEvent`.

7. **Update `src/input/mod.rs`**
   - Re-export `Action`, `InputMode`.

8. **Wire into `main.rs` for smoke test**
   - Create an `EventBus`, start it, recv a few events, print them, exit.
   - This is a temporary verification step.

## Error Handling

| Scenario | Handling |
|---|---|
| `mpsc` channel closed unexpectedly | Background tasks detect `send()` failure and exit gracefully. The `next()` method returns `None`. |
| Crossterm event stream error | Log error, break from reader loop. App continues but loses keyboard input — should trigger a controlled shutdown. |
| Crossterm event stream ends | Reader task exits. No crash. |
| Timer task panic | Tokio logs the panic. Other tasks continue. The app may lose state-check or render ticks. |

### Resilience Design

The event bus should **never panic**. All `send()` calls use `let _ = tx.send(...)` to silently drop events if the receiver is gone (which means the app is shutting down anyway).

## Testing Strategy

### Unit Tests — `InputMode`

```rust
#[test]
fn test_normal_mode_status_text() {
    let mode = InputMode::Normal;
    assert_eq!(mode.status_text(), "-- NORMAL --");
}

#[test]
fn test_insert_mode_status_text() {
    let mode = InputMode::Insert { agent_name: "test-agent".into() };
    assert_eq!(mode.status_text(), "-- INSERT (test-agent) --");
}

#[test]
fn test_normal_mode_is_not_text_input() {
    assert!(!InputMode::Normal.is_text_input());
}

#[test]
fn test_insert_mode_is_text_input() {
    let mode = InputMode::Insert { agent_name: "x".into() };
    assert!(mode.is_text_input());
}

#[test]
fn test_default_mode_is_normal() {
    assert_eq!(InputMode::default(), InputMode::Normal);
}
```

### Unit Tests — `AgentId`

```rust
#[test]
fn test_agent_id_uniqueness() {
    let a = AgentId::new();
    let b = AgentId::new();
    assert_ne!(a, b);
}

#[test]
fn test_agent_id_display_is_short() {
    let id = AgentId::new();
    let display = format!("{}", id);
    assert_eq!(display.len(), 8);
}
```

### Integration Tests — `EventBus`

```rust
#[tokio::test]
async fn test_event_bus_receives_state_tick() {
    let mut bus = EventBus::new();
    bus.start(30, 50); // 50ms tick for fast test

    let event = tokio::time::timeout(
        Duration::from_millis(200),
        bus.next(),
    ).await;

    assert!(event.is_ok());
    // Should receive either a StateTick or RenderRequest
}

#[tokio::test]
async fn test_event_bus_sender_can_inject_events() {
    let mut bus = EventBus::new();
    let tx = bus.sender();

    tx.send(AppEvent::QuitRequested).unwrap();

    let event = bus.next().await.unwrap();
    assert!(matches!(event, AppEvent::QuitRequested));
}

#[tokio::test]
async fn test_event_bus_handles_pty_output() {
    let mut bus = EventBus::new();
    let tx = bus.sender();

    let id = AgentId::new();
    tx.send(AppEvent::PtyOutput {
        agent_id: id,
        data: b"hello".to_vec(),
    }).unwrap();

    let event = bus.next().await.unwrap();
    match event {
        AppEvent::PtyOutput { agent_id, data } => {
            assert_eq!(agent_id, id);
            assert_eq!(data, b"hello");
        }
        _ => panic!("Expected PtyOutput event"),
    }
}
```

## Acceptance Criteria

- [ ] `InputMode` enum has 4 variants (Normal, Insert, Command, Search) with correct methods.
- [ ] `Action` enum covers all v0.1 and v0.2 actions (v0.2 ones are defined but unused).
- [ ] `AppEvent` enum covers all event types: Input, PtyOutput, PtyEof, AgentStateChanged, StateTick, RenderRequest, Resize, QuitRequested.
- [ ] `AgentId` newtype wraps `Uuid` with short display format.
- [ ] `EventBus` multiplexes crossterm input, state tick, and render scheduler.
- [ ] `EventBus::sender()` allows PTY controllers to inject `PtyOutput` events.
- [ ] Dirty-flag rendering concept is implemented (RenderRequest is a suggestion, not a command).
- [ ] All `send()` calls handle channel closure gracefully (no panics).
- [ ] All unit and integration tests pass.
- [ ] Event types are `Debug` for logging.
