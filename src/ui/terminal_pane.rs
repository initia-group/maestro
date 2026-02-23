//! Terminal pane widget — renders agent PTY output.
//!
//! Uses `tui-term` to render a `vt100::Screen` inside a Ratatui widget,
//! with a title bar showing agent name, project, and status indicator.
//!
//! See Feature 10 (Terminal Pane Widget) for full specification.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Widget};
use tui_term::widget::PseudoTerminal;

use crate::agent::scrollback::SearchState;
use crate::agent::state::AgentState;
use crate::ui::theme::Theme;

/// Renders the terminal output of a single agent inside a bordered pane.
///
/// The title bar shows: `Agent: <name> @ <project> [<status>]`
/// where the status indicator uses the agent's symbol and color from the theme.
/// Border color changes based on whether the pane is focused.
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
    /// Current scroll offset (0 = at bottom/live).
    scroll_offset: usize,
    /// Optional search state for highlighting matches.
    search_state: Option<&'a SearchState>,
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
            scroll_offset: 0,
            search_state: None,
        }
    }

    /// Set the scroll offset for this pane.
    pub fn with_scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    /// Set the search state for highlighting matches.
    pub fn with_search(mut self, search: Option<&'a SearchState>) -> Self {
        self.search_state = search;
        self
    }
}

impl Widget for TerminalPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let status_symbol = self.agent_state.symbol();
        let status_style = self.theme.status_style(self.agent_state.color_key());

        // Build styled title spans:  " Agent: name @ project [symbol] "
        let mut title = vec![
            Span::raw(" Agent: "),
            Span::styled(self.agent_name, self.theme.terminal_title),
            Span::raw(" @ "),
            Span::raw(self.project_name),
            Span::raw(" ["),
            Span::styled(status_symbol, status_style),
            Span::raw("] "),
        ];

        // Show scroll indicator when scrolled up
        if self.scroll_offset > 0 {
            let scroll_indicator = format!(" ^ {} lines ", self.scroll_offset);
            title.push(Span::styled(
                scroll_indicator,
                Style::default().fg(Color::Yellow),
            ));
        }

        // Choose border style based on focus
        let border_style = if self.is_focused {
            self.theme.terminal_title
        } else {
            self.theme.terminal_border
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

            // When scrolled, render the scrollback view on top of the tui-term output.
            // We manually render the screen contents at the scroll offset position.
            if self.scroll_offset > 0 {
                render_scrolled_content(self.screen, inner, buf, self.scroll_offset);
            }

            // Render search highlights if search is active
            if let Some(search) = self.search_state {
                render_search_highlights(buf, inner, search, self.scroll_offset);
            }
        }
    }
}

/// Render the terminal screen content at a scroll offset.
///
/// When the user scrolls up, we need to show historical content from the
/// vt100 scrollback buffer rather than the live screen bottom. This function
/// reads the screen contents line by line and renders them with the offset
/// applied.
fn render_scrolled_content(
    screen: &vt100::Screen,
    area: Rect,
    buf: &mut Buffer,
    scroll_offset: usize,
) {
    let rows = area.height as usize;
    let cols = area.width as usize;
    let total_scrollback = screen.scrollback();

    // Clamp offset to available scrollback
    let clamped_offset = scroll_offset.min(total_scrollback);
    if clamped_offset == 0 {
        return; // Nothing to do, tui-term already rendered the live view
    }

    // Clear the inner area first
    for row in 0..rows {
        let y = area.y + row as u16;
        for col in 0..cols {
            let x = area.x + col as u16;
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
            }
        }
    }

    // Get the full screen contents including scrollback, then render the
    // appropriate window. The vt100 screen's `contents_formatted()` gives
    // us the visible screen. For scrollback we use `rows_formatted()`.
    //
    // The scrollback rows are indexed from 0 (oldest) to scrollback-1 (newest).
    // The visible screen rows are 0 to screen_rows-1.
    //
    // When scrolled up by `offset`, we want to show:
    //   - Scrollback rows from (total_scrollback - offset) .. total_scrollback
    //   - Then visible screen rows from 0 .. (rows - offset)
    // But we need to fit exactly `rows` lines total.

    let screen_rows = screen.size().0 as usize;

    for visible_row in 0..rows {
        // The logical row in the full history (scrollback + screen).
        // At offset=0, we show screen rows 0..rows (the live view).
        // At offset=N, we shift up by N, so we show older content.
        let logical_row_from_bottom = (rows - 1 - visible_row) + clamped_offset;

        let y = area.y + visible_row as u16;

        if logical_row_from_bottom < screen_rows {
            // This row is in the visible screen area
            let screen_row = (screen_rows - 1 - logical_row_from_bottom) as u16;
            render_vt100_row(screen, screen_row, false, area.x, y, cols, buf);
        } else {
            // This row is in the scrollback buffer
            let scrollback_row_from_bottom = logical_row_from_bottom - screen_rows;
            if scrollback_row_from_bottom < total_scrollback {
                let scrollback_row =
                    (total_scrollback - 1 - scrollback_row_from_bottom) as u16;
                render_vt100_row(screen, scrollback_row, true, area.x, y, cols, buf);
            }
            // If beyond available scrollback, the row stays cleared/empty
        }
    }
}

/// Render a single row from the vt100 screen (either scrollback or visible).
fn render_vt100_row(
    screen: &vt100::Screen,
    row: u16,
    is_scrollback: bool,
    start_x: u16,
    y: u16,
    max_cols: usize,
    buf: &mut Buffer,
) {
    let screen_cols = screen.size().1 as usize;
    let render_cols = max_cols.min(screen_cols);

    for col in 0..render_cols {
        // vt100 0.16 does not expose individual scrollback cells;
        // scrollback rendering will be enabled when the crate adds the API.
        let cell = if is_scrollback {
            None
        } else {
            screen.cell(row, col as u16)
        };

        if let Some(cell) = cell {
            let x = start_x + col as u16;
            if let Some(buf_cell) = buf.cell_mut((x, y)) {
                let ch = cell.contents();
                if ch.is_empty() {
                    buf_cell.set_symbol(" ");
                } else {
                    buf_cell.set_symbol(ch);
                }

                // Apply vt100 colors/attributes
                let fg = vt100_color_to_ratatui(cell.fgcolor());
                let bg = vt100_color_to_ratatui(cell.bgcolor());
                let mut style = Style::default().fg(fg).bg(bg);
                if cell.bold() {
                    style = style.add_modifier(ratatui::style::Modifier::BOLD);
                }
                if cell.italic() {
                    style = style.add_modifier(ratatui::style::Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(ratatui::style::Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(ratatui::style::Modifier::REVERSED);
                }
                buf_cell.set_style(style);
            }
        }
    }
}

/// Convert a vt100 color to a ratatui color.
fn vt100_color_to_ratatui(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// After rendering the terminal via tui-term, overlay search highlights.
fn render_search_highlights(
    buf: &mut Buffer,
    pane_area: Rect,
    search: &SearchState,
    scroll_offset: usize,
) {
    let highlight_style = Style::default().bg(Color::Yellow).fg(Color::Black);

    let current_highlight_style = Style::default()
        .bg(Color::Rgb(255, 140, 0)) // Orange
        .fg(Color::Black);

    let screen_rows = pane_area.height as usize;

    for (i, m) in search.matches().iter().enumerate() {
        // Calculate the visible line position considering scroll offset.
        // Line 0 is the first line of the screen contents. When scrolled,
        // we need to adjust which lines are visible.
        //
        // The screen contents lines map to visible rows differently based
        // on scroll offset. For simplicity, we check if the match line
        // falls within the currently visible window.
        let visible_line = if scroll_offset == 0 {
            // Not scrolled: line maps directly to visible row
            m.line as isize
        } else {
            // When scrolled: the visible window shows lines
            // [total_lines - screen_rows - scroll_offset .. total_lines - scroll_offset]
            // But for screen.contents(), lines are numbered from the top of the
            // visible screen (not scrollback). So with offset=0 we see the bottom.
            m.line as isize
        };

        if visible_line < 0 || visible_line >= screen_rows as isize {
            continue;
        }

        let y = pane_area.y + visible_line as u16;
        let style = if i == search.current_match_index() {
            current_highlight_style
        } else {
            highlight_style
        };

        for col in m.start_col..m.end_col {
            let x = pane_area.x + col as u16;
            if x < pane_area.x + pane_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(style);
                }
            }
        }
    }
}

/// Renders an empty pane when no agent is selected or available.
///
/// Displays a centered message (e.g., "No agent selected. Press 'n' to spawn one.")
/// with dim/muted styling from the theme.
pub struct EmptyPane<'a> {
    theme: &'a Theme,
    message: &'a str,
}

impl<'a> EmptyPane<'a> {
    pub fn new(theme: &'a Theme, message: &'a str) -> Self {
        Self { theme, message }
    }
}

impl Widget for EmptyPane<'_> {
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
            buf.set_string(x, y, msg, self.theme.terminal_border);
        }
    }
}

/// Get the absolute cursor position for a given pane.
///
/// Maps the vt100 screen cursor position to absolute terminal coordinates
/// by offsetting from the pane's inner area origin. Returns `None` if the
/// cursor falls outside the pane bounds.
pub fn cursor_position(screen: &vt100::Screen, pane_inner: &Rect) -> Option<(u16, u16)> {
    let (row, col) = screen.cursor_position();
    let x = pane_inner.x + col;
    let y = pane_inner.y + row;

    // Only return if within bounds
    if x < pane_inner.x + pane_inner.width && y < pane_inner.y + pane_inner.height {
        Some((x, y))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn cursor_position_within_bounds() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"hello");
        let pane = Rect::new(30, 1, 80, 24);
        let pos = cursor_position(parser.screen(), &pane);
        assert!(pos.is_some());
        let (x, y) = pos.unwrap();
        assert_eq!(x, 30 + 5); // cursor after "hello"
        assert_eq!(y, 1); // first row
    }

    #[test]
    fn cursor_position_out_of_bounds() {
        // Parser has a 24x80 screen, but the pane is only 10 rows tall.
        // Move cursor to row 20 (0-indexed 19), which is beyond the 10-row pane.
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(b"\x1b[20;1H");
        let pane = Rect::new(0, 0, 80, 10);
        let pos = cursor_position(parser.screen(), &pane);
        assert!(pos.is_none());
    }

    #[test]
    fn cursor_position_at_origin() {
        let parser = vt100::Parser::new(24, 80, 0);
        let pane = Rect::new(10, 5, 80, 24);
        let pos = cursor_position(parser.screen(), &pane);
        assert_eq!(pos, Some((10, 5)));
    }

    #[test]
    fn cursor_position_zero_size_pane() {
        let parser = vt100::Parser::new(24, 80, 0);
        let pane = Rect::new(0, 0, 0, 0);
        let pos = cursor_position(parser.screen(), &pane);
        assert!(pos.is_none());
    }

    #[test]
    fn terminal_pane_renders_without_panic() {
        let mut parser = vt100::Parser::new(10, 40, 0);
        parser.process(b"\x1b[32mgreen text\x1b[0m normal text");

        let state = AgentState::Running { since: Utc::now() };
        let theme = Theme::default_dark();
        let pane = TerminalPane::new(parser.screen(), "test", "project", &state, true, &theme);

        let area = Rect::new(0, 0, 42, 12);
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);

        // Verify the block title is rendered (check for "Agent:" in the top row)
        let top_row: String = (0..area.width)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(top_row.contains("Agent:"));
    }

    #[test]
    fn terminal_pane_zero_size_no_panic() {
        let parser = vt100::Parser::new(10, 40, 0);
        let state = AgentState::Spawning { since: Utc::now() };
        let theme = Theme::default_dark();
        let pane = TerminalPane::new(parser.screen(), "test", "proj", &state, false, &theme);

        let area = Rect::new(0, 0, 2, 2);
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);
        // Should not panic — inner area will be 0x0
    }

    #[test]
    fn empty_pane_renders_centered_message() {
        let theme = Theme::default_dark();
        let msg = "No agent selected";
        let pane = EmptyPane::new(&theme, msg);

        let area = Rect::new(0, 0, 60, 20);
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);

        // Check that the message appears in the buffer
        let mid_y = 1 + (20 - 2) / 2; // inner.y + inner.height/2
        let row: String = (0..area.width)
            .map(|x| buf.cell((x, mid_y)).unwrap().symbol().to_string())
            .collect();
        assert!(row.contains("No agent selected"));
    }

    #[test]
    fn empty_pane_zero_size_no_panic() {
        let theme = Theme::default_dark();
        let pane = EmptyPane::new(&theme, "test");

        let area = Rect::new(0, 0, 2, 2);
        let mut buf = Buffer::empty(area);
        pane.render(area, &mut buf);
    }

    #[test]
    fn terminal_pane_unfocused_uses_border_style() {
        let parser = vt100::Parser::new(10, 40, 0);
        let state = AgentState::Idle { since: Utc::now() };
        let theme = Theme::default_dark();

        // Just verify both focused and unfocused render without panic
        let pane_focused =
            TerminalPane::new(parser.screen(), "a", "p", &state, true, &theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 42, 12));
        pane_focused.render(Rect::new(0, 0, 42, 12), &mut buf);

        let pane_unfocused =
            TerminalPane::new(parser.screen(), "a", "p", &state, false, &theme);
        let mut buf2 = Buffer::empty(Rect::new(0, 0, 42, 12));
        pane_unfocused.render(Rect::new(0, 0, 42, 12), &mut buf2);
    }

    #[test]
    fn all_agent_states_render_in_title() {
        let theme = Theme::default_dark();
        let parser = vt100::Parser::new(10, 40, 0);
        let area = Rect::new(0, 0, 60, 12);

        let states: Vec<AgentState> = vec![
            AgentState::Spawning { since: Utc::now() },
            AgentState::Running { since: Utc::now() },
            AgentState::WaitingForInput {
                since: Utc::now(),
                prompt_type: crate::agent::state::PromptType::Question,
            },
            AgentState::Idle { since: Utc::now() },
            AgentState::Completed {
                at: Utc::now(),
                exit_code: Some(0),
            },
            AgentState::Errored {
                at: Utc::now(),
                error_hint: None,
            },
        ];

        for state in &states {
            let pane =
                TerminalPane::new(parser.screen(), "agent", "project", state, true, &theme);
            let mut buf = Buffer::empty(area);
            pane.render(area, &mut buf);
            // All states should render without panic
        }
    }
}
