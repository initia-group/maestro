//! Pane management for split and grid views.
//!
//! `PaneManager` tracks the current layout mode, which pane is focused,
//! and which agent is assigned to each pane slot.

use crate::agent::AgentId;
use crate::ui::layout::ActiveLayout;

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
    /// Create a new PaneManager in single-pane mode.
    pub fn new() -> Self {
        Self {
            layout: ActiveLayout::Single,
            focused_pane: 0,
            pane_agents: vec![None],
        }
    }

    /// Create a new PaneManager with a specific initial layout.
    pub fn with_layout(layout: ActiveLayout) -> Self {
        let pane_count = pane_count_for_layout(&layout);
        Self {
            layout,
            focused_pane: 0,
            pane_agents: vec![None; pane_count],
        }
    }

    /// Switch to a new layout mode.
    ///
    /// Preserves existing agent assignments where possible. If the new layout
    /// has more panes, extra slots are `None`. If fewer, assignments are
    /// truncated and focus is clamped.
    pub fn set_layout(&mut self, layout: ActiveLayout) {
        let pane_count = pane_count_for_layout(&layout);

        // Preserve existing assignments, extend or truncate
        self.pane_agents.resize(pane_count, None);
        self.focused_pane = self.focused_pane.min(pane_count - 1);
        self.layout = layout;
    }

    /// Cycle focus to the next pane (wraps around).
    pub fn cycle_focus(&mut self) {
        let count = self.pane_agents.len();
        if count > 1 {
            self.focused_pane = (self.focused_pane + 1) % count;
        }
    }

    /// Close the focused pane, reducing the layout.
    ///
    /// - Grid (4 panes) -> removes focused pane, switches to SplitHorizontal
    ///   if 2 remain, or stays at 3 panes.
    /// - Split (2 panes) -> removes focused pane, switches to Single.
    /// - Single -> no-op (cannot go below 1 pane).
    pub fn close_focused_pane(&mut self) {
        match self.layout {
            ActiveLayout::Grid => {
                // Grid -> remove focused pane
                self.pane_agents.remove(self.focused_pane);
                if self.pane_agents.len() == 2 {
                    self.layout = ActiveLayout::SplitHorizontal;
                }
                self.focused_pane = self.focused_pane.min(self.pane_agents.len() - 1);
            }
            ActiveLayout::SplitHorizontal | ActiveLayout::SplitVertical => {
                // Split -> remove focused pane, switch to single
                self.pane_agents.remove(self.focused_pane);
                self.layout = ActiveLayout::Single;
                self.focused_pane = 0;
            }
            ActiveLayout::Single => {
                // Already single -- no-op
            }
        }
    }

    /// Assign an agent to the focused pane.
    pub fn assign_to_focused(&mut self, agent_id: AgentId) {
        if let Some(slot) = self.pane_agents.get_mut(self.focused_pane) {
            *slot = Some(agent_id);
        }
    }

    /// Assign an agent to a specific pane by index.
    pub fn assign_to_pane(&mut self, pane_index: usize, agent_id: Option<AgentId>) {
        if let Some(slot) = self.pane_agents.get_mut(pane_index) {
            *slot = agent_id;
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

    /// Set the focused pane index directly (for mouse click handling).
    ///
    /// Clamps to valid pane range.
    pub fn set_focused_pane(&mut self, index: usize) {
        if index < self.pane_agents.len() {
            self.focused_pane = index;
        }
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

impl Default for PaneManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the number of panes for a given layout mode.
fn pane_count_for_layout(layout: &ActiveLayout) -> usize {
    match layout {
        ActiveLayout::Single => 1,
        ActiveLayout::SplitHorizontal | ActiveLayout::SplitVertical => 2,
        ActiveLayout::Grid => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_single() {
        let pm = PaneManager::new();
        assert_eq!(pm.pane_count(), 1);
        assert_eq!(pm.focused_pane(), 0);
        assert_eq!(*pm.layout(), ActiveLayout::Single);
    }

    #[test]
    fn with_layout_grid() {
        let pm = PaneManager::with_layout(ActiveLayout::Grid);
        assert_eq!(pm.pane_count(), 4);
        assert_eq!(pm.focused_pane(), 0);
    }

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
    fn cycle_focus_single_is_noop() {
        let mut pm = PaneManager::new();
        assert_eq!(pm.focused_pane(), 0);
        pm.cycle_focus();
        assert_eq!(pm.focused_pane(), 0);
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

    #[test]
    fn grid_close_twice_reaches_split() {
        let mut pm = PaneManager::new();
        pm.set_layout(ActiveLayout::Grid);
        pm.close_focused_pane(); // 4 -> 3
        assert_eq!(pm.pane_count(), 3);
        pm.close_focused_pane(); // 3 -> 2 -> SplitHorizontal
        assert_eq!(pm.pane_count(), 2);
        assert!(matches!(pm.layout(), ActiveLayout::SplitHorizontal));
    }

    #[test]
    fn close_single_is_noop() {
        let mut pm = PaneManager::new();
        pm.close_focused_pane();
        assert_eq!(pm.pane_count(), 1);
        assert!(matches!(pm.layout(), ActiveLayout::Single));
    }

    #[test]
    fn assign_and_retrieve_agent() {
        let mut pm = PaneManager::new();
        pm.set_layout(ActiveLayout::SplitVertical);

        let id = AgentId::new();
        pm.assign_to_focused(id);
        assert_eq!(pm.agent_in_pane(0), Some(id));
        assert_eq!(pm.agent_in_pane(1), None);
    }

    #[test]
    fn assign_to_pane_by_index() {
        let mut pm = PaneManager::new();
        pm.set_layout(ActiveLayout::Grid);

        let id1 = AgentId::new();
        let id2 = AgentId::new();
        pm.assign_to_pane(0, Some(id1));
        pm.assign_to_pane(3, Some(id2));

        assert_eq!(pm.agent_in_pane(0), Some(id1));
        assert_eq!(pm.agent_in_pane(1), None);
        assert_eq!(pm.agent_in_pane(2), None);
        assert_eq!(pm.agent_in_pane(3), Some(id2));
    }

    #[test]
    fn set_layout_preserves_assignments() {
        let mut pm = PaneManager::new();
        let id = AgentId::new();
        pm.assign_to_focused(id);
        assert_eq!(pm.agent_in_pane(0), Some(id));

        // Expand to split - pane 0 keeps its agent
        pm.set_layout(ActiveLayout::SplitHorizontal);
        assert_eq!(pm.agent_in_pane(0), Some(id));
        assert_eq!(pm.agent_in_pane(1), None);
    }

    #[test]
    fn set_layout_clamps_focus() {
        let mut pm = PaneManager::new();
        pm.set_layout(ActiveLayout::Grid);
        // Move focus to pane 3
        pm.cycle_focus(); // 1
        pm.cycle_focus(); // 2
        pm.cycle_focus(); // 3
        assert_eq!(pm.focused_pane(), 3);

        // Shrink to single -> focus clamped to 0
        pm.set_layout(ActiveLayout::Single);
        assert_eq!(pm.focused_pane(), 0);
    }

    #[test]
    fn cycle_focus_grid_visits_all_four() {
        let mut pm = PaneManager::new();
        pm.set_layout(ActiveLayout::Grid);
        assert_eq!(pm.focused_pane(), 0);
        pm.cycle_focus();
        assert_eq!(pm.focused_pane(), 1);
        pm.cycle_focus();
        assert_eq!(pm.focused_pane(), 2);
        pm.cycle_focus();
        assert_eq!(pm.focused_pane(), 3);
        pm.cycle_focus();
        assert_eq!(pm.focused_pane(), 0); // wraps
    }

    #[test]
    fn agent_in_pane_out_of_bounds() {
        let pm = PaneManager::new();
        assert_eq!(pm.agent_in_pane(5), None);
    }

    #[test]
    fn default_impl() {
        let pm = PaneManager::default();
        assert_eq!(pm.pane_count(), 1);
        assert_eq!(*pm.layout(), ActiveLayout::Single);
    }

    #[test]
    fn close_split_vertical_returns_to_single() {
        let mut pm = PaneManager::new();
        pm.set_layout(ActiveLayout::SplitVertical);
        assert_eq!(pm.pane_count(), 2);
        pm.close_focused_pane();
        assert_eq!(pm.pane_count(), 1);
        assert!(matches!(pm.layout(), ActiveLayout::Single));
    }

    #[test]
    fn close_focused_pane_preserves_other_agent() {
        let mut pm = PaneManager::new();
        pm.set_layout(ActiveLayout::SplitHorizontal);

        let id_a = AgentId::new();
        let id_b = AgentId::new();
        pm.assign_to_pane(0, Some(id_a));
        pm.assign_to_pane(1, Some(id_b));

        // Focus is on pane 0, close it
        pm.close_focused_pane();
        // Pane 1's agent (id_b) should now be the sole remaining agent in pane 0
        assert_eq!(pm.pane_count(), 1);
        assert_eq!(pm.agent_in_pane(0), Some(id_b));
    }
}
