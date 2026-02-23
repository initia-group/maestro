# Feature 13: Split & Grid Views (v0.2)

## Overview

Extend the single-pane terminal view to support multiple simultaneous panes: horizontal split (2 panes stacked), vertical split (2 panes side-by-side), and a 2x2 grid (4 panes). This allows users to monitor multiple agents at once without switching. Panes have independent focus (one is active for Insert Mode interaction).

## Dependencies

- **Feature 08** (Theme & Layout) — `calculate_layout()` already supports Split/Grid modes. This feature activates and refines them.
- **Feature 10** (Terminal Pane) — renders each pane.
- **Feature 12** (App Bootstrap) — `App` handles layout switching and pane focus.

## Technical Specification

### Pane Management

```rust
/// Manages the state of split panes.
pub struct PaneManager {
    /// Current layout mode.
    layout: ActiveLayout,

    /// Which pane is focused (0-indexed).
    focused_pane: usize,

    /// Agent assignments for each pane.
    /// `pane_agents[i]` = the AgentId shown in pane i.
    /// None = empty pane.
    pane_agents: Vec<Option<AgentId>>,
}

impl PaneManager {
    pub fn new() -> Self {
        Self {
            layout: ActiveLayout::Single,
            focused_pane: 0,
            pane_agents: vec![None],
        }
    }

    /// Switch to a new layout mode.
    pub fn set_layout(&mut self, layout: ActiveLayout) {
        let pane_count = match layout {
            ActiveLayout::Single => 1,
            ActiveLayout::SplitHorizontal | ActiveLayout::SplitVertical => 2,
            ActiveLayout::Grid => 4,
        };

        // Preserve existing assignments, extend or truncate
        self.pane_agents.resize(pane_count, None);
        self.focused_pane = self.focused_pane.min(pane_count - 1);
        self.layout = layout;
    }

    /// Cycle focus to the next pane (Tab key).
    pub fn cycle_focus(&mut self) {
        let count = self.pane_agents.len();
        if count > 1 {
            self.focused_pane = (self.focused_pane + 1) % count;
        }
    }

    /// Close the focused pane (return to single if only 2, or to split if 4).
    pub fn close_focused_pane(&mut self) {
        match self.layout {
            ActiveLayout::Grid => {
                // Grid → remove focused pane, switch to appropriate layout
                self.pane_agents.remove(self.focused_pane);
                if self.pane_agents.len() == 2 {
                    self.layout = ActiveLayout::SplitHorizontal;
                }
                self.focused_pane = self.focused_pane.min(self.pane_agents.len() - 1);
            }
            ActiveLayout::SplitHorizontal | ActiveLayout::SplitVertical => {
                // Split → remove focused pane, switch to single
                self.pane_agents.remove(self.focused_pane);
                self.layout = ActiveLayout::Single;
                self.focused_pane = 0;
            }
            ActiveLayout::Single => {
                // Already single — no-op
            }
        }
    }

    /// Assign an agent to the focused pane.
    pub fn assign_to_focused(&mut self, agent_id: AgentId) {
        if let Some(slot) = self.pane_agents.get_mut(self.focused_pane) {
            *slot = Some(agent_id);
        }
    }

    /// Get the agent in a specific pane.
    pub fn agent_in_pane(&self, pane_index: usize) -> Option<AgentId> {
        self.pane_agents.get(pane_index).copied().flatten()
    }

    /// Get the focused pane index.
    pub fn focused_pane(&self) -> usize {
        self.focused_pane
    }

    /// Get the current layout.
    pub fn layout(&self) -> &ActiveLayout {
        &self.layout
    }

    /// Number of panes.
    pub fn pane_count(&self) -> usize {
        self.pane_agents.len()
    }
}
```

### Keybindings

| Key | Action | Context |
|-----|--------|---------|
| `s` | `Action::SplitHorizontal` | Normal Mode — split into 2 horizontal panes |
| `v` | `Action::SplitVertical` | Normal Mode — split into 2 vertical panes |
| `Tab` | `Action::CyclePaneFocus` | Normal Mode — move focus to next pane |
| `Ctrl+w` | `Action::CloseSplit` | Normal Mode — close focused pane |

### Agent-Pane Assignment Strategy

When the user splits the view:
1. **Pane 0** (top/left) shows the currently selected agent.
2. **Pane 1** (bottom/right) shows the next agent in display order.
3. For Grid mode, panes 2-3 show the following agents.
4. If there aren't enough agents, extra panes are empty.

When the user navigates the sidebar in split/grid mode:
- The **focused pane** updates to show the selected agent.
- Other panes keep their assignments.

### PTY Resize for Multiple Panes

When switching layouts, all visible agents must be resized:

```
Single (90 cols) → SplitV (45 cols each)
│
├─ Agent A: resize PTY to 45 cols
├─ Agent B: resize PTY to 45 cols
└─ Both vt100 parsers: set_size(rows, 45)
```

This triggers `SIGWINCH` in both agent processes. Claude Code will re-render at the narrower width.

### Focused Pane Visual Indicator

The focused pane has a brighter border (using `theme.terminal_title` style). Unfocused panes use `theme.terminal_border` (dim). This is already supported by `TerminalPane`'s `is_focused` parameter.

### Insert Mode in Split View

When entering Insert Mode in split view:
- Only the **focused pane** receives keystrokes.
- The status bar shows `-- INSERT (agent-name) --`.
- The cursor is visible only in the focused pane.
- `Tab` is NOT available in Insert Mode (Tab is forwarded to PTY). Use `Esc` first, then `Tab` to switch panes.

## Implementation Steps

1. **Implement `PaneManager`**
   - Add to `src/ui/pane_manager.rs` (new file) or extend `src/ui/layout.rs`.
   - Layout switching, focus cycling, pane closing.
   - Agent-pane assignment logic.

2. **Update `App`**
   - Replace `focused_pane: usize` with `PaneManager`.
   - Update `dispatch_action()` to handle Split/Close/CycleFocus actions.
   - Update `render()` to render multiple panes with correct focus.
   - Update `resize_visible_agents()` to resize all visible agents.

3. **Update `InputHandler`**
   - `s`, `v`, `Tab`, `Ctrl+w` are already mapped in Feature 07.
   - No changes needed.

4. **Update `calculate_layout()`**
   - Already supports all modes (Feature 08). Verify edge cases.

5. **Test with multiple agents**
   - Split with 2 agents, interact with each.
   - Grid with 4 agents.
   - Resize in split mode.
   - Close panes to return to single.

## Error Handling

| Scenario | Handling |
|---|---|
| Split with only 1 agent | Second pane shows EmptyPane. |
| Split with 0 agents | Both panes empty. |
| Close last pane | No-op (can't go below single). |
| Grid with narrow terminal | Panes get very small. Show truncated output. At extreme sizes, switch to error message. |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_split_creates_two_panes() {
    let mut pm = PaneManager::new();
    pm.set_layout(ActiveLayout::SplitHorizontal);
    assert_eq!(pm.pane_count(), 2);
}

#[test]
fn test_cycle_focus() {
    let mut pm = PaneManager::new();
    pm.set_layout(ActiveLayout::SplitVertical);
    assert_eq!(pm.focused_pane(), 0);
    pm.cycle_focus();
    assert_eq!(pm.focused_pane(), 1);
    pm.cycle_focus();
    assert_eq!(pm.focused_pane(), 0); // wraps
}

#[test]
fn test_close_pane_returns_to_single() {
    let mut pm = PaneManager::new();
    pm.set_layout(ActiveLayout::SplitHorizontal);
    pm.close_focused_pane();
    assert_eq!(pm.pane_count(), 1);
    assert!(matches!(pm.layout(), ActiveLayout::Single));
}

#[test]
fn test_grid_close_to_split() {
    let mut pm = PaneManager::new();
    pm.set_layout(ActiveLayout::Grid);
    assert_eq!(pm.pane_count(), 4);
    pm.close_focused_pane();
    // Should go to 3 panes, which maps to split
    assert_eq!(pm.pane_count(), 3);
}
```

### Integration Tests

- Start Maestro with 4 agents, switch to Grid, verify all render.
- Interact with each pane in Insert Mode.
- Resize terminal in Grid mode.

## Acceptance Criteria

- [ ] `s` key splits into 2 horizontal panes.
- [ ] `v` key splits into 2 vertical panes.
- [ ] `Tab` cycles focus between panes.
- [ ] `Ctrl+w` closes the focused pane.
- [ ] Focused pane has a highlighted border.
- [ ] Insert Mode only affects the focused pane.
- [ ] PTY resize happens when layout changes.
- [ ] Empty panes show a placeholder message.
- [ ] Sidebar navigation updates the focused pane's agent.
- [ ] Grid mode shows 4 panes simultaneously.
- [ ] Closing panes reduces layout (Grid → Split → Single).
