# Feature 16: Mouse Support (v0.2)

## Overview

Add mouse interaction to Maestro: click sidebar items to select agents, click terminal panes to focus them, and scroll wheel for terminal scrollback. Mouse support is optional and complements the keyboard-first design — all mouse actions have keyboard equivalents.

## Dependencies

- **Feature 03** (Core Types & Event System) — extend `InputEvent` with mouse events.
- **Feature 09** (Sidebar) — click-to-select agents.
- **Feature 10** (Terminal Pane) — click-to-focus panes.
- **Feature 13** (Split/Grid Views) — multiple panes to click between.
- **Feature 15** (Scrollback) — mouse wheel scrolling.

## Technical Specification

### Enabling Mouse Capture

Crossterm supports mouse events. Enable at terminal setup:

```rust
use crossterm::event::{EnableMouseCapture, DisableMouseCapture};

// In main.rs, after entering alternate screen:
stdout().execute(EnableMouseCapture)?;

// On cleanup:
stdout().execute(DisableMouseCapture)?;
```

### Mouse Event Types

```rust
/// Extended input event with mouse support.
#[derive(Debug)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}
```

Crossterm's `MouseEvent` provides:
- `MouseEventKind::Down(button)` — click
- `MouseEventKind::ScrollUp` — scroll wheel up
- `MouseEventKind::ScrollDown` — scroll wheel down
- `MouseEventKind::Moved` — hover (ignored for v0.2)
- `column`, `row` — cursor position

### Mouse Handler

```rust
use crossterm::event::{MouseEvent, MouseEventKind, MouseButton};

impl InputHandler {
    pub fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        layout: &AppLayout,
    ) -> Action {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_left_click(mouse.column, mouse.row, layout)
            }
            MouseEventKind::ScrollUp => {
                // Only scroll if mouse is over a terminal pane
                if is_over_pane(mouse.column, mouse.row, layout) {
                    Action::ScrollUp
                } else {
                    Action::None
                }
            }
            MouseEventKind::ScrollDown => {
                if is_over_pane(mouse.column, mouse.row, layout) {
                    Action::ScrollDown
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        }
    }

    fn handle_left_click(&mut self, col: u16, row: u16, layout: &AppLayout) -> Action {
        // Check if click is in the sidebar
        if is_in_rect(col, row, &layout.sidebar) {
            let relative_row = row - layout.sidebar.y;
            return Action::SidebarClick { row: relative_row as usize };
        }

        // Check if click is in a terminal pane
        for (i, pane) in layout.panes.iter().enumerate() {
            if is_in_rect(col, row, &pane.area) {
                return Action::PaneFocusClick { pane_index: i };
            }
        }

        Action::None
    }
}

fn is_in_rect(col: u16, row: u16, rect: &Rect) -> bool {
    col >= rect.x && col < rect.x + rect.width
        && row >= rect.y && row < rect.y + rect.height
}

fn is_over_pane(col: u16, row: u16, layout: &AppLayout) -> bool {
    layout.panes.iter().any(|p| is_in_rect(col, row, &p.area))
}
```

### New Actions

Add to the `Action` enum:

```rust
/// User clicked a specific row in the sidebar.
SidebarClick { row: usize },
/// User clicked a terminal pane to focus it.
PaneFocusClick { pane_index: usize },
```

### Sidebar Click Handling

When the sidebar receives a click:
1. Calculate which item is at the clicked row (accounting for scroll offset).
2. If it's a project header → toggle collapse.
3. If it's an agent → select it (same as navigating with j/k and pressing Enter would).

```rust
// In App::dispatch_action():
Action::SidebarClick { row } => {
    let scroll_offset = self.sidebar_state.scroll_offset();
    let item_index = scroll_offset + row;
    self.sidebar_state.set_selected(item_index);

    // If clicking the already-selected agent, enter Insert Mode
    // (like double-clicking to interact)
    if let Some(id) = self.sidebar_state.selected_agent_id() {
        // Just select for now — Enter to interact
    }

    self.dirty = true;
}
```

### Pane Click Handling

When a terminal pane is clicked:
1. Set the focused pane to the clicked pane's index.
2. If already focused and in Normal Mode, optionally enter Insert Mode.

### Mouse in Insert Mode

In Insert Mode, mouse events should be **forwarded to the PTY** if the click is within the active pane. This allows Claude Code to receive mouse events if it supports them. However, for v0.2, we'll keep it simple:
- Mouse clicks on the sidebar always work (to switch agents).
- Mouse clicks on a different pane switch focus (exit Insert Mode).
- Mouse events within the focused pane are ignored (not forwarded to PTY).

### Configuration

```toml
[ui]
# Enable mouse support (default: true)
mouse_enabled = true
```

## Implementation Steps

1. **Enable mouse capture in `main.rs`**
   - Add `EnableMouseCapture` / `DisableMouseCapture`.

2. **Extend `InputEvent` in `event/types.rs`**
   - Add `Mouse(MouseEvent)` variant.

3. **Update `EventBus` input reader**
   - Handle `CrosstermEvent::Mouse(event)` → `InputEvent::Mouse(event)`.

4. **Add `SidebarClick` and `PaneFocusClick` to `Action`**

5. **Implement `InputHandler::handle_mouse()`**
   - Hit-testing against layout regions.
   - Click-to-select, click-to-focus, scroll wheel.

6. **Update `App::handle_event()` and `dispatch_action()`**
   - Process mouse-related actions.

7. **Add `mouse_enabled` to `UiConfig`**
   - Skip mouse capture if disabled.

## Error Handling

| Scenario | Handling |
|---|---|
| Mouse click outside any region | Return `Action::None`. |
| Mouse disabled in config | Don't call `EnableMouseCapture`. Mouse events won't arrive. |
| Click on empty sidebar row | No-op (no item at that position). |
| Scroll wheel outside pane | Ignored. |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_click_in_sidebar() {
    let layout = mock_layout(120, 40, 28);
    let handler = InputHandler::new();
    let action = handler.handle_mouse(
        mock_mouse_click(10, 5), // x=10, y=5 → inside sidebar (width=28)
        &layout,
    );
    assert!(matches!(action, Action::SidebarClick { .. }));
}

#[test]
fn test_click_in_pane() {
    let layout = mock_layout(120, 40, 28);
    let handler = InputHandler::new();
    let action = handler.handle_mouse(
        mock_mouse_click(60, 10), // x=60 → inside terminal pane
        &layout,
    );
    assert!(matches!(action, Action::PaneFocusClick { .. }));
}

#[test]
fn test_scroll_in_pane() {
    let layout = mock_layout(120, 40, 28);
    let handler = InputHandler::new();
    let action = handler.handle_mouse(
        mock_scroll_up(60, 10),
        &layout,
    );
    assert_eq!(action, Action::ScrollUp);
}

#[test]
fn test_click_outside_regions() {
    let layout = mock_layout(120, 40, 28);
    let handler = InputHandler::new();
    // Click on the very bottom row (status bar) — should be no-op
    let action = handler.handle_mouse(
        mock_mouse_click(60, 39),
        &layout,
    );
    assert_eq!(action, Action::None);
}
```

## Acceptance Criteria

- [ ] Left-click on a sidebar agent selects it.
- [ ] Left-click on a project header toggles collapse.
- [ ] Left-click on a terminal pane focuses it (in split/grid view).
- [ ] Scroll wheel over a terminal pane scrolls the scrollback.
- [ ] Mouse events outside any interactive region are ignored.
- [ ] Mouse can be disabled via `ui.mouse_enabled = false`.
- [ ] Mouse support is enabled with `EnableMouseCapture` at startup.
- [ ] Mouse capture is disabled on exit (`DisableMouseCapture`).
- [ ] All keyboard shortcuts still work alongside mouse.
