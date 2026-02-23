# Feature 11: UI Status Bar

## Overview

Implement the status bar widget that sits at the bottom of the screen. It provides a quick overview of all agent states (aggregate counts), the current input mode, and contextual keybinding hints. This is the "glanceable" information layer — users should be able to understand system status without reading the sidebar.

## Dependencies

- **Feature 03** (Core Types & Event System) — `InputMode` for mode display.
- **Feature 06** (Agent Lifecycle Management) — `StateCounts` for aggregate counts.
- **Feature 08** (Theme & Layout) — `Theme` for styling, `AppLayout::status_bar` for area.

## Technical Specification

### Status Bar Layout

The status bar is a single row with three sections:

```
┌───────────────────────────────────────────────────────────────────────────┐
│ ● 2 running  ? 1 waiting  - 1 idle  ✓ 1 done  ! 1 err  │  -- NORMAL -- │
│ ◄─── agent state counts (left-aligned) ──────────────────►  ◄── mode ──► │
└───────────────────────────────────────────────────────────────────────────┘
```

In Normal Mode, keybinding hints appear between the counts and mode:

```
│ ● 2 running  ? 1 waiting │ j/k:nav  i:insert  n:new  d:kill │ -- NORMAL -- │
```

In Insert Mode, the hint changes:

```
│ ● 2 running  ? 1 waiting │ Esc:normal  Ctrl+\:escape │ -- INSERT (agent) -- │
```

### Status Bar Widget (`src/ui/status_bar.rs`)

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use crate::agent::manager::StateCounts;
use crate::input::mode::InputMode;
use crate::ui::theme::Theme;

/// The bottom status bar widget.
pub struct StatusBar<'a> {
    /// Aggregate state counts.
    state_counts: &'a StateCounts,
    /// Current input mode.
    mode: &'a InputMode,
    /// Theme reference.
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    pub fn new(
        state_counts: &'a StateCounts,
        mode: &'a InputMode,
        theme: &'a Theme,
    ) -> Self {
        Self {
            state_counts,
            mode,
            theme,
        }
    }

    /// Build the left section: agent state counts with colored indicators.
    fn build_counts_spans(&self) -> Vec<Span<'a>> {
        let mut spans = Vec::new();
        let separator = Span::styled("  ", ratatui::style::Style::default());

        if self.state_counts.running > 0 {
            spans.push(Span::styled(
                format!("● {} running", self.state_counts.running),
                self.theme.status_running,
            ));
        }
        if self.state_counts.waiting > 0 {
            if !spans.is_empty() { spans.push(separator.clone()); }
            spans.push(Span::styled(
                format!("? {} waiting", self.state_counts.waiting),
                self.theme.status_waiting,
            ));
        }
        if self.state_counts.idle > 0 {
            if !spans.is_empty() { spans.push(separator.clone()); }
            spans.push(Span::styled(
                format!("- {} idle", self.state_counts.idle),
                self.theme.status_idle,
            ));
        }
        if self.state_counts.completed > 0 {
            if !spans.is_empty() { spans.push(separator.clone()); }
            spans.push(Span::styled(
                format!("✓ {} done", self.state_counts.completed),
                self.theme.status_completed,
            ));
        }
        if self.state_counts.errored > 0 {
            if !spans.is_empty() { spans.push(separator.clone()); }
            spans.push(Span::styled(
                format!("! {} err", self.state_counts.errored),
                self.theme.status_errored,
            ));
        }
        if self.state_counts.spawning > 0 {
            if !spans.is_empty() { spans.push(separator.clone()); }
            spans.push(Span::styled(
                format!("○ {} starting", self.state_counts.spawning),
                self.theme.status_spawning,
            ));
        }

        spans
    }

    /// Build the keybinding hint section based on current mode.
    fn build_hint_spans(&self) -> Vec<Span<'a>> {
        let hint_style = self.theme.status_bar_keybinding_hint;

        match self.mode {
            InputMode::Normal => {
                vec![Span::styled(
                    "j/k:nav  i:insert  n:new  d:kill  ?:help  q:quit",
                    hint_style,
                )]
            }
            InputMode::Insert { .. } => {
                vec![Span::styled(
                    "Esc:normal  Ctrl+\\:escape",
                    hint_style,
                )]
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
        }
    }

    /// Build the mode indicator span.
    fn build_mode_span(&self) -> Span<'a> {
        let (text, style) = match self.mode {
            InputMode::Normal => (
                "-- NORMAL --".to_string(),
                self.theme.status_bar_mode_normal,
            ),
            InputMode::Insert { ref agent_name } => (
                format!("-- INSERT ({}) --", agent_name),
                self.theme.status_bar_mode_insert,
            ),
            InputMode::Command { .. } => (
                "-- COMMAND --".to_string(),
                self.theme.status_bar_mode_command,
            ),
            InputMode::Search { ref query, current_match, match_count } => (
                format!("/{} ({}/{})", query, current_match, match_count),
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

        // Fill background
        let bg_style = ratatui::style::Style::default()
            .bg(self.theme.status_bar_bg)
            .fg(self.theme.status_bar_fg);
        for x in area.x..area.x + area.width {
            buf.get_mut(x, area.y).set_style(bg_style);
        }

        // Build the three sections
        let count_spans = self.build_counts_spans();
        let hint_spans = self.build_hint_spans();
        let mode_span = self.build_mode_span();

        // Calculate widths
        let count_width: usize = count_spans.iter().map(|s| s.width()).sum();
        let mode_width = mode_span.width();
        let hint_width: usize = hint_spans.iter().map(|s| s.width()).sum();

        // Layout: [counts] [flexible space] [hints] [space] [mode]
        let available = area.width as usize;

        // Render counts (left-aligned)
        let mut x = area.x + 1; // 1 char padding
        for span in &count_spans {
            buf.set_span(x, area.y, span, span.width() as u16);
            x += span.width() as u16;
        }

        // Render mode (right-aligned)
        let mode_x = area.x + area.width - mode_width as u16 - 1;
        buf.set_span(mode_x, area.y, &mode_span, mode_width as u16);

        // Render hints (between counts and mode, if there's room)
        let gap = (mode_x as usize).saturating_sub(x as usize);
        if gap > hint_width + 4 {
            // Center the hints in the gap
            let hint_x = x + ((gap - hint_width) / 2) as u16;
            let mut hx = hint_x;
            for span in &hint_spans {
                buf.set_span(hx, area.y, span, span.width() as u16);
                hx += span.width() as u16;
            }
        }
    }
}
```

### Dynamic Hint Adaptation

When the terminal is narrow, keybinding hints are truncated or hidden:

1. **Full width (>100 cols)**: All three sections visible.
2. **Medium width (60-100 cols)**: Hints are shortened ("j/k i:ins n:new").
3. **Narrow (<60 cols)**: Only counts and mode shown, no hints.

```rust
fn build_hint_spans_adaptive(&self, available_width: usize) -> Vec<Span<'a>> {
    let hint_style = self.theme.status_bar_keybinding_hint;

    if available_width < 60 {
        return vec![]; // No hints
    }

    if available_width < 100 {
        // Short hints
        return match self.mode {
            InputMode::Normal => vec![Span::styled("j/k i n d ?", hint_style)],
            InputMode::Insert { .. } => vec![Span::styled("Esc", hint_style)],
            _ => vec![],
        };
    }

    // Full hints (same as build_hint_spans)
    self.build_hint_spans()
}
```

## Implementation Steps

1. **Implement `src/ui/status_bar.rs`**
   - `StatusBar` struct with constructor.
   - `build_counts_spans()` — colored state counts.
   - `build_hint_spans()` — mode-specific keybinding hints.
   - `build_mode_span()` — mode indicator text.
   - `Widget::render()` — layout the three sections.
   - Adaptive hint width logic.

2. **Update `src/ui/mod.rs`**
   - Re-export `StatusBar`.

## Error Handling

| Scenario | Handling |
|---|---|
| Zero counts (no agents) | Show nothing in the counts section — just the mode. |
| Terminal too narrow | Graceful degradation: hide hints first, then truncate counts. |
| Zero-height area | Early return from `render()`. |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_counts_spans_all_states() {
    let counts = StateCounts {
        running: 2, waiting: 1, idle: 1, completed: 0, errored: 1, spawning: 0,
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
    assert!(!text.contains("done")); // 0 completed should not appear
}

#[test]
fn test_counts_spans_empty() {
    let counts = StateCounts::default();
    let mode = InputMode::Normal;
    let theme = Theme::default_dark();
    let bar = StatusBar::new(&counts, &mode, &theme);

    let spans = bar.build_counts_spans();
    assert!(spans.is_empty());
}

#[test]
fn test_mode_span_normal() {
    let counts = StateCounts::default();
    let mode = InputMode::Normal;
    let theme = Theme::default_dark();
    let bar = StatusBar::new(&counts, &mode, &theme);

    let span = bar.build_mode_span();
    assert_eq!(span.content.as_ref(), "-- NORMAL --");
}

#[test]
fn test_mode_span_insert() {
    let counts = StateCounts::default();
    let mode = InputMode::Insert { agent_name: "backend".into() };
    let theme = Theme::default_dark();
    let bar = StatusBar::new(&counts, &mode, &theme);

    let span = bar.build_mode_span();
    assert_eq!(span.content.as_ref(), "-- INSERT (backend) --");
}

#[test]
fn test_hints_change_by_mode() {
    let counts = StateCounts::default();
    let theme = Theme::default_dark();

    let normal = StatusBar::new(&counts, &InputMode::Normal, &theme);
    let normal_hints = normal.build_hint_spans();
    let text: String = normal_hints.iter().map(|s| s.content.to_string()).collect();
    assert!(text.contains("j/k:nav"));

    let insert = StatusBar::new(&counts, &InputMode::Insert { agent_name: "x".into() }, &theme);
    let insert_hints = insert.build_hint_spans();
    let text: String = insert_hints.iter().map(|s| s.content.to_string()).collect();
    assert!(text.contains("Esc:normal"));
}
```

### Snapshot Tests

```rust
#[test]
fn test_status_bar_render_snapshot() {
    let counts = StateCounts { running: 2, waiting: 1, ..Default::default() };
    let mode = InputMode::Normal;
    let theme = Theme::default_dark();
    let bar = StatusBar::new(&counts, &mode, &theme);

    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    bar.render(area, &mut buf);

    insta::assert_snapshot!(buf_to_string(&buf));
}
```

## Acceptance Criteria

- [ ] Status bar renders in the bottom row of the screen.
- [ ] Left section shows colored agent state counts (only non-zero states).
- [ ] Right section shows the current mode indicator (NORMAL, INSERT, COMMAND).
- [ ] Insert mode indicator includes the active agent name.
- [ ] Keybinding hints are shown between counts and mode.
- [ ] Hints adapt to available width (hidden when narrow).
- [ ] Each state count uses the correct color from the theme.
- [ ] Empty state (no agents) shows only the mode indicator.
- [ ] Background fills the entire row.
- [ ] All unit tests pass.
