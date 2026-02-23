# Feature 10: UI Terminal Pane

## Overview

Implement the terminal pane widget that renders Claude Code's full TUI output inside a Ratatui frame. This is the core "terminal-in-terminal" capability that makes Maestro possible. It uses `vt100::Parser` to maintain a virtual screen buffer and `tui-term::PseudoTerminal` to render that buffer as a Ratatui widget.

## Dependencies

- **Feature 04** (PTY Management) — PTY output is processed by the vt100 parser.
- **Feature 06** (Agent Lifecycle Management) — `AgentHandle` provides the `vt100::Screen`.
- **Feature 08** (Theme & Layout) — `PaneLayout` for area calculation, `Theme` for border styling.

## Technical Specification

### Architecture

```
PTY output bytes
       │
       ▼
  vt100::Parser  ──→  vt100::Screen (virtual terminal grid)
                              │
                              ▼
                     tui_term::PseudoTerminal (Ratatui Widget)
                              │
                              ▼
                     ratatui::buffer::Buffer (drawn to real terminal)
```

### Terminal Pane Widget (`src/ui/terminal_pane.rs`)

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};
use ratatui::text::Span;
use tui_term::widget::PseudoTerminal;
use crate::agent::state::AgentState;
use crate::ui::theme::Theme;

/// Renders the terminal output of a single agent.
pub struct TerminalPane<'a> {
    /// The vt100 screen to render.
    screen: &'a vt100::Screen,
    /// Agent name (for the title bar).
    agent_name: &'a str,
    /// Project name (for the title bar).
    project_name: &'a str,
    /// Current agent state (for the status indicator in the title).
    agent_state: &'a AgentState,
    /// Whether this pane has focus (affects border style).
    is_focused: bool,
    /// Theme reference.
    theme: &'a Theme,
}

impl<'a> TerminalPane<'a> {
    pub fn new(
        screen: &'a vt100::Screen,
        agent_name: &'a str,
        project_name: &'a str,
        agent_state: &'a AgentState,
        is_focused: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            screen,
            agent_name,
            project_name,
            agent_state,
            is_focused,
            theme,
        }
    }
}

impl<'a> Widget for TerminalPane<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build the title: "Agent: backend-refactor @ myapp [R]"
        let status_indicator = match self.agent_state {
            AgentState::Running { .. }         => "[R]",
            AgentState::WaitingForInput { .. } => "[?]",
            AgentState::Idle { .. }            => "[-]",
            AgentState::Completed { .. }       => "[✓]",
            AgentState::Errored { .. }         => "[!]",
            AgentState::Spawning               => "[○]",
        };

        let title = format!(
            " Agent: {} @ {} {} ",
            self.agent_name,
            self.project_name,
            status_indicator,
        );

        // Choose border style based on focus
        let border_style = if self.is_focused {
            self.theme.terminal_title.clone()
        } else {
            self.theme.terminal_border.clone()
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Render the vt100 screen using tui-term
        if inner.width > 0 && inner.height > 0 {
            let pseudo_term = PseudoTerminal::new(self.screen);
            pseudo_term.render(inner, buf);
        }
    }
}

/// Renders an empty pane (when no agent is selected or available).
pub struct EmptyPane<'a> {
    theme: &'a Theme,
    message: &'a str,
}

impl<'a> EmptyPane<'a> {
    pub fn new(theme: &'a Theme, message: &'a str) -> Self {
        Self { theme, message }
    }
}

impl<'a> Widget for EmptyPane<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Maestro ")
            .borders(Borders::ALL)
            .border_style(self.theme.terminal_border);

        let inner = block.inner(area);
        block.render(area, buf);

        // Center the message
        if inner.width > 0 && inner.height > 0 {
            let msg = self.message;
            let x = inner.x + (inner.width.saturating_sub(msg.len() as u16)) / 2;
            let y = inner.y + inner.height / 2;
            buf.set_string(x, y, msg, self.theme.terminal_title);
        }
    }
}
```

### How `tui-term::PseudoTerminal` Works

The `tui-term` crate provides a `PseudoTerminal` widget that takes a `&vt100::Screen` reference and renders it cell-by-cell into a Ratatui buffer:

1. It iterates over each row and column of the `vt100::Screen`.
2. For each `vt100::Cell`, it reads the character and attributes (foreground color, background color, bold, italic, underline, etc.).
3. It maps these to Ratatui's `Cell` type and writes them into the buffer.

This means Claude Code's full TUI output — syntax highlighting, spinners, progress bars, tool approval prompts — renders correctly inside Maestro.

### Key Considerations

#### 1. Screen Size Mismatch

The `vt100::Parser` has a fixed size set at creation (or via `set_size()`). If the PTY and parser dimensions don't match the pane's inner area, the rendering will be off:
- **PTY too small**: Only part of the pane is filled; rest is empty.
- **PTY too large**: Output is clipped to the pane boundary.

**Solution**: Whenever the layout changes (resize, split toggle), the App must:
1. Recalculate `PaneLayout` for all visible panes.
2. Call `agent_handle.resize(new_pty_size)` for each visible agent.
3. The `resize()` method updates both the PTY dimensions and `vt100::Parser` size.

#### 2. Cursor Rendering

The `vt100::Screen` tracks cursor position. In Insert Mode, the cursor should be visible at the correct position in the active pane. `tui-term` handles this by setting the cursor position in the Ratatui frame.

For Normal Mode, the cursor should be hidden in the terminal pane (it's in the sidebar). Maestro controls this by:
- In Insert Mode: telling Ratatui to show cursor at the vt100 cursor position.
- In Normal Mode: hiding the terminal cursor entirely.

```rust
/// Get the cursor position for a given pane (if cursor should be shown).
pub fn cursor_position(
    screen: &vt100::Screen,
    pane_inner: &Rect,
) -> Option<(u16, u16)> {
    let cursor = screen.cursor_position();
    let x = pane_inner.x + cursor.1;
    let y = pane_inner.y + cursor.0;

    // Only return if within bounds
    if x < pane_inner.x + pane_inner.width && y < pane_inner.y + pane_inner.height {
        Some((x, y))
    } else {
        None
    }
}
```

#### 3. Performance — Only Render Visible Agents

With 15 agents, only 1-4 are visible at any time. The vt100 parser processes ALL agents' output (to track state), but tui-term only renders visible ones. The dirty flag on each `AgentHandle` further avoids re-rendering unchanged panes.

#### 4. Color Mapping

`vt100` uses its own color enum. `tui-term` handles the mapping to Ratatui's `Color` type. Both support:
- 8 basic colors + bright variants (16 total).
- 256-color palette.
- True color (RGB).

Claude Code typically uses true color for syntax highlighting, so this full mapping is essential.

### Resize Flow (Detailed)

```
Terminal resize event (or split toggle)
│
├─ App receives AppEvent::Resize { cols, rows }
│
├─ App calls calculate_layout(new_area, sidebar_width, layout_mode)
│   → Returns new AppLayout with updated PaneLayout for each visible pane
│
├─ For each visible pane:
│   ├─ Calculate new PTY size: pane_to_pty_size(pane.inner)
│   ├─ Get agent handle for this pane
│   ├─ Call handle.resize(new_pty_size)
│   │   ├─ pty_controller.resize(new_pty_size)  → SIGWINCH to child
│   │   └─ parser.set_size(rows, cols)           → vt100 reflows content
│   └─ Mark agent as dirty
│
└─ Next render will use the new layout
```

Important: Non-visible agents are NOT resized. They keep their last dimensions. When a non-visible agent becomes visible (user switches to it), it should be resized at that point.

## Implementation Steps

1. **Implement `src/ui/terminal_pane.rs`**
   - `TerminalPane` widget with `Widget` impl.
   - Title rendering with agent name, project, and status indicator.
   - Border styling based on focus state.
   - `tui_term::PseudoTerminal` rendering of the vt100 screen.
   - `EmptyPane` widget for empty states.
   - `cursor_position()` helper.

2. **Verify `tui-term` API**
   - Ensure `PseudoTerminal::new(&vt100::Screen)` compiles with the declared dependency versions.
   - Test that color rendering works (true color, 256-color).

3. **Update `src/ui/mod.rs`**
   - Re-export `TerminalPane`, `EmptyPane`.

## Error Handling

| Scenario | Handling |
|---|---|
| Zero-size inner area | Skip rendering (guard with `if width > 0 && height > 0`). |
| vt100 screen is empty (no output yet) | Renders blank terminal. The `EmptyPane` widget should be used instead until first output. |
| tui-term panics on malformed screen | Unlikely but caught by top-level panic hook. |
| Cursor out of bounds | `cursor_position()` returns `None`, cursor is hidden. |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_cursor_position_within_bounds() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(b"hello");
    let pane = Rect::new(30, 1, 80, 24);
    let pos = cursor_position(parser.screen(), &pane);
    assert!(pos.is_some());
    let (x, y) = pos.unwrap();
    assert_eq!(x, 30 + 5); // cursor after "hello"
    assert_eq!(y, 1);       // first row
}

#[test]
fn test_cursor_position_out_of_bounds() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    // Move cursor beyond the pane
    parser.process(b"\x1b[25;1H"); // row 25
    let pane = Rect::new(0, 0, 80, 24);
    let pos = cursor_position(parser.screen(), &pane);
    assert!(pos.is_none());
}
```

### Visual Integration Tests

These require running Claude Code in a PTY and rendering the output. They're best done manually but can be partially automated with snapshot tests:

```rust
#[test]
fn test_terminal_pane_render_with_ansi() {
    let mut parser = vt100::Parser::new(10, 40, 0);
    // Feed it some colored output
    parser.process(b"\x1b[32mgreen text\x1b[0m normal text");

    let state = AgentState::Running { since: Utc::now() };
    let theme = Theme::default_dark();
    let pane = TerminalPane::new(
        parser.screen(), "test", "project", &state, true, &theme,
    );

    let area = Rect::new(0, 0, 42, 12);
    let mut buf = Buffer::empty(area);
    pane.render(area, &mut buf);

    // Verify the green text is in the buffer with correct color
    let cell = buf.get(1, 1); // inner area, first char
    assert_eq!(cell.symbol(), "g");
    // Check color (implementation depends on how tui-term maps colors)
}

#[test]
fn test_empty_pane_centered_message() {
    let theme = Theme::default_dark();
    let pane = EmptyPane::new(&theme, "No agent selected");

    let area = Rect::new(0, 0, 60, 20);
    let mut buf = Buffer::empty(area);
    pane.render(area, &mut buf);

    // Verify message is somewhere in the buffer
    let content = buf_to_string(&buf);
    assert!(content.contains("No agent selected"));
}
```

### Manual Verification

1. Start Maestro with one agent running `claude`.
2. Verify that Claude Code's full output renders correctly:
   - Colored text (syntax highlighting).
   - Cursor positioning (input prompts).
   - Clearing and redrawing (when Claude refreshes its TUI).
   - Tool approval prompts display correctly.
3. Resize the terminal → verify output re-renders at new dimensions.
4. Switch between agents → verify each renders its own terminal content.
5. Split view → verify both panes render different agents.

## Acceptance Criteria

- [ ] `TerminalPane` renders `vt100::Screen` content inside a bordered Ratatui area.
- [ ] Title bar shows: agent name, project name, and status indicator.
- [ ] Focused pane has a highlighted border; unfocused pane has a dim border.
- [ ] ANSI colors (8-color, 256-color, true color) render correctly.
- [ ] Bold, italic, underline text attributes render correctly.
- [ ] Cursor position is correctly mapped from vt100 to screen coordinates.
- [ ] In Insert Mode, cursor is visible at the correct position.
- [ ] In Normal Mode, terminal pane cursor is hidden.
- [ ] `EmptyPane` renders a centered message when no agent is selected.
- [ ] Zero-size panes are handled without crash.
- [ ] Resize flow updates PTY dimensions, parser dimensions, and triggers re-render.
- [ ] Non-visible agents are not resized until they become visible.
- [ ] Claude Code's TUI (spinners, tool approvals, colored output) renders correctly.
