//! Sidebar widget — displays project/agent tree.
//!
//! Shows hierarchical list of projects and their agents with
//! status indicators, selection highlighting, and collapse support.
//! Implements `ratatui::widgets::StatefulWidget` for integration with
//! the main render loop.

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, StatefulWidget, Widget};

use crate::agent::state::AgentState;
use crate::agent::AgentId;
use crate::ui::theme::Theme;

/// Agent info tuple used when rebuilding the sidebar: `(id, name, state, uptime)`.
pub type AgentInfo = (AgentId, String, AgentState, String);

/// Project data for sidebar rebuild: `(project_name, agents)`.
pub type ProjectAgents = (String, Vec<AgentInfo>);

// ─── Data Model ────────────────────────────────────────────────

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

/// Sidebar UI state (separate from agent data).
///
/// Maintains the flat item list, selection index, scroll offset,
/// and set of collapsed projects. Call [`SidebarState::rebuild`]
/// whenever agent data changes.
pub struct SidebarState {
    /// Cached flat list of sidebar items.
    items: Vec<SidebarItem>,
    /// Index of the currently selected item.
    selected_index: usize,
    /// Scroll offset for rendering.
    scroll_offset: usize,
    /// Set of collapsed project names.
    collapsed_projects: HashSet<String>,
}

impl SidebarState {
    /// Create a new empty sidebar state.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            collapsed_projects: HashSet::new(),
        }
    }

    /// Rebuild the flat item list from grouped agent data.
    ///
    /// `projects` is an ordered list of `(project_name, agents)` where each
    /// agent is `(id, name, state, uptime)`. This avoids coupling to
    /// `AgentManager` directly, making the sidebar testable in isolation.
    pub fn rebuild(
        &mut self,
        projects: &[ProjectAgents],
    ) {
        self.items.clear();

        for (project_name, agents) in projects {
            let is_collapsed = self.collapsed_projects.contains(project_name);

            self.items.push(SidebarItem::ProjectHeader {
                name: project_name.clone(),
                agent_count: agents.len(),
                is_collapsed,
            });

            if !is_collapsed {
                for (id, name, state, uptime) in agents {
                    self.items.push(SidebarItem::Agent {
                        id: *id,
                        name: name.clone(),
                        project_name: project_name.clone(),
                        state: state.clone(),
                        uptime: uptime.clone(),
                    });
                }
            }
        }

        // Clamp selection to valid range.
        if self.items.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.items.len() - 1);
        }
    }

    /// Move selection down by one.
    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
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
        if self.selected_index == 0 {
            return;
        }
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

    /// Toggle collapse state of the project under the cursor.
    ///
    /// If an agent is selected, toggles its parent project.
    pub fn toggle_collapse(&mut self) {
        let project_name = match self.items.get(self.selected_index) {
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

    /// Get the project name of the currently selected item.
    ///
    /// If a project header is selected, returns its name.
    /// If an agent is selected, returns the agent's parent project name.
    /// Returns `None` if the sidebar is empty.
    pub fn selected_project_name(&self) -> Option<&str> {
        match self.items.get(self.selected_index) {
            Some(SidebarItem::ProjectHeader { name, .. }) => Some(name),
            Some(SidebarItem::Agent { project_name, .. }) => Some(project_name),
            None => None,
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

    /// Get the current scroll offset.
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Set the selected index directly (for mouse click handling).
    ///
    /// Clamps to the valid item range.
    pub fn set_selected(&mut self, index: usize) {
        if !self.items.is_empty() {
            self.selected_index = index.min(self.items.len() - 1);
        }
    }

    /// Adjust scroll offset so the selected item remains visible.
    pub fn scroll_into_view(&mut self, visible_height: usize) {
        self.scroll_offset =
            calculate_scroll_offset(self.selected_index, self.items.len(), visible_height);
    }
}

impl Default for SidebarState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Widget ────────────────────────────────────────────────────

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
        // Draw the sidebar border and title.
        let block = Block::default()
            .title(" PROJECTS ")
            .borders(Borders::RIGHT)
            .border_style(self.theme.sidebar_border);

        let inner = block.inner(area);
        block.render(area, buf);

        if state.items().is_empty() {
            // Empty state — render centered message.
            let msg = "No agents";
            if inner.width as usize >= msg.len() && inner.height > 0 {
                let x = inner.x + (inner.width.saturating_sub(msg.len() as u16)) / 2;
                let y = inner.y + inner.height / 2;
                let style = Style::default().fg(ratatui::style::Color::DarkGray);
                buf.set_string(x, y, msg, style);
            }
            return;
        }

        let visible_height = inner.height as usize;
        state.scroll_into_view(visible_height);
        let scroll_offset = state.scroll_offset;

        for (i, item) in state
            .items()
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height)
        {
            let y = inner.y + (i - scroll_offset) as u16;
            let is_selected = i == state.selected_index();

            let row = RowArea {
                x: inner.x,
                y,
                width: inner.width,
                is_selected,
            };

            match item {
                SidebarItem::ProjectHeader {
                    name,
                    agent_count,
                    is_collapsed,
                } => {
                    self.render_project_header(buf, &row, name, *agent_count, *is_collapsed);
                }
                SidebarItem::Agent {
                    name,
                    state: agent_state,
                    uptime,
                    ..
                } => {
                    self.render_agent_row(buf, &row, name, agent_state, uptime);
                }
            }
        }
    }
}

/// Positional parameters for rendering a single sidebar row.
struct RowArea {
    x: u16,
    y: u16,
    width: u16,
    is_selected: bool,
}

impl<'a> Sidebar<'a> {
    /// Fill the full row width with the selection background if selected.
    fn fill_row_bg(&self, buf: &mut Buffer, row: &RowArea) {
        if !row.is_selected {
            return;
        }
        let bg = Style::default().bg(self.theme.sidebar_selected_bg);
        for col in row.x..row.x + row.width {
            if let Some(cell) = buf.cell_mut((col, row.y)) {
                cell.set_style(bg);
            }
        }
    }

    fn render_project_header(
        &self,
        buf: &mut Buffer,
        row: &RowArea,
        name: &str,
        agent_count: usize,
        is_collapsed: bool,
    ) {
        let chevron = if is_collapsed { "▸" } else { "▼" };
        let text = format!("{} {} ({})", chevron, name, agent_count);
        let truncated = truncate_name(&text, row.width.saturating_sub(2) as usize);

        let style = if row.is_selected {
            self.theme
                .sidebar_project_header
                .bg(self.theme.sidebar_selected_bg)
        } else {
            self.theme.sidebar_project_header
        };

        self.fill_row_bg(buf, row);
        buf.set_string(row.x + 1, row.y, &truncated, style);
    }

    fn render_agent_row(
        &self,
        buf: &mut Buffer,
        row: &RowArea,
        name: &str,
        agent_state: &AgentState,
        uptime: &str,
    ) {
        let status_symbol = agent_state.symbol();
        let status_style = self.theme.status_style(agent_state.color_key());

        let name_style = if row.is_selected {
            self.theme
                .sidebar_agent_name_selected
                .bg(self.theme.sidebar_selected_bg)
        } else {
            self.theme.sidebar_agent_name
        };

        self.fill_row_bg(buf, row);

        // Layout: "  ● agent-name"
        //  x+1: indent (2 spaces)
        //  x+3: status symbol (1 char + 1 space)
        //  x+5: agent name
        let indent = "  ";
        buf.set_string(row.x + 1, row.y, indent, name_style);
        buf.set_string(row.x + 3, row.y, status_symbol, status_style);

        // Calculate available width for agent name.
        let prefix_used: u16 = 5; // 1 padding + 2 indent + 1 symbol + 1 space
        let uptime_reserve: u16 = if self.show_uptime && !uptime.is_empty() {
            uptime.len() as u16 + 2 // space + uptime + trailing pad
        } else {
            0
        };
        let available = row
            .width
            .saturating_sub(prefix_used)
            .saturating_sub(uptime_reserve) as usize;
        let truncated_name = truncate_name(name, available);
        buf.set_string(row.x + 5, row.y, &truncated_name, name_style);

        // Uptime right-aligned if enabled.
        if self.show_uptime && !uptime.is_empty() {
            let uptime_width = uptime.len() as u16;
            if uptime_width + 1 < row.width {
                let uptime_x = row.x + row.width - uptime_width - 1;
                buf.set_string(uptime_x, row.y, uptime, self.theme.sidebar_uptime);
            }
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────

/// Truncate a name to fit within `max_width`, adding an ellipsis if needed.
pub fn truncate_name(name: &str, max_width: usize) -> String {
    if name.len() <= max_width {
        name.to_string()
    } else if max_width > 1 {
        format!("{}…", &name[..max_width - 1])
    } else if max_width == 1 {
        "…".to_string()
    } else {
        String::new()
    }
}

/// Calculate the scroll offset to keep the selected item visible.
///
/// Maintains a padding of 2 items above/below the selection when possible.
pub fn calculate_scroll_offset(
    selected: usize,
    total_items: usize,
    visible_height: usize,
) -> usize {
    if visible_height == 0 || total_items <= visible_height {
        return 0;
    }

    let max_offset = total_items.saturating_sub(visible_height);
    let padding = 2usize;

    if selected < padding {
        0
    } else if selected >= total_items.saturating_sub(padding) {
        max_offset
    } else if selected + padding + 1 > visible_height {
        (selected + padding + 1 - visible_height).min(max_offset)
    } else {
        0
    }
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// Helper to build test project data.
    fn test_projects() -> Vec<(String, Vec<(AgentId, String, AgentState, String)>)> {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        let id3 = AgentId::new();
        vec![
            (
                "myapp".to_string(),
                vec![
                    (
                        id1,
                        "backend".to_string(),
                        AgentState::Running {
                            since: Utc::now(),
                        },
                        "5m".to_string(),
                    ),
                    (
                        id2,
                        "frontend".to_string(),
                        AgentState::Idle {
                            since: Utc::now(),
                        },
                        "2m".to_string(),
                    ),
                ],
            ),
            (
                "webui".to_string(),
                vec![(
                    id3,
                    "test-runner".to_string(),
                    AgentState::Completed {
                        at: Utc::now(),
                        exit_code: Some(0),
                    },
                    "8m".to_string(),
                )],
            ),
        ]
    }

    fn build_state_with_projects() -> SidebarState {
        let mut state = SidebarState::new();
        state.rebuild(&test_projects());
        state
    }

    // ── SidebarState basics ───────────────────

    #[test]
    fn new_state_is_empty() {
        let state = SidebarState::new();
        assert!(state.items().is_empty());
        assert_eq!(state.selected_index(), 0);
        assert!(state.selected_agent_id().is_none());
    }

    #[test]
    fn rebuild_populates_items() {
        let state = build_state_with_projects();
        // 2 project headers + 2 agents + 1 agent = 5
        assert_eq!(state.items().len(), 5);

        // First item is a project header.
        assert!(matches!(
            state.items()[0],
            SidebarItem::ProjectHeader {
                ref name,
                agent_count: 2,
                ..
            } if name == "myapp"
        ));

        // Second item is an agent.
        assert!(matches!(
            state.items()[1],
            SidebarItem::Agent { ref name, .. } if name == "backend"
        ));
    }

    #[test]
    fn rebuild_clamps_selection() {
        let mut state = SidebarState::new();
        state.selected_index = 100; // way out of bounds
        state.rebuild(&test_projects());
        assert_eq!(state.selected_index(), 4); // clamped to last index
    }

    #[test]
    fn rebuild_empty_data() {
        let mut state = SidebarState::new();
        state.selected_index = 5;
        state.rebuild(&[]);
        assert!(state.items().is_empty());
        assert_eq!(state.selected_index(), 0);
    }

    // ── Navigation ────────────────────────────

    #[test]
    fn select_next() {
        let mut state = build_state_with_projects();
        assert_eq!(state.selected_index(), 0);

        state.select_next();
        assert_eq!(state.selected_index(), 1);

        state.select_next();
        assert_eq!(state.selected_index(), 2);
    }

    #[test]
    fn select_next_stops_at_end() {
        let mut state = build_state_with_projects();
        for _ in 0..20 {
            state.select_next();
        }
        assert_eq!(state.selected_index(), state.items().len() - 1);
    }

    #[test]
    fn select_prev() {
        let mut state = build_state_with_projects();
        state.selected_index = 3;
        state.select_prev();
        assert_eq!(state.selected_index(), 2);
    }

    #[test]
    fn select_prev_stops_at_zero() {
        let mut state = SidebarState::new();
        state.select_prev();
        assert_eq!(state.selected_index(), 0);

        let mut state = build_state_with_projects();
        state.select_prev();
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn next_project_jumps_to_header() {
        let mut state = build_state_with_projects();
        // Start at index 0 (myapp header), jump to next header.
        state.next_project();
        assert_eq!(state.selected_index(), 3); // webui header
    }

    #[test]
    fn next_project_no_op_at_last_header() {
        let mut state = build_state_with_projects();
        state.selected_index = 3; // webui header
        state.next_project();
        assert_eq!(state.selected_index(), 3); // stays
    }

    #[test]
    fn prev_project_jumps_back() {
        let mut state = build_state_with_projects();
        state.selected_index = 4; // test-runner agent
        state.prev_project();
        assert_eq!(state.selected_index(), 3); // webui header
    }

    #[test]
    fn prev_project_no_op_at_first() {
        let mut state = build_state_with_projects();
        state.prev_project();
        assert_eq!(state.selected_index(), 0);
    }

    // ── Jump to agent ─────────────────────────

    #[test]
    fn jump_to_agent_1_indexed() {
        let mut state = build_state_with_projects();
        state.jump_to_agent(1);
        assert_eq!(state.selected_index(), 1); // first agent

        state.jump_to_agent(2);
        assert_eq!(state.selected_index(), 2); // second agent

        state.jump_to_agent(3);
        assert_eq!(state.selected_index(), 4); // third agent (in second project)
    }

    #[test]
    fn jump_to_agent_out_of_range() {
        let mut state = build_state_with_projects();
        state.jump_to_agent(99);
        assert_eq!(state.selected_index(), 0); // unchanged
    }

    // ── Collapse / Expand ─────────────────────

    #[test]
    fn toggle_collapse_on_project_header() {
        let mut state = build_state_with_projects();
        // Select myapp header (index 0).
        assert_eq!(state.items().len(), 5);

        state.toggle_collapse();
        // Now myapp is in collapsed set.
        assert!(state.collapsed_projects.contains("myapp"));

        // Rebuild to see the effect.
        state.rebuild(&test_projects());
        // myapp collapsed: header + webui header + 1 agent = 3
        assert_eq!(state.items().len(), 3);

        // Expand again.
        state.toggle_collapse(); // selected is still on myapp header (index 0)
        state.rebuild(&test_projects());
        assert_eq!(state.items().len(), 5);
    }

    #[test]
    fn toggle_collapse_on_agent_collapses_parent() {
        let mut state = build_state_with_projects();
        state.selected_index = 1; // backend agent under myapp

        state.toggle_collapse();
        assert!(state.collapsed_projects.contains("myapp"));
    }

    #[test]
    fn toggle_collapse_empty_items() {
        let mut state = SidebarState::new();
        state.toggle_collapse(); // should not panic
    }

    // ── selected_agent_id ─────────────────────

    #[test]
    fn selected_agent_id_on_agent() {
        let state = build_state_with_projects();
        // Index 0 is a header → None.
        assert!(state.selected_agent_id().is_none());
    }

    #[test]
    fn selected_agent_id_returns_id() {
        let mut state = build_state_with_projects();
        state.selected_index = 1; // backend agent
        assert!(state.selected_agent_id().is_some());
    }

    // ── select_agent ──────────────────────────

    #[test]
    fn select_agent_by_id() {
        let projects = test_projects();
        let target_id = projects[1].1[0].0; // test-runner agent

        let mut state = SidebarState::new();
        state.rebuild(&projects);

        state.select_agent(target_id);
        assert_eq!(state.selected_index(), 4);
        assert_eq!(state.selected_agent_id(), Some(target_id));
    }

    #[test]
    fn select_agent_not_found() {
        let mut state = build_state_with_projects();
        let unknown_id = AgentId::new();
        let before = state.selected_index();
        state.select_agent(unknown_id);
        assert_eq!(state.selected_index(), before); // unchanged
    }

    // ── Scroll offset ─────────────────────────

    #[test]
    fn scroll_offset_fits_all() {
        assert_eq!(calculate_scroll_offset(0, 5, 10), 0);
        assert_eq!(calculate_scroll_offset(4, 5, 10), 0);
    }

    #[test]
    fn scroll_offset_top() {
        assert_eq!(calculate_scroll_offset(0, 20, 10), 0);
        assert_eq!(calculate_scroll_offset(1, 20, 10), 0);
    }

    #[test]
    fn scroll_offset_middle() {
        // selected=9, total=20, visible=10
        // 9 + 2 + 1 = 12 > 10 → offset = 12 - 10 = 2
        assert_eq!(calculate_scroll_offset(9, 20, 10), 2);
    }

    #[test]
    fn scroll_offset_bottom() {
        // selected=19 (last), total=20, visible=10
        // 19 >= 20 - 2 → max_offset = 20 - 10 = 10
        assert_eq!(calculate_scroll_offset(19, 20, 10), 10);
    }

    #[test]
    fn scroll_offset_zero_height() {
        assert_eq!(calculate_scroll_offset(5, 20, 0), 0);
    }

    // ── Name truncation ───────────────────────

    #[test]
    fn truncate_short_name() {
        assert_eq!(truncate_name("short", 10), "short");
    }

    #[test]
    fn truncate_exact_fit() {
        assert_eq!(truncate_name("exactly10!", 10), "exactly10!");
    }

    #[test]
    fn truncate_long_name() {
        assert_eq!(
            truncate_name("this-is-a-very-long-name", 10),
            "this-is-a…"
        );
    }

    #[test]
    fn truncate_width_one() {
        assert_eq!(truncate_name("hello", 1), "…");
    }

    #[test]
    fn truncate_width_zero() {
        assert_eq!(truncate_name("hello", 0), "");
    }

    #[test]
    fn truncate_width_two() {
        assert_eq!(truncate_name("hello", 2), "h…");
    }

    // ── Widget rendering ──────────────────────

    #[test]
    fn render_empty_state() {
        let theme = Theme::default_dark();
        let sidebar = Sidebar::new(&theme, true);
        let mut state = SidebarState::new();

        let area = Rect::new(0, 0, 28, 10);
        let mut buf = Buffer::empty(area);
        sidebar.render(area, &mut buf, &mut state);

        // "No agents" should appear somewhere in the buffer.
        let content = buffer_to_string(&buf);
        assert!(content.contains("No agents"));
    }

    #[test]
    fn render_with_agents() {
        let theme = Theme::default_dark();
        let sidebar = Sidebar::new(&theme, true);
        let mut state = build_state_with_projects();

        let area = Rect::new(0, 0, 28, 10);
        let mut buf = Buffer::empty(area);
        sidebar.render(area, &mut buf, &mut state);

        let content = buffer_to_string(&buf);
        // Should contain project header marker and project name.
        assert!(content.contains("myapp"));
        assert!(content.contains("backend"));
        assert!(content.contains("webui"));
    }

    #[test]
    fn render_collapsed_hides_agents() {
        let theme = Theme::default_dark();
        let sidebar = Sidebar::new(&theme, false);
        let mut state = build_state_with_projects();

        // Collapse myapp.
        state.toggle_collapse();
        state.rebuild(&test_projects());

        let area = Rect::new(0, 0, 28, 10);
        let mut buf = Buffer::empty(area);
        sidebar.render(area, &mut buf, &mut state);

        let content = buffer_to_string(&buf);
        assert!(content.contains("myapp"));
        // backend and frontend should NOT be visible.
        assert!(!content.contains("backend"));
        assert!(!content.contains("frontend"));
        // But test-runner under webui should be visible.
        assert!(content.contains("test-runner"));
    }

    #[test]
    fn render_uptime_hidden_when_disabled() {
        let theme = Theme::default_dark();
        let sidebar = Sidebar::new(&theme, false); // show_uptime = false
        let mut state = build_state_with_projects();
        state.selected_index = 1; // select backend agent

        let area = Rect::new(0, 0, 28, 10);
        let mut buf = Buffer::empty(area);
        sidebar.render(area, &mut buf, &mut state);

        let content = buffer_to_string(&buf);
        // "5m" uptime should not appear when disabled.
        assert!(!content.contains("5m"));
    }

    #[test]
    fn render_scrolling() {
        let theme = Theme::default_dark();
        let sidebar = Sidebar::new(&theme, false);

        // Build many items so they exceed visible height.
        let mut projects = Vec::new();
        let mut agents = Vec::new();
        for i in 0..15 {
            agents.push((
                AgentId::new(),
                format!("agent-{}", i),
                AgentState::Running {
                    since: Utc::now(),
                },
                String::new(),
            ));
        }
        projects.push(("bigproject".to_string(), agents));

        let mut state = SidebarState::new();
        state.rebuild(&projects);
        // Select the last item to force scrolling.
        state.selected_index = 15; // last agent

        let area = Rect::new(0, 0, 28, 8);
        let mut buf = Buffer::empty(area);
        sidebar.render(area, &mut buf, &mut state);

        let content = buffer_to_string(&buf);
        // The last agent should be visible.
        assert!(content.contains("agent-14"));
        // The first agents should be scrolled out.
        assert!(!content.contains("agent-0"));
    }

    #[test]
    fn default_trait() {
        let state = SidebarState::default();
        assert!(state.items().is_empty());
        assert_eq!(state.selected_index(), 0);
    }

    // ── Test helper ───────────────────────────

    /// Convert a ratatui buffer to a multi-line string for assertions.
    fn buffer_to_string(buf: &Buffer) -> String {
        let area = buf.area;
        let mut lines = Vec::new();
        for y in area.y..area.y + area.height {
            let mut line = String::new();
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    line.push_str(cell.symbol());
                }
            }
            lines.push(line);
        }
        lines.join("\n")
    }
}
