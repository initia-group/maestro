//! Status bar widget — bottom info bar.
//!
//! Shows agent state counts, keyboard hints, and current input mode.
//! See Feature 11 (Status Bar) for the full specification.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Widget;

use crate::input::mode::InputMode;
use crate::ui::theme::Theme;

/// Aggregate counts of agents in each state.
///
/// Used by the status bar to show a quick summary without listing
/// every individual agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateCounts {
    pub spawning: usize,
    pub running: usize,
    pub waiting: usize,
    pub idle: usize,
    pub completed: usize,
    pub errored: usize,
}

impl StateCounts {
    /// Total number of agents across all states.
    pub fn total(&self) -> usize {
        self.spawning + self.running + self.waiting + self.idle + self.completed + self.errored
    }
}

/// The bottom status bar widget.
///
/// Displays three sections laid out horizontally:
/// - **Left**: Colored agent state counts (only non-zero states).
/// - **Center**: Context-aware keybinding hints (hidden when narrow).
/// - **Right**: Current input mode indicator.
pub struct StatusBar<'a> {
    state_counts: &'a StateCounts,
    mode: &'a InputMode,
    theme: &'a Theme,
    /// Optional transient flash message (replaces hints when present).
    flash_message: Option<&'a str>,
}

impl<'a> StatusBar<'a> {
    pub fn new(state_counts: &'a StateCounts, mode: &'a InputMode, theme: &'a Theme) -> Self {
        Self {
            state_counts,
            mode,
            theme,
            flash_message: None,
        }
    }

    /// Set a transient flash message to display in the center section.
    pub fn with_flash_message(mut self, msg: Option<&'a str>) -> Self {
        self.flash_message = msg;
        self
    }

    /// Build the left section: agent state counts with colored indicators.
    ///
    /// Only non-zero counts are shown. Each entry is a colored span like "● 2 running".
    fn build_counts_spans(&self) -> Vec<Span<'a>> {
        let mut spans = Vec::new();
        let separator = Span::styled("  ", Style::default());

        // Order: running, waiting, idle, completed, errored, spawning
        // (most important/actionable first)
        if self.state_counts.running > 0 {
            spans.push(Span::styled(
                format!("● {} running", self.state_counts.running),
                self.theme.status_running,
            ));
        }
        if self.state_counts.waiting > 0 {
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
            spans.push(Span::styled(
                format!("? {} waiting", self.state_counts.waiting),
                self.theme.status_waiting,
            ));
        }
        if self.state_counts.idle > 0 {
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
            spans.push(Span::styled(
                format!("- {} idle", self.state_counts.idle),
                self.theme.status_idle,
            ));
        }
        if self.state_counts.completed > 0 {
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
            spans.push(Span::styled(
                format!("✓ {} done", self.state_counts.completed),
                self.theme.status_completed,
            ));
        }
        if self.state_counts.errored > 0 {
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
            spans.push(Span::styled(
                format!("! {} err", self.state_counts.errored),
                self.theme.status_errored,
            ));
        }
        if self.state_counts.spawning > 0 {
            if !spans.is_empty() {
                spans.push(separator.clone());
            }
            spans.push(Span::styled(
                format!("○ {} starting", self.state_counts.spawning),
                self.theme.status_spawning,
            ));
        }

        spans
    }

    /// Build keybinding hint spans adapted to available width.
    ///
    /// - Wide (>=100): Full hints with descriptions.
    /// - Medium (60..100): Abbreviated hints.
    /// - Narrow (<60): No hints.
    fn build_hint_spans(&self, available_width: u16) -> Vec<Span<'a>> {
        let hint_style = self.theme.status_bar_keybinding_hint;

        if available_width < 60 {
            return vec![];
        }

        if available_width < 100 {
            // Short hints
            return match self.mode {
                InputMode::Normal => vec![Span::styled("j/k i n d ?", hint_style)],
                InputMode::Insert { .. } => vec![Span::styled("Ctrl+G exit", hint_style)],
                InputMode::Command { .. } => vec![Span::styled("Enter Esc", hint_style)],
                InputMode::Search { .. } => vec![Span::styled("Enter n/N Esc", hint_style)],
                InputMode::Rename { .. } => vec![Span::styled("Enter Esc", hint_style)],
                InputMode::SpawnPicker { .. } => vec![Span::styled("1-4 Enter Esc", hint_style)],
                InputMode::NewProject { .. } => vec![Span::styled("Tab Enter Esc", hint_style)],
                InputMode::RenameProject { .. } => vec![Span::styled("Enter Esc", hint_style)],
            };
        }

        // Full hints
        match self.mode {
            InputMode::Normal => {
                vec![Span::styled(
                    "j/k:nav  i:insert  n:new  d:kill  ?:help  q:quit",
                    hint_style,
                )]
            }
            InputMode::Insert { .. } => {
                vec![Span::styled("Ctrl+G:exit insert", hint_style)]
            }
            InputMode::Command { .. } => {
                vec![Span::styled(
                    "Enter:execute  Esc:close  ↑/↓:navigate",
                    hint_style,
                )]
            }
            InputMode::Search { .. } => {
                vec![Span::styled(
                    "Enter:confirm  Esc:cancel  n/N:next/prev",
                    hint_style,
                )]
            }
            InputMode::Rename { .. } => {
                vec![Span::styled(
                    "Enter:confirm  Esc:cancel  Ctrl+U:clear",
                    hint_style,
                )]
            }
            InputMode::SpawnPicker { .. } => {
                vec![Span::styled(
                    "1-4:quick-pick  Enter:select  Esc:cancel  j/k:navigate",
                    hint_style,
                )]
            }
            InputMode::NewProject { .. } => {
                vec![Span::styled(
                    "Tab:complete  Enter:confirm  Esc:cancel  ↑/↓:navigate",
                    hint_style,
                )]
            }
            InputMode::RenameProject { .. } => {
                vec![Span::styled(
                    "Enter:confirm  Esc:cancel  Ctrl+U:clear",
                    hint_style,
                )]
            }
        }
    }

    /// Build the mode indicator span (right section).
    fn build_mode_span(&self) -> Span<'a> {
        let (text, style) = match self.mode {
            InputMode::Normal => ("-- NORMAL --".to_string(), self.theme.status_bar_mode_normal),
            InputMode::Insert { ref agent_name } => (
                format!("-- INSERT ({}) --", agent_name),
                self.theme.status_bar_mode_insert,
            ),
            InputMode::Command { .. } => (
                "-- COMMAND --".to_string(),
                self.theme.status_bar_mode_command,
            ),
            InputMode::Search { ref query } => (
                format!("-- SEARCH: {} --", query),
                self.theme.status_bar_mode_command,
            ),
            InputMode::Rename { ref input, .. } => (
                format!("-- RENAME: {} --", input),
                self.theme.status_bar_mode_command,
            ),
            InputMode::SpawnPicker { .. } => (
                "-- SPAWN --".to_string(),
                self.theme.status_bar_mode_command,
            ),
            InputMode::NewProject { .. } => (
                "-- NEW PROJECT --".to_string(),
                self.theme.status_bar_mode_command,
            ),
            InputMode::RenameProject { ref input, .. } => (
                format!("-- RENAME PROJECT: {} --", input),
                self.theme.status_bar_mode_command,
            ),
        };

        Span::styled(text, style)
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        // Fill background across the entire row.
        let bg_style = Style::default()
            .bg(self.theme.status_bar_bg)
            .fg(self.theme.status_bar_fg);
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_style(bg_style);
        }

        // Build sections.
        let count_spans = self.build_counts_spans();
        let mode_span = self.build_mode_span();
        let hint_spans = if let Some(msg) = self.flash_message {
            let flash_style = Style::default()
                .fg(ratatui::style::Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD);
            vec![Span::styled(msg, flash_style)]
        } else {
            self.build_hint_spans(area.width)
        };

        // Calculate widths.
        let count_width: usize = count_spans.iter().map(|s| s.width()).sum();
        let mode_width = mode_span.width();
        let hint_width: usize = hint_spans.iter().map(|s| s.width()).sum();

        let available = area.width as usize;

        // Check if we even have room for counts + mode + padding.
        // Minimum: 1 pad left + counts + 1 pad + mode + 1 pad right.
        let min_needed = 1 + count_width + 1 + mode_width + 1;
        if available < min_needed {
            // Ultra-narrow: just render mode right-aligned if it fits.
            if available > mode_width {
                let mode_x = area.x + area.width - mode_width as u16 - 1;
                buf.set_span(mode_x, area.y, &mode_span, mode_width as u16);
            }
            return;
        }

        // Render counts (left-aligned with 1-char padding).
        let mut x = area.x + 1;
        for span in &count_spans {
            let w = span.width() as u16;
            buf.set_span(x, area.y, span, w);
            x += w;
        }

        // Render mode (right-aligned with 1-char padding).
        let mode_x = area.x + area.width - mode_width as u16 - 1;
        buf.set_span(mode_x, area.y, &mode_span, mode_width as u16);

        // Render hints (centered in the gap between counts and mode).
        let gap = (mode_x as usize).saturating_sub(x as usize);
        if gap > hint_width + 4 {
            let hint_x = x + ((gap - hint_width) / 2) as u16;
            let mut hx = hint_x;
            for span in &hint_spans {
                let w = span.width() as u16;
                buf.set_span(hx, area.y, span, w);
                hx += w;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── StateCounts ──────────────────────────────────────────

    #[test]
    fn state_counts_default_is_all_zero() {
        let c = StateCounts::default();
        assert_eq!(c.total(), 0);
        assert_eq!(c.spawning, 0);
        assert_eq!(c.running, 0);
        assert_eq!(c.waiting, 0);
        assert_eq!(c.idle, 0);
        assert_eq!(c.completed, 0);
        assert_eq!(c.errored, 0);
    }

    #[test]
    fn state_counts_total() {
        let c = StateCounts {
            spawning: 1,
            running: 2,
            waiting: 3,
            idle: 4,
            completed: 5,
            errored: 6,
        };
        assert_eq!(c.total(), 21);
    }

    // ─── Count Spans ──────────────────────────────────────────

    #[test]
    fn counts_spans_all_states() {
        let counts = StateCounts {
            running: 2,
            waiting: 1,
            idle: 1,
            completed: 0,
            errored: 1,
            spawning: 0,
        };
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let spans = bar.build_counts_spans();
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("2 running"));
        assert!(text.contains("1 waiting"));
        assert!(text.contains("1 idle"));
        assert!(text.contains("1 err"));
        // Zero counts should NOT appear.
        assert!(!text.contains("done"));
        assert!(!text.contains("starting"));
    }

    #[test]
    fn counts_spans_empty() {
        let counts = StateCounts::default();
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let spans = bar.build_counts_spans();
        assert!(spans.is_empty());
    }

    #[test]
    fn counts_spans_single_state() {
        let counts = StateCounts {
            running: 3,
            ..Default::default()
        };
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let spans = bar.build_counts_spans();
        // Should be a single span — no separator.
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "● 3 running");
    }

    // ─── Mode Span ────────────────────────────────────────────

    #[test]
    fn mode_span_normal() {
        let counts = StateCounts::default();
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let span = bar.build_mode_span();
        assert_eq!(span.content.as_ref(), "-- NORMAL --");
        assert_eq!(span.style, theme.status_bar_mode_normal);
    }

    #[test]
    fn mode_span_insert() {
        let counts = StateCounts::default();
        let mode = InputMode::Insert {
            agent_name: "backend".into(),
        };
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let span = bar.build_mode_span();
        assert_eq!(span.content.as_ref(), "-- INSERT (backend) --");
        assert_eq!(span.style, theme.status_bar_mode_insert);
    }

    #[test]
    fn mode_span_command() {
        let counts = StateCounts::default();
        let mode = InputMode::Command {
            input: String::new(),
            selected: 0,
        };
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let span = bar.build_mode_span();
        assert_eq!(span.content.as_ref(), "-- COMMAND --");
        assert_eq!(span.style, theme.status_bar_mode_command);
    }

    #[test]
    fn mode_span_search() {
        let counts = StateCounts::default();
        let mode = InputMode::Search {
            query: "TODO".into(),
        };
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let span = bar.build_mode_span();
        assert_eq!(span.content.as_ref(), "-- SEARCH: TODO --");
        assert_eq!(span.style, theme.status_bar_mode_command);
    }

    // ─── Hint Spans ───────────────────────────────────────────

    #[test]
    fn hints_change_by_mode() {
        let counts = StateCounts::default();
        let theme = Theme::default_dark();

        let normal = StatusBar::new(&counts, &InputMode::Normal, &theme);
        let text: String = normal
            .build_hint_spans(120)
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("j/k:nav"));

        let insert_mode = InputMode::Insert {
            agent_name: "x".into(),
        };
        let insert = StatusBar::new(&counts, &insert_mode, &theme);
        let text: String = insert
            .build_hint_spans(120)
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("Ctrl+G:exit insert"));
    }

    #[test]
    fn hints_hidden_when_narrow() {
        let counts = StateCounts::default();
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &InputMode::Normal, &theme);

        let spans = bar.build_hint_spans(50);
        assert!(spans.is_empty());
    }

    #[test]
    fn hints_abbreviated_at_medium_width() {
        let counts = StateCounts::default();
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &InputMode::Normal, &theme);

        let spans = bar.build_hint_spans(80);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(text, "j/k i n d ?");
    }

    #[test]
    fn hints_full_at_wide_width() {
        let counts = StateCounts::default();
        let theme = Theme::default_dark();

        let cmd_mode = InputMode::Command {
            input: String::new(),
            selected: 0,
        };
        let cmd = StatusBar::new(&counts, &cmd_mode, &theme);
        let text: String = cmd
            .build_hint_spans(120)
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("Enter:execute"));

        let search_mode = InputMode::Search {
            query: String::new(),
        };
        let search = StatusBar::new(&counts, &search_mode, &theme);
        let text: String = search
            .build_hint_spans(120)
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("n/N:next/prev"));
    }

    // ─── Render ───────────────────────────────────────────────

    #[test]
    fn render_zero_area() {
        let counts = StateCounts::default();
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        // Zero height — should not panic.
        let area = Rect::new(0, 0, 80, 0);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);

        // Zero width — should not panic.
        let bar2 = StatusBar::new(&counts, &mode, &theme);
        let area2 = Rect::new(0, 0, 0, 1);
        let mut buf2 = Buffer::empty(area2);
        bar2.render(area2, &mut buf2);
    }

    #[test]
    fn render_wide_has_all_sections() {
        let counts = StateCounts {
            running: 2,
            waiting: 1,
            ..Default::default()
        };
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);

        let content = buf_to_string(&buf);
        // Counts visible.
        assert!(content.contains("2 running"), "missing counts: {}", content);
        assert!(
            content.contains("1 waiting"),
            "missing waiting: {}",
            content
        );
        // Mode visible.
        assert!(
            content.contains("-- NORMAL --"),
            "missing mode: {}",
            content
        );
        // Hints visible.
        assert!(content.contains("j/k:nav"), "missing hints: {}", content);
    }

    #[test]
    fn render_narrow_hides_hints() {
        let counts = StateCounts {
            running: 1,
            ..Default::default()
        };
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);

        let content = buf_to_string(&buf);
        assert!(
            content.contains("1 running"),
            "missing counts: {}",
            content
        );
        assert!(
            content.contains("-- NORMAL --"),
            "missing mode: {}",
            content
        );
        // Hints should NOT appear.
        assert!(!content.contains("j/k:nav"), "hints should be hidden");
    }

    #[test]
    fn render_empty_counts_shows_only_mode() {
        let counts = StateCounts::default();
        let mode = InputMode::Normal;
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);

        let content = buf_to_string(&buf);
        assert!(content.contains("-- NORMAL --"));
        // No counts text.
        assert!(!content.contains("running"));
        assert!(!content.contains("waiting"));
    }

    #[test]
    fn render_insert_mode_shows_agent_name() {
        let counts = StateCounts::default();
        let mode = InputMode::Insert {
            agent_name: "api-server".into(),
        };
        let theme = Theme::default_dark();
        let bar = StatusBar::new(&counts, &mode, &theme);

        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        bar.render(area, &mut buf);

        let content = buf_to_string(&buf);
        assert!(content.contains("-- INSERT (api-server) --"));
    }

    /// Extract the text content from a buffer as a single string.
    fn buf_to_string(buf: &Buffer) -> String {
        let area = buf.area;
        let mut s = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                let cell = &buf[(x, y)];
                s.push_str(cell.symbol());
            }
        }
        s
    }
}
