# Feature 09: UI Sidebar Widget

## Overview

Implement the sidebar widget that renders the project tree with agent status indicators. The sidebar is the primary navigation element — it shows all projects, all agents grouped by project, and each agent's current state with a colored indicator. The selected agent is highlighted. Projects can be collapsed/expanded.

## Dependencies

- **Feature 05** (Agent State Machine) — `AgentState` for status indicators.
- **Feature 06** (Agent Lifecycle Management) — `AgentManager::agents_by_project()` for data.
- **Feature 08** (Theme & Layout) — `Theme` for styling, `AppLayout::sidebar` for area.

## Technical Specification

### Sidebar Data Model

The sidebar renders data from the `AgentManager`, but it has its own state for selection and collapse:

```rust
/// Sidebar UI state (separate from agent data).
pub struct SidebarState {
    /// Index of the currently selected item in the flat list.
    /// This counts across all projects and agents.
    selected_index: usize,

    /// Set of collapsed project names.
    collapsed_projects: std::collections::HashSet<String>,

    /// Cached flat list of sidebar items (rebuilt when data changes).
    items: Vec<SidebarItem>,
}

/// An item in the flat sidebar list.
#[derive(Debug, Clone)]
pub enum SidebarItem {
    /// A project header row.
    ProjectHeader {
        name: String,
        agent_count: usize,
        is_collapsed: bool,
    },
    /// An agent row under a project.
    Agent {
        id: AgentId,
        name: String,
        project_name: String,
        state: AgentState,
        uptime: String,
    },
}

impl SidebarState {
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            collapsed_projects: std::collections::HashSet::new(),
            items: Vec::new(),
        }
    }

    /// Rebuild the flat item list from the agent manager's data.
    /// Call this whenever agents are added/removed/state-changed.
    pub fn rebuild(
        &mut self,
        agents_by_project: &[(String, Vec<AgentId>)],
        agent_manager: &AgentManager,
    ) {
        self.items.clear();

        for (project_name, agent_ids) in agents_by_project {
            let is_collapsed = self.collapsed_projects.contains(project_name);

            self.items.push(SidebarItem::ProjectHeader {
                name: project_name.clone(),
                agent_count: agent_ids.len(),
                is_collapsed,
            });

            if !is_collapsed {
                for &id in agent_ids {
                    if let Some(handle) = agent_manager.get(id) {
                        self.items.push(SidebarItem::Agent {
                            id,
                            name: handle.name().to_string(),
                            project_name: project_name.clone(),
                            state: handle.state().clone(),
                            uptime: handle.uptime(),
                        });
                    }
                }
            }
        }

        // Clamp selection to valid range
        if !self.items.is_empty() {
            self.selected_index = self.selected_index.min(self.items.len() - 1);
        }
    }

    /// Move selection down by one.
    pub fn select_next(&mut self) {
        if self.items.is_empty() { return; }
        self.selected_index = (self.selected_index + 1).min(self.items.len() - 1);
    }

    /// Move selection up by one.
    pub fn select_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    /// Jump to the next project header.
    pub fn next_project(&mut self) {
        for i in (self.selected_index + 1)..self.items.len() {
            if matches!(self.items[i], SidebarItem::ProjectHeader { .. }) {
                self.selected_index = i;
                return;
            }
        }
    }

    /// Jump to the previous project header.
    pub fn prev_project(&mut self) {
        if self.selected_index == 0 { return; }
        for i in (0..self.selected_index).rev() {
            if matches!(self.items[i], SidebarItem::ProjectHeader { .. }) {
                self.selected_index = i;
                return;
            }
        }
    }

    /// Jump to the Nth agent (1-indexed, across all projects).
    pub fn jump_to_agent(&mut self, n: usize) {
        let mut agent_count = 0;
        for (i, item) in self.items.iter().enumerate() {
            if matches!(item, SidebarItem::Agent { .. }) {
                agent_count += 1;
                if agent_count == n {
                    self.selected_index = i;
                    return;
                }
            }
        }
    }

    /// Toggle collapse state of the project that the cursor is on
    /// (or the project containing the selected agent).
    pub fn toggle_collapse(&mut self) {
        let project_name = match &self.items.get(self.selected_index) {
            Some(SidebarItem::ProjectHeader { name, .. }) => name.clone(),
            Some(SidebarItem::Agent { project_name, .. }) => project_name.clone(),
            None => return,
        };

        if self.collapsed_projects.contains(&project_name) {
            self.collapsed_projects.remove(&project_name);
        } else {
            self.collapsed_projects.insert(project_name);
        }
    }

    /// Get the currently selected agent ID (if an agent is selected).
    pub fn selected_agent_id(&self) -> Option<AgentId> {
        match self.items.get(self.selected_index) {
            Some(SidebarItem::Agent { id, .. }) => Some(*id),
            _ => None,
        }
    }

    /// Get the selected index.
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Get all items for rendering.
    pub fn items(&self) -> &[SidebarItem] {
        &self.items
    }

    /// Select a specific agent by ID.
    pub fn select_agent(&mut self, agent_id: AgentId) {
        for (i, item) in self.items.iter().enumerate() {
            if let SidebarItem::Agent { id, .. } = item {
                if *id == agent_id {
                    self.selected_index = i;
                    return;
                }
            }
        }
    }
}
```

### Sidebar Widget (`src/ui/sidebar.rs`)

The rendering implementation as a Ratatui widget.

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, StatefulWidget};
use crate::ui::theme::Theme;

/// Renders the sidebar widget.
pub struct Sidebar<'a> {
    theme: &'a Theme,
    show_uptime: bool,
}

impl<'a> Sidebar<'a> {
    pub fn new(theme: &'a Theme, show_uptime: bool) -> Self {
        Self { theme, show_uptime }
    }
}

impl<'a> StatefulWidget for Sidebar<'a> {
    type State = SidebarState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut SidebarState) {
        // Draw background
        let block = Block::default()
            .title(" PROJECTS ")
            .borders(Borders::RIGHT)
            .border_style(self.theme.sidebar_border);

        let inner = block.inner(area);
        block.render(area, buf);

        // Render each item
        let visible_height = inner.height as usize;
        let scroll_offset = calculate_scroll_offset(
            state.selected_index(),
            state.items().len(),
            visible_height,
        );

        for (i, item) in state.items().iter().enumerate().skip(scroll_offset).take(visible_height) {
            let y = inner.y + (i - scroll_offset) as u16;
            let is_selected = i == state.selected_index();

            match item {
                SidebarItem::ProjectHeader { name, agent_count, is_collapsed } => {
                    self.render_project_header(
                        buf, inner.x, y, inner.width,
                        name, *agent_count, *is_collapsed, is_selected,
                    );
                }
                SidebarItem::Agent { name, state: agent_state, uptime, .. } => {
                    self.render_agent_row(
                        buf, inner.x, y, inner.width,
                        name, agent_state, uptime, is_selected,
                    );
                }
            }
        }
    }
}

impl<'a> Sidebar<'a> {
    fn render_project_header(
        &self,
        buf: &mut Buffer,
        x: u16, y: u16, width: u16,
        name: &str,
        agent_count: usize,
        is_collapsed: bool,
        is_selected: bool,
    ) {
        let chevron = if is_collapsed { "▸" } else { "▼" };
        let text = format!("{} {} ({})", chevron, name, agent_count);

        let style = if is_selected {
            self.theme.sidebar_project_header
                .bg(self.theme.sidebar_selected_bg)
        } else {
            self.theme.sidebar_project_header
        };

        // Fill the full width with background
        let bg_style = if is_selected {
            ratatui::style::Style::default().bg(self.theme.sidebar_selected_bg)
        } else {
            ratatui::style::Style::default()
        };
        for col in x..x + width {
            buf.get_mut(col, y).set_style(bg_style);
        }

        buf.set_string(x + 1, y, &text, style);
    }

    fn render_agent_row(
        &self,
        buf: &mut Buffer,
        x: u16, y: u16, width: u16,
        name: &str,
        agent_state: &AgentState,
        uptime: &str,
        is_selected: bool,
    ) {
        let status_symbol = agent_state.symbol();
        let status_style = self.theme.status_style(agent_state.color_key());

        let name_style = if is_selected {
            self.theme.sidebar_agent_name_selected
                .bg(self.theme.sidebar_selected_bg)
        } else {
            self.theme.sidebar_agent_name
        };

        // Fill the full width with background
        let bg_style = if is_selected {
            ratatui::style::Style::default().bg(self.theme.sidebar_selected_bg)
        } else {
            ratatui::style::Style::default()
        };
        for col in x..x + width {
            buf.get_mut(col, y).set_style(bg_style);
        }

        // "  ● agent-name"
        let indent = "  ";
        buf.set_string(x + 1, y, indent, name_style);
        buf.set_string(x + 3, y, status_symbol, status_style);
        buf.set_string(x + 5, y, name, name_style);

        // Uptime right-aligned if enabled
        if self.show_uptime && !uptime.is_empty() {
            let uptime_width = uptime.len() as u16;
            if x + 5 + name.len() as u16 + 2 + uptime_width < x + width {
                let uptime_x = x + width - uptime_width - 1;
                buf.set_string(uptime_x, y, uptime, self.theme.sidebar_uptime);
            }
        }
    }
}

/// Calculate the scroll offset to keep the selected item visible.
fn calculate_scroll_offset(
    selected: usize,
    total_items: usize,
    visible_height: usize,
) -> usize {
    if total_items <= visible_height {
        return 0;
    }

    let padding = 2; // Keep 2 items of context above/below selection
    if selected < padding {
        0
    } else if selected >= total_items - padding {
        total_items.saturating_sub(visible_height)
    } else if selected >= visible_height - padding {
        selected + padding + 1 - visible_height
    } else {
        0
    }
}
```

### Visual Layout

Each sidebar row looks like:

```
▼ myapp (3)          ← Project header (collapsible)
  ● backend-refac  5m  ← Agent: status + name + uptime
  ? test-runner    2m  ← Agent waiting for input
  - docs-writer    8m  ← Agent idle
▸ webui (2)          ← Collapsed project
```

- Status indicators use the colored symbols from `AgentState::symbol()`.
- Agent names are truncated if they exceed available width.
- Uptime is right-aligned and only shown if `show_uptime = true`.
- Selected row has a highlighted background.

### Name Truncation

Agent names can be long. Truncate with ellipsis to fit:

```rust
fn truncate_name(name: &str, max_width: usize) -> String {
    if name.len() <= max_width {
        name.to_string()
    } else if max_width > 2 {
        format!("{}…", &name[..max_width - 1])
    } else {
        name[..max_width].to_string()
    }
}
```

The available width for agent names is: `sidebar_width - indent(3) - status(2) - uptime(~5) - padding(2)`.

## Implementation Steps

1. **Implement `SidebarState`**
   - `new()`, `rebuild()`, navigation methods.
   - `selected_agent_id()` for the App to know which agent is focused.

2. **Implement `Sidebar` widget**
   - `StatefulWidget` impl with `SidebarState`.
   - `render_project_header()` and `render_agent_row()` helpers.
   - Scroll offset calculation.
   - Name truncation.

3. **Implement `calculate_scroll_offset()`**
   - Keep selected item visible with padding.

4. **Update `src/ui/mod.rs`**
   - Re-export `Sidebar`, `SidebarState`, `SidebarItem`.

## Error Handling

| Scenario | Handling |
|---|---|
| Empty sidebar (no projects/agents) | Render "No agents" message centered in sidebar area. |
| Selection out of bounds | Clamped in `rebuild()`. |
| Agent name too long | Truncated with ellipsis. |
| Sidebar too narrow | Graceful degradation — hide uptime, then truncate names aggressively. |

## Testing Strategy

### Unit Tests — SidebarState

```rust
#[test]
fn test_select_next_wraps() {
    let mut state = SidebarState::new();
    // Setup with 3 items...
    state.select_next();
    assert_eq!(state.selected_index(), 1);
    state.select_next();
    assert_eq!(state.selected_index(), 2);
    state.select_next();
    assert_eq!(state.selected_index(), 2); // stays at last
}

#[test]
fn test_select_prev_stops_at_zero() {
    let mut state = SidebarState::new();
    state.select_prev();
    assert_eq!(state.selected_index(), 0);
}

#[test]
fn test_jump_to_agent() {
    // Setup state with project + 3 agents
    // jump_to_agent(2) should select the 2nd agent
}

#[test]
fn test_toggle_collapse() {
    // Setup state with a project
    // toggle_collapse → project is collapsed, agents hidden
    // toggle_collapse again → expanded
}

#[test]
fn test_scroll_offset() {
    assert_eq!(calculate_scroll_offset(0, 20, 10), 0);
    assert_eq!(calculate_scroll_offset(5, 20, 10), 0);
    assert_eq!(calculate_scroll_offset(9, 20, 10), 2);
    assert_eq!(calculate_scroll_offset(19, 20, 10), 10);
}

#[test]
fn test_name_truncation() {
    assert_eq!(truncate_name("short", 10), "short");
    assert_eq!(truncate_name("this-is-a-very-long-name", 10), "this-is-a…");
}
```

### Snapshot Tests

Use `insta` to capture rendered sidebar output as snapshots:

```rust
#[test]
fn test_sidebar_render_snapshot() {
    let mut state = /* setup with test data */;
    let theme = Theme::default_dark();
    let sidebar = Sidebar::new(&theme, true);

    let area = Rect::new(0, 0, 28, 20);
    let mut buf = Buffer::empty(area);
    sidebar.render(area, &mut buf, &mut state);

    insta::assert_snapshot!(buf_to_string(&buf));
}
```

## Acceptance Criteria

- [ ] Project headers show: chevron + name + agent count.
- [ ] Agent rows show: indent + status symbol (colored) + name + uptime.
- [ ] Selected item has highlighted background.
- [ ] `j`/`k` navigation moves selection up/down.
- [ ] `J`/`K` jumps between project headers.
- [ ] Number keys (1-9) jump to the Nth agent.
- [ ] Scrolling works when agents exceed visible height.
- [ ] Collapsed projects hide their agents.
- [ ] Agent names are truncated with ellipsis when too long.
- [ ] Uptime is right-aligned (and hidden if disabled in config).
- [ ] Empty state shows "No agents" message.
- [ ] All unit tests pass.
