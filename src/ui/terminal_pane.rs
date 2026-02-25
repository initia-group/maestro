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
use crate::agent::state::{AgentState, PromptType};
use crate::ui::theme::Theme;

/// A text selection within a terminal pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelection {
    /// The pane index where the selection lives.
    pub pane_index: usize,
    /// Starting position (row, col) relative to the pane inner area.
    pub start: (u16, u16),
    /// Current end position (row, col) relative to the pane inner area.
    pub end: (u16, u16),
}

impl TextSelection {
    /// Create a new selection starting at the given position.
    pub fn new(pane_index: usize, row: u16, col: u16) -> Self {
        Self {
            pane_index,
            start: (row, col),
            end: (row, col),
        }
    }

    /// Whether start and end are the same (a click, not a drag).
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Normalize start/end so start <= end (top-left to bottom-right).
    pub fn normalized(&self) -> ((u16, u16), (u16, u16)) {
        if self.start.0 < self.end.0
            || (self.start.0 == self.end.0 && self.start.1 <= self.end.1)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

/// Extract the text within a selection from a vt100 screen.
pub fn extract_selected_text(screen: &vt100::Screen, selection: &TextSelection) -> String {
    let ((sr, sc), (er, ec)) = selection.normalized();
    let contents = screen.contents();
    let lines: Vec<&str> = contents.lines().collect();
    let mut result = String::new();

    for row in sr..=er {
        let row_idx = row as usize;
        if row_idx >= lines.len() {
            break;
        }
        let line = lines[row_idx];
        let chars: Vec<char> = line.chars().collect();

        let start_col = if row == sr { sc as usize } else { 0 };
        let end_col = if row == er {
            (ec as usize + 1).min(chars.len())
        } else {
            chars.len()
        };

        let clamped_start = start_col.min(chars.len());
        let clamped_end = end_col.min(chars.len());
        let selected: String = chars[clamped_start..clamped_end].iter().collect();
        result.push_str(&selected);
        if row < er {
            result.push('\n');
        }
    }

    result
}

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
    /// Optional text selection for highlighting.
    selection: Option<&'a TextSelection>,
    /// Pulse animation phase (0..7) for WaitingForInput indicator.
    pulse_phase: u8,
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
            selection: None,
            pulse_phase: 0,
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

    /// Set the text selection for highlighting.
    pub fn with_selection(mut self, selection: Option<&'a TextSelection>) -> Self {
        self.selection = selection;
        self
    }

    /// Set the pulse animation phase for WaitingForInput indicators.
    pub fn with_pulse_phase(mut self, phase: u8) -> Self {
        self.pulse_phase = phase;
        self
    }
}

impl Widget for TerminalPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let status_symbol = self.agent_state.symbol();
        let status_style = self.theme.status_style(self.agent_state.color_key());
        let detail = self.agent_state.detail_label();
        let is_waiting = matches!(self.agent_state, AgentState::WaitingForInput { .. });
        let is_ask = matches!(
            self.agent_state,
            AgentState::WaitingForInput {
                prompt_type: PromptType::AskUserQuestion { .. },
                ..
            }
        );

        // For WaitingForInput, pulse the symbol background in the title bar.
        // AskUserQuestion gets a distinct blue/purple pulse.
        let symbol_style = if is_ask {
            let pulse_bg = self.theme.pulse_ask_symbol_color(self.pulse_phase);
            status_style.bg(pulse_bg)
        } else if is_waiting {
            let pulse_bg = self.theme.pulse_waiting_symbol_color(self.pulse_phase);
            status_style.bg(pulse_bg)
        } else {
            status_style
        };

        // Build styled title spans:  " Agent: name @ project [symbol label] "
        let mut title = vec![
            Span::raw(" Agent: "),
            Span::styled(self.agent_name, self.theme.terminal_title),
            Span::raw(" @ "),
            Span::raw(self.project_name),
            Span::raw(" ["),
            Span::styled(status_symbol, symbol_style),
            Span::raw(" "),
            Span::styled(detail, symbol_style),
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

        // Choose border style: pulse for WaitingForInput, focus-aware otherwise.
        // AskUserQuestion gets a distinct blue/purple border pulse.
        let border_style = if is_ask {
            let pulse_color = self.theme.pulse_ask_symbol_color(self.pulse_phase);
            Style::default().fg(pulse_color)
        } else if is_waiting {
            let pulse_color = self.theme.pulse_waiting_symbol_color(self.pulse_phase);
            Style::default().fg(pulse_color)
        } else if self.is_focused {
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

        // Render the vt100 screen using tui-term.
        // When scrolled, the caller sets vt100's scrollback offset via
        // set_scrollback() before passing the screen, so PseudoTerminal
        // automatically renders the scrollback-adjusted view.
        if inner.width > 0 && inner.height > 0 {
            let pseudo_term = PseudoTerminal::new(self.screen);
            pseudo_term.render(inner, buf);

            // Render search highlights if search is active
            if let Some(search) = self.search_state {
                render_search_highlights(buf, inner, search, self.scroll_offset);
            }

            // Render selection highlight if a selection is active
            if let Some(sel) = self.selection {
                render_selection_highlight(buf, inner, sel);
            }
        }
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

/// After rendering the terminal, overlay the selection highlight.
fn render_selection_highlight(buf: &mut Buffer, pane_area: Rect, selection: &TextSelection) {
    let selection_style = Style::default()
        .bg(Color::Rgb(68, 138, 255))
        .fg(Color::White);

    let ((sr, sc), (er, ec)) = selection.normalized();

    for row in sr..=er {
        let y = pane_area.y + row;
        if y >= pane_area.y + pane_area.height {
            break;
        }

        let start_col = if row == sr { sc } else { 0 };
        let end_col = if row == er {
            ec + 1
        } else {
            pane_area.width
        };

        for col in start_col..end_col.min(pane_area.width) {
            let x = pane_area.x + col;
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(selection_style);
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
