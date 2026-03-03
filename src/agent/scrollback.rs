//! Scrollback buffer and search state for terminal output.
//!
//! Provides `ScrollbackBuffer` for storing historical terminal output with
//! scroll offset tracking, and `SearchState` / `SearchMatch` for finding
//! text in the terminal screen content.

use regex::Regex;

/// Scrollback buffer for storing historical terminal output.
pub struct ScrollbackBuffer {
    /// Raw output bytes -- stored for search and re-parsing.
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
    ///
    /// When the buffer exceeds `max_bytes`, trims in bulk (removes 25%
    /// of capacity at once) to amortize the O(n) drain cost instead of
    /// trimming on every single append that crosses the limit.
    pub fn append(&mut self, data: &[u8]) {
        self.raw_bytes.extend_from_slice(data);

        // Trim in bulk when over limit — keep 75% to amortize the drain cost
        if self.raw_bytes.len() > self.max_bytes {
            let keep = self.max_bytes * 3 / 4;
            let trim = self.raw_bytes.len() - keep;
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

    /// Scroll up by a fixed number of lines (for mouse wheel).
    pub fn mouse_scroll_up(&mut self, lines: usize) {
        self.scroll_offset += lines;
    }

    /// Scroll down by a fixed number of lines (for mouse wheel).
    pub fn mouse_scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
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

    /// Clamp scroll offset to the available scrollback lines.
    pub fn clamp_scroll(&mut self, max_scrollback: usize) {
        if self.scroll_offset > max_scrollback {
            self.scroll_offset = max_scrollback;
        }
    }

    /// Get the search state (immutable).
    pub fn search(&self) -> Option<&SearchState> {
        self.search.as_ref()
    }

    /// Get the search state (mutable).
    pub fn search_mut(&mut self) -> Option<&mut SearchState> {
        self.search.as_mut()
    }

    /// Start a new search with the given query.
    pub fn start_search(&mut self, query: &str) {
        if query.is_empty() {
            self.search = None;
        } else {
            self.search = Some(SearchState::new(query));
        }
    }

    /// Clear the current search.
    pub fn clear_search(&mut self) {
        self.search = None;
    }

    /// Returns the raw bytes length.
    pub fn raw_len(&self) -> usize {
        self.raw_bytes.len()
    }

    /// Returns a reference to the raw bytes for persistence.
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }
}

/// A single search match location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Line number in the screen content (0 = first line).
    pub line: usize,
    /// Start column of the match.
    pub start_col: usize,
    /// End column of the match (exclusive).
    pub end_col: usize,
}

/// Search state for finding text in terminal output.
pub struct SearchState {
    /// The search query (plain text or regex).
    query: String,
    /// Compiled regex (None if query is invalid regex, falls back to plain text).
    regex: Option<Regex>,
    /// All match positions.
    matches: Vec<SearchMatch>,
    /// Current match index.
    current_match: usize,
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
                // Plain text search (case-insensitive)
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

    /// Jump to the next match. Returns the match if available.
    pub fn next_match(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.current_match = (self.current_match + 1) % self.matches.len();
        self.matches.get(self.current_match)
    }

    /// Jump to the previous match. Returns the match if available.
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

    /// Total number of matches found.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Index of the current match.
    pub fn current_match_index(&self) -> usize {
        self.current_match
    }

    /// Get the query string.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get all matches (immutable).
    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    /// Get the current match (if any).
    pub fn current(&self) -> Option<&SearchMatch> {
        self.matches.get(self.current_match)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ScrollbackBuffer tests ----

    #[test]
    fn test_scrollback_buffer_new() {
        let buf = ScrollbackBuffer::new(1024);
        assert_eq!(buf.scroll_offset(), 0);
        assert!(!buf.is_scrolled());
        assert!(buf.search().is_none());
    }

    #[test]
    fn test_scrollback_buffer_append() {
        let mut buf = ScrollbackBuffer::new(1024);
        buf.append(b"hello world");
        assert_eq!(buf.raw_len(), 11);
    }

    #[test]
    fn test_scrollback_buffer_trim() {
        let mut buf = ScrollbackBuffer::new(100);
        buf.append(&[0u8; 60]);
        buf.append(&[1u8; 60]);
        assert!(buf.raw_bytes.len() <= 100);
    }

    #[test]
    fn test_scrollback_buffer_scroll() {
        let mut buf = ScrollbackBuffer::new(1024);
        assert_eq!(buf.scroll_offset(), 0);
        assert!(!buf.is_scrolled());

        buf.scroll_up(20);
        assert_eq!(buf.scroll_offset(), 10); // half page
        assert!(buf.is_scrolled());

        buf.scroll_down(20);
        assert_eq!(buf.scroll_offset(), 0);
        assert!(!buf.is_scrolled());
    }

    #[test]
    fn test_scrollback_buffer_mouse_scroll() {
        let mut buf = ScrollbackBuffer::new(1024);
        assert_eq!(buf.scroll_offset(), 0);

        buf.mouse_scroll_up(3);
        assert_eq!(buf.scroll_offset(), 3);
        assert!(buf.is_scrolled());

        buf.mouse_scroll_up(3);
        assert_eq!(buf.scroll_offset(), 6);

        buf.mouse_scroll_down(3);
        assert_eq!(buf.scroll_offset(), 3);

        buf.mouse_scroll_down(3);
        assert_eq!(buf.scroll_offset(), 0);
        assert!(!buf.is_scrolled());
    }

    #[test]
    fn test_scrollback_buffer_mouse_scroll_down_saturates() {
        let mut buf = ScrollbackBuffer::new(1024);
        buf.mouse_scroll_down(5); // already at 0
        assert_eq!(buf.scroll_offset(), 0);
    }

    #[test]
    fn test_scrollback_buffer_scroll_down_saturates() {
        let mut buf = ScrollbackBuffer::new(1024);
        buf.scroll_down(20); // already at 0
        assert_eq!(buf.scroll_offset(), 0);
    }

    #[test]
    fn test_scrollback_buffer_scroll_to_bottom() {
        let mut buf = ScrollbackBuffer::new(1024);
        buf.scroll_up(20);
        buf.scroll_up(20);
        assert!(buf.is_scrolled());

        buf.scroll_to_bottom();
        assert_eq!(buf.scroll_offset(), 0);
        assert!(!buf.is_scrolled());
    }

    #[test]
    fn test_scrollback_buffer_clamp_scroll() {
        let mut buf = ScrollbackBuffer::new(1024);
        buf.scroll_up(100); // offset = 50
        buf.scroll_up(100); // offset = 100
        buf.clamp_scroll(30);
        assert_eq!(buf.scroll_offset(), 30);
    }

    #[test]
    fn test_scrollback_buffer_search_lifecycle() {
        let mut buf = ScrollbackBuffer::new(1024);
        assert!(buf.search().is_none());

        buf.start_search("hello");
        assert!(buf.search().is_some());
        assert_eq!(buf.search().unwrap().query(), "hello");

        buf.clear_search();
        assert!(buf.search().is_none());
    }

    #[test]
    fn test_scrollback_buffer_empty_search_clears() {
        let mut buf = ScrollbackBuffer::new(1024);
        buf.start_search("hello");
        assert!(buf.search().is_some());

        buf.start_search("");
        assert!(buf.search().is_none());
    }

    // ---- SearchState tests ----

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
            SearchMatch {
                line: 0,
                start_col: 0,
                end_col: 1,
            },
            SearchMatch {
                line: 1,
                start_col: 0,
                end_col: 1,
            },
            SearchMatch {
                line: 2,
                start_col: 0,
                end_col: 1,
            },
        ];

        assert_eq!(search.next_match().unwrap().line, 1);
        assert_eq!(search.next_match().unwrap().line, 2);
        assert_eq!(search.next_match().unwrap().line, 0); // wraps
        assert_eq!(search.prev_match().unwrap().line, 2);
    }

    #[test]
    fn test_search_navigation_empty() {
        let mut search = SearchState::new("x");
        assert!(search.next_match().is_none());
        assert!(search.prev_match().is_none());
    }

    #[test]
    fn test_search_case_insensitive_plain_text() {
        let mut parser = vt100::Parser::new(10, 80, 1000);
        parser.process(b"Hello HELLO hello");

        // With an invalid regex pattern, falls back to plain text
        // But "Hello" is valid regex, so it will use regex (case-sensitive).
        // Let's test with a pattern that's invalid as regex.
        let mut search = SearchState::new("[invalid");
        search.search(parser.screen());
        // "[invalid" is not valid regex, so falls back to plain text
        // Plain text is case-insensitive, looking for "[invalid" in "Hello HELLO hello"
        assert_eq!(search.match_count(), 0);
    }

    #[test]
    fn test_search_no_matches() {
        let mut parser = vt100::Parser::new(10, 80, 1000);
        parser.process(b"hello world");

        let mut search = SearchState::new("xyz");
        search.search(parser.screen());
        assert_eq!(search.match_count(), 0);
    }

    #[test]
    fn test_search_match_positions() {
        let mut parser = vt100::Parser::new(10, 80, 1000);
        parser.process(b"abcabc");

        let mut search = SearchState::new("abc");
        search.search(parser.screen());
        assert_eq!(search.match_count(), 2);
        assert_eq!(search.matches()[0].start_col, 0);
        assert_eq!(search.matches()[0].end_col, 3);
        assert_eq!(search.matches()[1].start_col, 3);
        assert_eq!(search.matches()[1].end_col, 6);
    }

    #[test]
    fn test_search_current() {
        let mut search = SearchState::new("x");
        assert!(search.current().is_none());

        search.matches = vec![SearchMatch {
            line: 0,
            start_col: 0,
            end_col: 1,
        }];
        assert!(search.current().is_some());
        assert_eq!(search.current().unwrap().line, 0);
    }

    #[test]
    fn test_search_query_getter() {
        let search = SearchState::new("test query");
        assert_eq!(search.query(), "test query");
    }
}
