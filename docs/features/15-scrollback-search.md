# Feature 15: Scrollback & Search (v0.2)

## Overview

Add scrollback (viewing terminal history beyond the visible screen) and text search within agent output. Scrollback lets users review past output with `Ctrl+u`/`Ctrl+d`. Search (`/`) finds text in the scrollback with `n`/`N` for next/previous match. Together, these make it possible to review long agent conversations without losing context.

## Dependencies

- **Feature 04** (PTY Management) — PTY output is the data source.
- **Feature 06** (Agent Lifecycle) — `AgentHandle` stores the scrollback buffer.
- **Feature 07** (Input Handling) — Search mode bindings.
- **Feature 10** (Terminal Pane) — Modified to support scrollback rendering.

## Technical Specification

### Scrollback Buffer Architecture

The `vt100::Parser` has a built-in scrollback buffer (the third parameter to `Parser::new()` sets the scrollback line count). However, for large scrollback, we use a dedicated ring buffer alongside the vt100 parser.

```rust
/// Scrollback buffer for storing historical terminal output.
pub struct ScrollbackBuffer {
    /// Raw output bytes — stored for search and re-parsing.
    raw_bytes: Vec<u8>,

    /// Maximum buffer size in bytes (configurable, default 10MB per agent).
    max_bytes: usize,

    /// Current scroll position (0 = bottom/live, >0 = scrolled up).
    scroll_offset: usize,

    /// Search state.
    search: Option<SearchState>,
}

impl ScrollbackBuffer {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            raw_bytes: Vec::with_capacity(max_bytes / 4),
            max_bytes,
            scroll_offset: 0,
            search: None,
        }
    }

    /// Append new output bytes from the PTY.
    pub fn append(&mut self, data: &[u8]) {
        self.raw_bytes.extend_from_slice(data);

        // Trim from the front if we exceed max size
        if self.raw_bytes.len() > self.max_bytes {
            let trim = self.raw_bytes.len() - self.max_bytes;
            self.raw_bytes.drain(..trim);
        }
    }

    /// Scroll up by half a page.
    pub fn scroll_up(&mut self, page_height: usize) {
        self.scroll_offset += page_height / 2;
    }

    /// Scroll down by half a page.
    pub fn scroll_down(&mut self, page_height: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(page_height / 2);
    }

    /// Whether we're currently scrolled (not at the bottom).
    pub fn is_scrolled(&self) -> bool {
        self.scroll_offset > 0
    }

    /// Reset scroll to bottom (follow live output).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Get the current scroll offset.
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }
}
```

### vt100 Scrollback Integration

The `vt100::Parser` supports scrollback natively. When creating the parser:

```rust
// 10000 lines of scrollback
let parser = vt100::Parser::new(rows, cols, 10000);
```

The `Screen` then provides:
- `screen.scrollback()` — returns the number of scrollback lines.
- `screen.rows_formatted(start, end)` — can access scrollback lines with negative offsets.

For rendering scrollback, we use the vt100 screen with an offset:

```rust
/// Render the terminal pane with scrollback offset.
pub fn render_with_scrollback(
    screen: &vt100::Screen,
    area: Rect,
    buf: &mut Buffer,
    scroll_offset: usize,
) {
    let total_scrollback = screen.scrollback();
    let clamped_offset = scroll_offset.min(total_scrollback);

    // Use tui-term but with a custom scroll position
    // tui-term doesn't natively support scrollback offset,
    // so we need to render manually:

    let rows = area.height as usize;
    let cols = area.width as usize;

    for row in 0..rows {
        let scrollback_row = row as isize - clamped_offset as isize;
        // If scrollback_row < 0, we're in scrollback territory
        // This requires accessing the vt100 screen's internal representation

        // Alternative approach: re-parse raw bytes at the scroll position
        // (more complex but more reliable)
    }
}
```

> **Implementation Note**: The exact API for rendering vt100 scrollback depends on the crate version. If `vt100` doesn't expose convenient scrollback access, store raw bytes in the `ScrollbackBuffer` and re-parse for the visible window using a secondary `vt100::Parser`.

### Scrollback Rendering Approach

**Recommended approach for v0.2**: Use `vt100::Parser`'s built-in scrollback with a large buffer (10000 lines). When scrolled, render from the scrollback portion of the screen instead of the live bottom.

The `vt100::Screen` provides `contents_between(start_row, start_col, end_row, end_col)` which can access scrollback. Rows above 0 are scrollback.

### Search (`SearchState`)

```rust
use regex::Regex;

pub struct SearchState {
    /// The search query (plain text or regex).
    query: String,
    /// Compiled regex (None if query is plain text).
    regex: Option<Regex>,
    /// All match positions: (line_number, start_col, end_col).
    matches: Vec<SearchMatch>,
    /// Current match index.
    current_match: usize,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Line number in the scrollback (0 = most recent).
    pub line: usize,
    /// Column range of the match.
    pub start_col: usize,
    pub end_col: usize,
}

impl SearchState {
    /// Create a new search with a query.
    pub fn new(query: &str) -> Self {
        let regex = Regex::new(query).ok();
        Self {
            query: query.to_string(),
            regex,
            matches: Vec::new(),
            current_match: 0,
        }
    }

    /// Execute the search against the screen content.
    pub fn search(&mut self, screen: &vt100::Screen) {
        self.matches.clear();
        let contents = screen.contents();

        for (line_idx, line) in contents.lines().enumerate() {
            if let Some(ref re) = self.regex {
                for mat in re.find_iter(line) {
                    self.matches.push(SearchMatch {
                        line: line_idx,
                        start_col: mat.start(),
                        end_col: mat.end(),
                    });
                }
            } else {
                // Plain text search
                let query_lower = self.query.to_lowercase();
                let line_lower = line.to_lowercase();
                let mut start = 0;
                while let Some(pos) = line_lower[start..].find(&query_lower) {
                    self.matches.push(SearchMatch {
                        line: line_idx,
                        start_col: start + pos,
                        end_col: start + pos + self.query.len(),
                    });
                    start += pos + 1;
                }
            }
        }
    }

    /// Jump to the next match.
    pub fn next_match(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.current_match = (self.current_match + 1) % self.matches.len();
        self.matches.get(self.current_match)
    }

    /// Jump to the previous match.
    pub fn prev_match(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.current_match = if self.current_match == 0 {
            self.matches.len() - 1
        } else {
            self.current_match - 1
        };
        self.matches.get(self.current_match)
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    pub fn current_match_index(&self) -> usize {
        self.current_match
    }
}
```

### Search Highlighting

When search is active, matched text should be highlighted in the terminal pane:

```rust
/// After rendering the terminal via tui-term, overlay search highlights.
fn render_search_highlights(
    buf: &mut Buffer,
    pane_area: Rect,
    search: &SearchState,
    scroll_offset: usize,
) {
    let highlight_style = Style::default()
        .bg(Color::Yellow)
        .fg(Color::Black);

    let current_highlight_style = Style::default()
        .bg(Color::Rgb(255, 140, 0)) // Orange
        .fg(Color::Black);

    for (i, m) in search.matches.iter().enumerate() {
        // Check if this match is in the visible area
        let visible_line = m.line as isize - scroll_offset as isize;
        if visible_line < 0 || visible_line >= pane_area.height as isize {
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
                buf.get_mut(x, y).set_style(style);
            }
        }
    }
}
```

### Scroll Indicator

When the user is scrolled up, show an indicator in the terminal pane's title bar:

```
Agent: backend-refactor @ myapp [R] ↑ 42 lines
```

This tells the user they're viewing historical output, not live.

## Implementation Steps

1. **Add scrollback to `vt100::Parser` creation**
   - Change `Parser::new(rows, cols, 0)` to `Parser::new(rows, cols, 10000)`.

2. **Add `ScrollbackBuffer` to `AgentHandle`**
   - Store raw bytes for search purposes.
   - Track scroll offset.

3. **Implement scroll actions in `App`**
   - `Ctrl+u` → scroll up half page.
   - `Ctrl+d` → scroll down half page.
   - New output auto-scrolls to bottom (if already at bottom).

4. **Implement `SearchState`**
   - Query compilation, matching, navigation.

5. **Implement Search mode in `InputHandler`**
   - `/` enters search mode.
   - Characters build the query.
   - Enter confirms (switch to Normal, keep results).
   - Esc cancels (clear search).
   - `n`/`N` navigate matches.

6. **Modify `TerminalPane` rendering**
   - Support scroll offset.
   - Render search highlights.
   - Show scroll indicator in title.

## Error Handling

| Scenario | Handling |
|---|---|
| Invalid regex query | Fall back to plain text search. |
| No matches found | Show "(0/0)" in status bar. |
| Scroll beyond available scrollback | Clamp to maximum available. |
| Very large scrollback (memory) | Ring buffer trims old content at `max_bytes`. |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_search_plain_text() {
    let mut parser = vt100::Parser::new(10, 80, 1000);
    parser.process(b"hello world\nfoo bar hello\nbaz");

    let mut search = SearchState::new("hello");
    search.search(parser.screen());
    assert_eq!(search.match_count(), 2);
}

#[test]
fn test_search_regex() {
    let mut parser = vt100::Parser::new(10, 80, 1000);
    parser.process(b"error: foo\nwarning: bar\nerror: baz");

    let mut search = SearchState::new("error:.+");
    search.search(parser.screen());
    assert_eq!(search.match_count(), 2);
}

#[test]
fn test_search_navigation() {
    let mut search = SearchState::new("x");
    search.matches = vec![
        SearchMatch { line: 0, start_col: 0, end_col: 1 },
        SearchMatch { line: 1, start_col: 0, end_col: 1 },
        SearchMatch { line: 2, start_col: 0, end_col: 1 },
    ];

    assert_eq!(search.next_match().unwrap().line, 1);
    assert_eq!(search.next_match().unwrap().line, 2);
    assert_eq!(search.next_match().unwrap().line, 0); // wraps
    assert_eq!(search.prev_match().unwrap().line, 2);
}

#[test]
fn test_scrollback_buffer_trim() {
    let mut buf = ScrollbackBuffer::new(100);
    buf.append(&[0u8; 60]);
    buf.append(&[1u8; 60]);
    assert!(buf.raw_bytes.len() <= 100);
}
```

## Acceptance Criteria

- [ ] `Ctrl+u` scrolls up half a page in the terminal pane.
- [ ] `Ctrl+d` scrolls down half a page.
- [ ] New output auto-scrolls to bottom when already at bottom.
- [ ] Scroll indicator shows in title bar when scrolled up.
- [ ] `/` enters search mode. Query is shown in status bar.
- [ ] Search matches are highlighted in the terminal pane (yellow).
- [ ] Current match has a distinct highlight (orange).
- [ ] `n` jumps to next match, `N` to previous.
- [ ] Match count shown in status bar: "/{query} (3/15)".
- [ ] Enter confirms search (back to Normal, highlights persist).
- [ ] Esc cancels search (back to Normal, highlights cleared).
- [ ] Invalid regex falls back to plain text search.
- [ ] Scrollback buffer doesn't exceed configured memory limit.
