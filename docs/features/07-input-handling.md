# Feature 07: Input Handling & Modal System

## Overview

Implement the mode-aware input handler that translates keyboard events into application actions. Maestro uses a vim-inspired modal system: **Normal Mode** for navigation and commands, **Insert Mode** for interacting with the agent's PTY. The input handler is the gatekeeper that ensures keystrokes go to the right place.

## Dependencies

- **Feature 03** (Core Types & Event System) — `Action` enum, `InputMode` enum, `InputEvent`.

## Technical Specification

### Input Flow

```
Crossterm KeyEvent
       │
       ▼
  InputHandler
  ┌────────────────────────────────┐
  │  match current_mode:           │
  │                                │
  │  Normal →  key_to_action()     │──→ Action (navigate, spawn, quit, etc.)
  │  Insert →  forward_to_pty()    │──→ Action::SendToPty(bytes)
  │  Command → palette_input()     │──→ Action (palette navigation or Action::None)
  │  Search →  search_input()      │──→ Action (v0.2)
  └────────────────────────────────┘
```

### InputHandler (`src/input/handler.rs`)

```rust
use crate::input::action::Action;
use crate::input::mode::InputMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Mode-aware input handler that converts key events to actions.
pub struct InputHandler {
    /// Current input mode.
    mode: InputMode,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
        }
    }

    /// Get the current input mode (for display in status bar).
    pub fn mode(&self) -> &InputMode {
        &self.mode
    }

    /// Set the input mode directly (used by App when processing actions).
    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
    }

    /// Process a key event and return the corresponding action.
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match &self.mode {
            InputMode::Normal => self.handle_normal_mode(key),
            InputMode::Insert { .. } => self.handle_insert_mode(key),
            InputMode::Command { .. } => self.handle_command_mode(key),
            InputMode::Search { .. } => self.handle_search_mode(key),
        }
    }

    // ─── Normal Mode ──────────────────────────────────

    fn handle_normal_mode(&self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // ── Navigation ──
            (KeyModifiers::NONE, KeyCode::Char('j')) => Action::SelectNext,
            (KeyModifiers::NONE, KeyCode::Char('k')) => Action::SelectPrev,
            (KeyModifiers::NONE, KeyCode::Down)      => Action::SelectNext,
            (KeyModifiers::NONE, KeyCode::Up)        => Action::SelectPrev,

            (KeyModifiers::SHIFT, KeyCode::Char('J')) => Action::NextProject,
            (KeyModifiers::SHIFT, KeyCode::Char('K')) => Action::PrevProject,

            // ── Jump to agent by number ──
            (KeyModifiers::NONE, KeyCode::Char(c)) if ('1'..='9').contains(&c) => {
                Action::JumpToAgent((c as usize) - ('0' as usize))
            }

            // ── Mode switching ──
            (KeyModifiers::NONE, KeyCode::Enter)     => Action::EnterInsertMode,
            (KeyModifiers::NONE, KeyCode::Char('i')) => Action::EnterInsertMode,
            (KeyModifiers::NONE, KeyCode::Char(':')) => Action::OpenCommandPalette,
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => Action::OpenCommandPalette,

            // ── Agent lifecycle ──
            (KeyModifiers::NONE, KeyCode::Char('n')) => Action::SpawnAgent,
            (KeyModifiers::NONE, KeyCode::Char('d')) => Action::KillAgent,
            (KeyModifiers::NONE, KeyCode::Char('r')) => Action::RestartAgent,

            // ── Layout (v0.2) ──
            (KeyModifiers::NONE, KeyCode::Char('s')) => Action::SplitHorizontal,
            (KeyModifiers::NONE, KeyCode::Char('v')) => Action::SplitVertical,
            (KeyModifiers::NONE, KeyCode::Tab)       => Action::CyclePaneFocus,
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => Action::CloseSplit,

            // ── Search/Scroll (v0.2) ──
            (KeyModifiers::NONE, KeyCode::Char('/')) => Action::EnterSearchMode,
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => Action::ScrollUp,
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => Action::ScrollDown,

            // ── Application ──
            (KeyModifiers::NONE, KeyCode::Char('?')) => Action::ToggleHelp,
            (KeyModifiers::NONE, KeyCode::Char('q')) => Action::Quit,

            // ── Unbound key ──
            _ => Action::None,
        }
    }

    // ─── Insert Mode ──────────────────────────────────

    fn handle_insert_mode(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Esc always returns to Normal Mode
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::ExitInsertMode
            }

            // Ctrl+\ as escape hatch (in case Esc doesn't work for some reason)
            (KeyModifiers::CONTROL, KeyCode::Char('\\')) => {
                self.mode = InputMode::Normal;
                Action::ExitInsertMode
            }

            // All other keys are forwarded to the PTY as raw bytes
            _ => {
                let bytes = key_event_to_bytes(key);
                if bytes.is_empty() {
                    Action::None
                } else {
                    Action::SendToPty(bytes)
                }
            }
        }
    }

    // ─── Command Mode ─────────────────────────────────

    fn handle_command_mode(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Esc closes command palette
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::CloseCommandPalette
            }

            // Enter executes the selected command
            (KeyModifiers::NONE, KeyCode::Enter) => {
                // The command palette will be implemented in Feature 14
                // For now, just close and return the command string
                if let InputMode::Command { ref input, .. } = self.mode {
                    let _command = input.clone();
                    self.mode = InputMode::Normal;
                    // TODO: parse command and return appropriate Action
                    Action::CloseCommandPalette
                } else {
                    Action::None
                }
            }

            // Navigation within suggestions
            (KeyModifiers::NONE, KeyCode::Down) |
            (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                if let InputMode::Command { ref mut selected, .. } = self.mode {
                    *selected = selected.saturating_add(1);
                }
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Up) |
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                if let InputMode::Command { ref mut selected, .. } = self.mode {
                    *selected = selected.saturating_sub(1);
                }
                Action::None
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if let InputMode::Command { ref mut input, ref mut selected, .. } = self.mode {
                    input.pop();
                    *selected = 0;
                }
                Action::None
            }

            // Character input
            (KeyModifiers::NONE, KeyCode::Char(c)) |
            (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                if let InputMode::Command { ref mut input, ref mut selected, .. } = self.mode {
                    input.push(c);
                    *selected = 0;
                }
                Action::None
            }

            _ => Action::None,
        }
    }

    // ─── Search Mode (v0.2) ───────────────────────────

    fn handle_search_mode(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                // Confirm search, switch to Normal mode but keep results
                self.mode = InputMode::Normal;
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Char('n')) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Action::SearchNext
            }
            _ => {
                // TODO: implement in v0.2
                Action::None
            }
        }
    }
}

/// Convert a crossterm KeyEvent to the raw bytes that should be sent to a PTY.
///
/// This handles the translation from structured key events back to the byte
/// sequences that terminal applications expect.
fn key_event_to_bytes(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        // ── Printable characters ──
        KeyCode::Char(c) if ctrl => {
            // Ctrl+A..Z → 0x01..0x1A
            if c.is_ascii_lowercase() || c.is_ascii_uppercase() {
                let byte = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                if alt {
                    vec![0x1b, byte]
                } else {
                    vec![byte]
                }
            } else {
                vec![]
            }
        }
        KeyCode::Char(c) => {
            let mut bytes = Vec::new();
            if alt {
                bytes.push(0x1b); // ESC prefix for Alt
            }
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            bytes.extend_from_slice(encoded.as_bytes());
            bytes
        }

        // ── Special keys ──
        KeyCode::Enter     => vec![b'\r'],
        KeyCode::Tab       => vec![b'\t'],
        KeyCode::Backspace => vec![0x7f],  // DEL
        KeyCode::Esc       => vec![0x1b],
        KeyCode::Delete    => vec![0x1b, b'[', b'3', b'~'],

        // ── Arrow keys ──
        KeyCode::Up    => vec![0x1b, b'[', b'A'],
        KeyCode::Down  => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left  => vec![0x1b, b'[', b'D'],

        // ── Page navigation ──
        KeyCode::Home    => vec![0x1b, b'[', b'H'],
        KeyCode::End     => vec![0x1b, b'[', b'F'],
        KeyCode::PageUp  => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],

        // ── Function keys (F1-F12) ──
        KeyCode::F(1)  => vec![0x1b, b'O', b'P'],
        KeyCode::F(2)  => vec![0x1b, b'O', b'Q'],
        KeyCode::F(3)  => vec![0x1b, b'O', b'R'],
        KeyCode::F(4)  => vec![0x1b, b'O', b'S'],
        KeyCode::F(5)  => vec![0x1b, b'[', b'1', b'5', b'~'],
        KeyCode::F(6)  => vec![0x1b, b'[', b'1', b'7', b'~'],
        KeyCode::F(7)  => vec![0x1b, b'[', b'1', b'8', b'~'],
        KeyCode::F(8)  => vec![0x1b, b'[', b'1', b'9', b'~'],
        KeyCode::F(9)  => vec![0x1b, b'[', b'2', b'0', b'~'],
        KeyCode::F(10) => vec![0x1b, b'[', b'2', b'1', b'~'],
        KeyCode::F(11) => vec![0x1b, b'[', b'2', b'3', b'~'],
        KeyCode::F(12) => vec![0x1b, b'[', b'2', b'4', b'~'],
        KeyCode::F(_)  => vec![],

        // ── Insert key ──
        KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],

        // ── Null / unrecognized ──
        KeyCode::Null => vec![0x00],

        // ── Catch-all ──
        _ => vec![],
    }
}
```

### Key Design Decisions

#### 1. Mode transitions happen in the InputHandler

When `Esc` is pressed in Insert Mode, the handler both changes its internal mode AND returns `Action::ExitInsertMode`. This ensures the mode state is always consistent — the App doesn't have to remember to call `set_mode()` separately.

However, for `EnterInsertMode`, the App must call `set_mode(InputMode::Insert { agent_name })` because only the App knows the agent name. The handler returns the action, and the App applies the mode change with the correct agent name.

#### 2. `key_event_to_bytes` is comprehensive

This function translates crossterm's structured key events back to raw terminal byte sequences. This is necessary because the PTY expects raw bytes, not structured events. The mapping follows the xterm protocol, which is the de facto standard for terminal emulators.

Key considerations:
- **Ctrl+C** (`0x03`), **Ctrl+D** (`0x04`), **Ctrl+Z** (`0x1a`) are all forwarded. This means the user can send interrupt, EOF, and suspend signals to the agent.
- **Alt+key** is sent as `ESC + key` (the Meta key convention).
- **UTF-8 characters** are properly encoded.

#### 3. `Esc` vs `Ctrl+\` for escaping Insert Mode

`Esc` is the primary escape. But `Esc` is also a prefix for many terminal escape sequences (arrow keys, etc.). Crossterm handles this with a timeout, but in rare cases it might not register correctly. `Ctrl+\` (`SIGQUIT` in normal terminals) serves as an infallible escape hatch.

#### 4. Number keys (1-9) are agent jump shortcuts

In Normal Mode, pressing `1` through `9` jumps to the Nth agent in the flat display order. This allows fast switching without scrolling. `0` is intentionally not bound (ambiguous: first agent or "ten"?).

### Mode Transition Diagram

```
                 Enter / i                      Esc / Ctrl+\
Normal Mode  ──────────────→  Insert Mode  ──────────────────→  Normal Mode
     │                                                              ▲
     │  : / Ctrl+P                                    Esc           │
     └──────────────→  Command Mode  ───────────────────────────────┘
     │                                                              ▲
     │  /                                             Esc / Enter   │
     └──────────────→  Search Mode  ────────────────────────────────┘
```

## Implementation Steps

1. **Implement `key_event_to_bytes()`**
   - This is the most important function. Get it right before anything else.
   - Test with every key type: printable, control, alt, function keys, arrows, special.

2. **Implement `InputHandler::handle_normal_mode()`**
   - Map all Normal Mode keybindings to Actions.
   - Include v0.2 bindings (they return actions that the App will ignore until v0.2).

3. **Implement `InputHandler::handle_insert_mode()`**
   - Esc → ExitInsertMode + mode change.
   - Ctrl+\ → ExitInsertMode + mode change.
   - Everything else → SendToPty(key_event_to_bytes()).

4. **Implement `InputHandler::handle_command_mode()`**
   - Basic text input + navigation.
   - Esc to close.
   - Enter to execute (stub for v0.1).

5. **Implement stub `handle_search_mode()`**
   - Esc to close.
   - Other keys as no-ops for v0.1.

6. **Update `src/input/mod.rs`**
   - Re-export `InputHandler`.

## Error Handling

The input handler should **never panic or return errors**. All inputs produce either a valid `Action` or `Action::None`. Unknown key combinations are silently ignored.

| Scenario | Handling |
|---|---|
| Unknown key code | Return `Action::None`. |
| Key event with no byte mapping | Return `Action::None` (empty bytes → no PTY write). |
| Mode is in unexpected state | Defensive match — all branches covered. |

## Testing Strategy

### Unit Tests — `key_event_to_bytes()`

```rust
#[test]
fn test_printable_char() {
    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![b'a']);
}

#[test]
fn test_uppercase_char() {
    let key = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
    assert_eq!(key_event_to_bytes(key), vec![b'A']);
}

#[test]
fn test_ctrl_c() {
    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(key_event_to_bytes(key), vec![0x03]);
}

#[test]
fn test_ctrl_d() {
    let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
    assert_eq!(key_event_to_bytes(key), vec![0x04]);
}

#[test]
fn test_enter() {
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![b'\r']);
}

#[test]
fn test_backspace() {
    let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![0x7f]);
}

#[test]
fn test_arrow_up() {
    let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'A']);
}

#[test]
fn test_arrow_down() {
    let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'B']);
}

#[test]
fn test_alt_x() {
    let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
    assert_eq!(key_event_to_bytes(key), vec![0x1b, b'x']);
}

#[test]
fn test_tab() {
    let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![b'\t']);
}

#[test]
fn test_utf8_char() {
    let key = KeyEvent::new(KeyCode::Char('ö'), KeyModifiers::NONE);
    let bytes = key_event_to_bytes(key);
    assert_eq!(std::str::from_utf8(&bytes).unwrap(), "ö");
}

#[test]
fn test_f1() {
    let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![0x1b, b'O', b'P']);
}

#[test]
fn test_delete() {
    let key = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'3', b'~']);
}
```

### Unit Tests — Normal Mode

```rust
#[test]
fn test_j_selects_next() {
    let handler = InputHandler::new();
    let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
    assert_eq!(handler.handle_key(key), Action::SelectNext);
}

#[test]
fn test_k_selects_prev() {
    let handler = InputHandler::new();
    let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
    assert_eq!(handler.handle_key(key), Action::SelectPrev);
}

#[test]
fn test_enter_enters_insert_mode() {
    let handler = InputHandler::new();
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(handler.handle_key(key), Action::EnterInsertMode);
}

#[test]
fn test_number_jump() {
    let handler = InputHandler::new();
    let key = KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE);
    assert_eq!(handler.handle_key(key), Action::JumpToAgent(3));
}

#[test]
fn test_q_quits() {
    let handler = InputHandler::new();
    let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(handler.handle_key(key), Action::Quit);
}

#[test]
fn test_unknown_key_is_none() {
    let handler = InputHandler::new();
    let key = KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE);
    assert_eq!(handler.handle_key(key), Action::None);
}
```

### Unit Tests — Insert Mode

```rust
#[test]
fn test_insert_mode_forwards_keys() {
    let mut handler = InputHandler::new();
    handler.set_mode(InputMode::Insert { agent_name: "test".into() });

    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let action = handler.handle_key(key);
    assert_eq!(action, Action::SendToPty(vec![b'a']));
}

#[test]
fn test_insert_mode_esc_returns_to_normal() {
    let mut handler = InputHandler::new();
    handler.set_mode(InputMode::Insert { agent_name: "test".into() });

    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let action = handler.handle_key(key);
    assert_eq!(action, Action::ExitInsertMode);
    assert_eq!(*handler.mode(), InputMode::Normal);
}

#[test]
fn test_insert_mode_ctrl_backslash_escapes() {
    let mut handler = InputHandler::new();
    handler.set_mode(InputMode::Insert { agent_name: "test".into() });

    let key = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL);
    let action = handler.handle_key(key);
    assert_eq!(action, Action::ExitInsertMode);
    assert_eq!(*handler.mode(), InputMode::Normal);
}

#[test]
fn test_insert_mode_ctrl_c_forwarded() {
    let mut handler = InputHandler::new();
    handler.set_mode(InputMode::Insert { agent_name: "test".into() });

    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = handler.handle_key(key);
    assert_eq!(action, Action::SendToPty(vec![0x03]));
    // Mode should NOT change — Ctrl+C is forwarded, not intercepted
    assert!(matches!(handler.mode(), InputMode::Insert { .. }));
}
```

### Unit Tests — Command Mode

```rust
#[test]
fn test_command_mode_text_input() {
    let mut handler = InputHandler::new();
    handler.set_mode(InputMode::Command { input: String::new(), selected: 0 });

    handler.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    if let InputMode::Command { ref input, .. } = handler.mode() {
        assert_eq!(input, "s");
    } else {
        panic!("Expected Command mode");
    }
}

#[test]
fn test_command_mode_esc_closes() {
    let mut handler = InputHandler::new();
    handler.set_mode(InputMode::Command { input: "test".into(), selected: 0 });

    let action = handler.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(action, Action::CloseCommandPalette);
    assert_eq!(*handler.mode(), InputMode::Normal);
}
```

## Acceptance Criteria

- [ ] Normal Mode maps all specified keybindings to correct Actions.
- [ ] Insert Mode forwards all keys (except Esc and Ctrl+\) as raw bytes to PTY.
- [ ] `Esc` always returns to Normal Mode from any mode.
- [ ] `Ctrl+\` serves as an escape hatch from Insert Mode.
- [ ] `key_event_to_bytes()` correctly translates: printable, control, alt, arrows, function keys, delete, backspace, enter, tab.
- [ ] UTF-8 characters are properly encoded.
- [ ] Command Mode supports basic text input, backspace, arrow navigation.
- [ ] Number keys 1-9 jump to agents in Normal Mode.
- [ ] Unknown/unbound keys return `Action::None` (no crash).
- [ ] All unit tests pass (at least 20 test cases).
