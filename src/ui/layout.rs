//! Layout calculation engine.
//!
//! Computes the positions and sizes of sidebar, terminal panes,
//! status bar, and overlays for different layout modes.
//!
//! The layout pipeline:
//! 1. Split the terminal area into `[main | status_bar]` (vertical).
//! 2. Split the main area into `[sidebar | content]` (horizontal).
//! 3. Split content into panes according to the [`ActiveLayout`] mode.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Minimum terminal width in columns for Maestro to render.
pub const MIN_COLS: u16 = 60;

/// Minimum terminal height in rows for Maestro to render.
pub const MIN_ROWS: u16 = 10;

/// The calculated layout regions for rendering.
#[derive(Debug, Clone)]
pub struct AppLayout {
    /// The sidebar area (project tree / agent list).
    pub sidebar: Rect,
    /// The main content area(s) — one or more terminal panes.
    pub panes: Vec<PaneLayout>,
    /// The status bar area (bottom row).
    pub status_bar: Rect,
}

/// A single terminal pane's layout information.
#[derive(Debug, Clone)]
pub struct PaneLayout {
    /// The area for this pane (including border).
    pub area: Rect,
    /// The inner area (excluding border) — actual terminal content.
    pub inner: Rect,
    /// Which agent is displayed in this pane (if any).
    pub agent_index: Option<usize>,
}

/// Layout mode determines how the main content area is divided into panes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveLayout {
    /// Single pane — one agent visible at a time.
    Single,
    /// Horizontal split — two panes stacked vertically.
    SplitHorizontal,
    /// Vertical split — two panes side by side.
    SplitVertical,
    /// Grid — 2x2 layout with four panes.
    Grid,
}

/// Check if the terminal is large enough for Maestro to render.
pub fn is_terminal_large_enough(area: Rect) -> bool {
    area.width >= MIN_COLS && area.height >= MIN_ROWS
}

/// Compute effective sidebar width based on terminal width.
///
/// On wider terminals the sidebar gets a few extra columns so agent
/// names aren't truncated while the content area has plenty of room.
/// The result is clamped so the sidebar never exceeds 40% of the
/// terminal width.
fn effective_sidebar_width(configured: u16, terminal_width: u16) -> u16 {
    let extra = if terminal_width >= 180 {
        8
    } else if terminal_width >= 120 {
        4
    } else {
        0
    };
    let width = configured + extra;
    // Never let the sidebar eat more than 40% of the terminal.
    let max_width = terminal_width * 2 / 5;
    width.min(max_width)
}

/// Calculate the complete application layout.
///
/// Divides the terminal area into sidebar, terminal panes, and status bar
/// according to the given layout mode.
///
/// # Arguments
/// * `area` — the total terminal area.
/// * `sidebar_width` — configured sidebar width in columns.
/// * `layout` — the active layout mode.
pub fn calculate_layout(area: Rect, sidebar_width: u16, layout: &ActiveLayout) -> AppLayout {
    let sidebar_width = effective_sidebar_width(sidebar_width, area.width);

    // Step 1: Split into [main content | status bar]
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),   // sidebar + panes
            Constraint::Length(1), // status bar (single row)
        ])
        .split(area);

    let main_area = vertical[0];
    let status_bar = vertical[1];

    // Step 2: Split main area into [sidebar | content]
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_width),
            Constraint::Min(20), // minimum content width
        ])
        .split(main_area);

    let sidebar = horizontal[0];
    let content = horizontal[1];

    // Step 3: Divide content into panes based on layout mode
    let panes = match layout {
        ActiveLayout::Single => {
            vec![PaneLayout {
                area: content,
                inner: inner_rect(content),
                agent_index: Some(0),
            }]
        }

        ActiveLayout::SplitHorizontal => {
            let splits = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(content);
            vec![
                PaneLayout {
                    area: splits[0],
                    inner: inner_rect(splits[0]),
                    agent_index: Some(0),
                },
                PaneLayout {
                    area: splits[1],
                    inner: inner_rect(splits[1]),
                    agent_index: Some(1),
                },
            ]
        }

        ActiveLayout::SplitVertical => {
            let splits = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(content);
            vec![
                PaneLayout {
                    area: splits[0],
                    inner: inner_rect(splits[0]),
                    agent_index: Some(0),
                },
                PaneLayout {
                    area: splits[1],
                    inner: inner_rect(splits[1]),
                    agent_index: Some(1),
                },
            ]
        }

        ActiveLayout::Grid => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(content);

            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);

            let bottom = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);

            vec![
                PaneLayout {
                    area: top[0],
                    inner: inner_rect(top[0]),
                    agent_index: Some(0),
                },
                PaneLayout {
                    area: top[1],
                    inner: inner_rect(top[1]),
                    agent_index: Some(1),
                },
                PaneLayout {
                    area: bottom[0],
                    inner: inner_rect(bottom[0]),
                    agent_index: Some(2),
                },
                PaneLayout {
                    area: bottom[1],
                    inner: inner_rect(bottom[1]),
                    agent_index: Some(3),
                },
            ]
        }
    };

    AppLayout {
        sidebar,
        panes,
        status_bar,
    }
}

/// Calculate the inner area of a pane (1-cell border on each side).
fn inner_rect(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

/// Convert a pane's inner area to PTY dimensions.
///
/// Used when spawning or resizing a PTY to match its pane.
pub fn pane_to_pty_size(inner: &Rect) -> portable_pty::PtySize {
    portable_pty::PtySize {
        rows: inner.height,
        cols: inner.width,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Calculate the overlay area for the command palette (centered, 60% width).
///
/// The palette is positioned in the upper third of the terminal for easy
/// visibility and fast keyboard access.
pub fn command_palette_area(area: Rect) -> Rect {
    let width = (area.width as f32 * 0.6) as u16;
    let height = 12.min(area.height.saturating_sub(4));
    let x = (area.width - width) / 2;
    let y = area.height / 4;

    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Calculate the overlay area for the spawn picker (centered, compact).
///
/// The picker shows 4 items, so it needs 6 rows (4 items + 2 border rows).
pub fn spawn_picker_area(area: Rect) -> Rect {
    let width = 60u16.min(area.width.saturating_sub(4));
    let height = 6u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Calculate the overlay area for the help popup (centered, 50% width x 60% height).
pub fn help_overlay_area(area: Rect) -> Rect {
    let width = (area.width as f32 * 0.5) as u16;
    let height = (area.height as f32 * 0.6) as u16;
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;

    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_layout_one_pane() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::Single);

        assert_eq!(layout.sidebar.width, 32); // 28 + 4 (wide terminal bonus)
        assert_eq!(layout.panes.len(), 1);
        assert_eq!(layout.status_bar.height, 1);
        assert_eq!(layout.status_bar.y, 39); // bottom row
    }

    #[test]
    fn split_h_layout_two_panes() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::SplitHorizontal);

        assert_eq!(layout.panes.len(), 2);
        // Top pane should be above bottom pane.
        assert!(layout.panes[0].area.y < layout.panes[1].area.y);
    }

    #[test]
    fn split_v_layout_two_panes() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::SplitVertical);

        assert_eq!(layout.panes.len(), 2);
        // Left pane should be left of right pane.
        assert!(layout.panes[0].area.x < layout.panes[1].area.x);
    }

    #[test]
    fn grid_layout_four_panes() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::Grid);

        assert_eq!(layout.panes.len(), 4);
    }

    #[test]
    fn inner_rect_shrinks_by_one() {
        let area = Rect::new(10, 5, 50, 20);
        let inner = inner_rect(area);
        assert_eq!(inner.x, 11);
        assert_eq!(inner.y, 6);
        assert_eq!(inner.width, 48);
        assert_eq!(inner.height, 18);
    }

    #[test]
    fn inner_rect_saturates_on_tiny_area() {
        let area = Rect::new(0, 0, 1, 1);
        let inner = inner_rect(area);
        assert_eq!(inner.width, 0);
        assert_eq!(inner.height, 0);
    }

    #[test]
    fn small_terminal_detection() {
        assert!(!is_terminal_large_enough(Rect::new(0, 0, 50, 8)));
        assert!(!is_terminal_large_enough(Rect::new(0, 0, 59, 10)));
        assert!(!is_terminal_large_enough(Rect::new(0, 0, 60, 9)));
        assert!(is_terminal_large_enough(Rect::new(0, 0, 60, 10)));
        assert!(is_terminal_large_enough(Rect::new(0, 0, 120, 40)));
    }

    #[test]
    fn pane_to_pty_size_conversion() {
        let inner = Rect::new(30, 1, 90, 38);
        let size = pane_to_pty_size(&inner);
        assert_eq!(size.rows, 38);
        assert_eq!(size.cols, 90);
    }

    #[test]
    fn command_palette_is_centered() {
        let area = Rect::new(0, 0, 100, 40);
        let palette = command_palette_area(area);
        // Width is 60% of 100 = 60
        assert_eq!(palette.width, 60);
        // Centered: (100 - 60) / 2 = 20
        assert_eq!(palette.x, 20);
    }

    #[test]
    fn help_overlay_is_centered() {
        let area = Rect::new(0, 0, 100, 40);
        let help = help_overlay_area(area);
        // Width is 50% of 100 = 50
        assert_eq!(help.width, 50);
        // Height is 60% of 40 = 24
        assert_eq!(help.height, 24);
        // Centered horizontally: (100 - 50) / 2 = 25
        assert_eq!(help.x, 25);
        // Centered vertically: (40 - 24) / 2 = 8
        assert_eq!(help.y, 8);
    }

    #[test]
    fn sidebar_width_matches_config_on_small_terminal() {
        // Below 120 cols, sidebar uses configured width as-is.
        for width in [20u16, 28, 35, 40] {
            let area = Rect::new(0, 0, 100, 40);
            let layout = calculate_layout(area, width, &ActiveLayout::Single);
            assert_eq!(layout.sidebar.width, width);
        }
    }

    #[test]
    fn sidebar_grows_on_wide_terminal() {
        // At 120 cols, sidebar gets +4 (28 → 32).
        let area = Rect::new(0, 0, 120, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::Single);
        assert_eq!(layout.sidebar.width, 32);

        // At 180 cols, sidebar gets +8 (28 → 36).
        let area = Rect::new(0, 0, 180, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::Single);
        assert_eq!(layout.sidebar.width, 36);
    }

    #[test]
    fn sidebar_clamped_to_40_percent() {
        // On a 60-col terminal, 40% = 24, so configured 28 gets clamped.
        let area = Rect::new(0, 0, 60, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::Single);
        assert_eq!(layout.sidebar.width, 24);
    }

    #[test]
    fn effective_sidebar_width_logic() {
        // Small terminal — no extra width.
        assert_eq!(super::effective_sidebar_width(28, 100), 28);
        // Medium terminal — +4.
        assert_eq!(super::effective_sidebar_width(28, 120), 32);
        assert_eq!(super::effective_sidebar_width(28, 150), 32);
        // Large terminal — +8.
        assert_eq!(super::effective_sidebar_width(28, 180), 36);
        assert_eq!(super::effective_sidebar_width(28, 250), 36);
        // Clamp: 50-col terminal, 40% = 20, configured 28 → clamped to 20.
        assert_eq!(super::effective_sidebar_width(28, 50), 20);
    }

    #[test]
    fn status_bar_always_one_row_at_bottom() {
        for height in [10u16, 20, 40, 60] {
            let area = Rect::new(0, 0, 120, height);
            let layout = calculate_layout(area, 28, &ActiveLayout::Single);
            assert_eq!(layout.status_bar.height, 1);
            assert_eq!(layout.status_bar.y, height - 1);
        }
    }

    #[test]
    fn pane_inner_accounts_for_border() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = calculate_layout(area, 28, &ActiveLayout::Single);
        let pane = &layout.panes[0];

        assert_eq!(pane.inner.x, pane.area.x + 1);
        assert_eq!(pane.inner.y, pane.area.y + 1);
        assert_eq!(pane.inner.width, pane.area.width - 2);
        assert_eq!(pane.inner.height, pane.area.height - 2);
    }

    #[test]
    fn agent_indices_are_sequential() {
        let area = Rect::new(0, 0, 120, 40);

        let single = calculate_layout(area, 28, &ActiveLayout::Single);
        assert_eq!(single.panes[0].agent_index, Some(0));

        let grid = calculate_layout(area, 28, &ActiveLayout::Grid);
        for (i, pane) in grid.panes.iter().enumerate() {
            assert_eq!(pane.agent_index, Some(i));
        }
    }
}
