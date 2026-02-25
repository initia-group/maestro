//! Input handler — mode-aware key dispatch.
//!
//! Routes keyboard events to the appropriate handler based on
//! the current input mode (Normal, Insert, Command, Search).

use crate::input::action::{Action, SpawnKind};
use crate::input::mode::{InputMode, NewProjectStep};
use crate::ui::layout::AppLayout;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// Mode-aware input handler that converts key events to actions.
///
/// The handler owns the current input mode and translates each `KeyEvent`
/// into a semantic [`Action`]. Mode transitions (e.g., Esc in Insert mode)
/// are handled internally so the mode state is always consistent.
pub struct InputHandler {
    /// Current input mode.
    mode: InputMode,
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
        }
    }

    /// Get the current input mode (for display in status bar).
    pub fn mode(&self) -> &InputMode {
        &self.mode
    }

    /// Set the input mode directly (used by App when processing actions).
    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
    }

    /// Get a mutable reference to the current input mode.
    pub fn mode_mut(&mut self) -> &mut InputMode {
        &mut self.mode
    }

    /// Process a key event and return the corresponding action.
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        // Copy selection to clipboard.
        // macOS: Command key is NOT forwarded by terminal emulators, so we use
        // Alt+C (Option key is forwarded as Alt) as the macOS-friendly shortcut.
        // Linux: Ctrl+Shift+C is the standard terminal copy shortcut.
        // Both work on both platforms via crossterm.
        // Ctrl+C without Shift must NOT be intercepted — it sends interrupt (0x03) to PTY.
        if key.modifiers == KeyModifiers::ALT && key.code == KeyCode::Char('c') {
            return Action::CopySelection;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
            && key.code == KeyCode::Char('C')
        {
            return Action::CopySelection;
        }

        match &self.mode {
            InputMode::Normal => self.handle_normal_mode(key),
            InputMode::Insert { .. } => self.handle_insert_mode(key),
            InputMode::Command { .. } => self.handle_command_mode(key),
            InputMode::Search { .. } => self.handle_search_mode(key),
            InputMode::Rename { .. } => self.handle_rename_mode(key),
            InputMode::RenameProject { .. } => self.handle_rename_project_mode(key),
            InputMode::SpawnPicker { .. } => self.handle_spawn_picker_mode(key),
            InputMode::NewProject { .. } => self.handle_new_project_mode(key),
        }
    }

    // ─── Normal Mode ──────────────────────────────────

    fn handle_normal_mode(&self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // ── Navigation ──
            (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                Action::SelectNext
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                Action::SelectPrev
            }

            (KeyModifiers::SHIFT, KeyCode::Char('J')) => Action::NextProject,
            (KeyModifiers::SHIFT, KeyCode::Char('K')) => Action::PrevProject,

            (KeyModifiers::ALT, KeyCode::Char('j')) => Action::MoveAgentDown,
            (KeyModifiers::ALT, KeyCode::Char('k')) => Action::MoveAgentUp,

            // ── Jump to agent by number ──
            (KeyModifiers::NONE, KeyCode::Char(c)) if ('1'..='9').contains(&c) => {
                Action::JumpToAgent((c as usize) - ('0' as usize))
            }

            // ── Mode switching ──
            (KeyModifiers::NONE, KeyCode::Enter) | (KeyModifiers::NONE, KeyCode::Char('i')) => {
                Action::EnterInsertMode
            }
            (KeyModifiers::NONE, KeyCode::Char(':'))
            | (KeyModifiers::CONTROL, KeyCode::Char('p')) => Action::OpenCommandPalette,

            // ── Project ──
            (KeyModifiers::SHIFT, KeyCode::Char('P')) => Action::EnterNewProjectMode,

            // ── Agent lifecycle ──
            (KeyModifiers::NONE, KeyCode::Char('n')) => Action::OpenSpawnPicker,
            (KeyModifiers::NONE, KeyCode::Char('d')) => Action::KillAgent,
            (KeyModifiers::NONE, KeyCode::Char('r')) => Action::RestartAgent,
            (KeyModifiers::SHIFT, KeyCode::Char('R')) => Action::EnterRenameMode,
            (KeyModifiers::NONE, KeyCode::F(2)) => Action::EnterRenameProjectMode,

            // ── Layout ──
            (KeyModifiers::NONE, KeyCode::Char('s')) => Action::SplitHorizontal,
            (KeyModifiers::NONE, KeyCode::Char('v')) => Action::SplitVertical,
            (KeyModifiers::NONE, KeyCode::Tab) => Action::CyclePaneFocus,
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => Action::CloseSplit,

            // ── Search/Scroll ──
            (KeyModifiers::NONE, KeyCode::Char('/')) => Action::EnterSearchMode,
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => Action::ScrollUp,
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => Action::ScrollDown,

            // ── Application ──
            (KeyModifiers::SHIFT, KeyCode::Char('X')) => Action::ClearSession,
            (KeyModifiers::NONE, KeyCode::Char('?')) => Action::ToggleHelp,
            (KeyModifiers::NONE, KeyCode::Char('q')) => Action::Quit,

            // ── Unbound key ──
            _ => Action::None,
        }
    }

    // ─── Insert Mode ──────────────────────────────────

    fn handle_insert_mode(&mut self, key: KeyEvent) -> Action {
        tracing::debug!(
            code = ?key.code,
            modifiers = ?key.modifiers,
            kind = ?key.kind,
            "Insert mode received key event"
        );
        match (key.modifiers, key.code) {
            // Ctrl+G exits Insert Mode → Normal Mode.
            // This is the ONLY way to leave Insert Mode so that Esc can be
            // forwarded to the terminal app (Claude Code needs Esc for its UI).
            (KeyModifiers::CONTROL, KeyCode::Char('g')) => {
                self.mode = InputMode::Normal;
                Action::ExitInsertMode
            }

            // Everything else — including Esc — is forwarded to the PTY.
            _ => {
                let bytes = key_event_to_bytes(key);
                tracing::debug!(bytes = ?bytes, "key_event_to_bytes result");
                if bytes.is_empty() {
                    Action::None
                } else {
                    Action::SendToPty(bytes)
                }
            }
        }
    }

    // ─── Command Mode ─────────────────────────────────

    fn handle_command_mode(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Esc closes command palette
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::CloseCommandPalette
            }

            // Enter executes the selected command
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let (input, selected) = if let InputMode::Command {
                    ref input,
                    selected,
                } = self.mode
                {
                    (input.clone(), selected)
                } else {
                    (String::new(), 0)
                };
                self.mode = InputMode::Normal;
                Action::ExecuteCommand(input, selected)
            }

            // Navigation within suggestions
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                if let InputMode::Command {
                    ref mut selected, ..
                } = self.mode
                {
                    *selected = selected.saturating_add(1);
                }
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                if let InputMode::Command {
                    ref mut selected, ..
                } = self.mode
                {
                    *selected = selected.saturating_sub(1);
                }
                Action::None
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if let InputMode::Command {
                    ref mut input,
                    ref mut selected,
                    ..
                } = self.mode
                {
                    input.pop();
                    *selected = 0;
                }
                Action::None
            }

            // Character input
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                if let InputMode::Command {
                    ref mut input,
                    ref mut selected,
                    ..
                } = self.mode
                {
                    input.push(c);
                    *selected = 0;
                }
                Action::None
            }

            _ => Action::None,
        }
    }

    // ─── Mouse Handling ────────────────────────────────

    /// Process a mouse event and return the corresponding action.
    ///
    /// Uses the current layout to determine which region was clicked.
    /// In Insert mode, sidebar clicks and pane focus clicks still work,
    /// but mouse events within the focused pane are ignored (not forwarded to PTY).
    pub fn handle_mouse(&mut self, mouse: MouseEvent, layout: &AppLayout) -> Action {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_left_click(mouse.column, mouse.row, layout)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Update selection endpoint during drag
                for pane in &layout.panes {
                    if is_in_rect(mouse.column, mouse.row, &pane.inner) {
                        let rel_row = mouse.row.saturating_sub(pane.inner.y);
                        let rel_col = mouse.column.saturating_sub(pane.inner.x);
                        return Action::UpdateSelection {
                            row: rel_row,
                            col: rel_col,
                        };
                    }
                }
                Action::None
            }
            MouseEventKind::Up(MouseButton::Left) => Action::FinalizeSelection,
            MouseEventKind::ScrollUp => {
                if is_over_pane(mouse.column, mouse.row, layout) {
                    Action::ScrollUp
                } else {
                    Action::None
                }
            }
            MouseEventKind::ScrollDown => {
                if is_over_pane(mouse.column, mouse.row, layout) {
                    Action::ScrollDown
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        }
    }

    fn handle_left_click(&mut self, col: u16, row: u16, layout: &AppLayout) -> Action {
        // Check if click is in the sidebar
        if is_in_rect(col, row, &layout.sidebar) {
            let relative_row = row.saturating_sub(layout.sidebar.y);
            return Action::SidebarClick {
                row: relative_row as usize,
            };
        }

        // Check if click is in a terminal pane's inner area (start selection)
        for (i, pane) in layout.panes.iter().enumerate() {
            if is_in_rect(col, row, &pane.inner) {
                let rel_row = row.saturating_sub(pane.inner.y);
                let rel_col = col.saturating_sub(pane.inner.x);
                return Action::StartSelection {
                    pane_index: i,
                    row: rel_row,
                    col: rel_col,
                };
            }
        }

        // Check if click is in a terminal pane's border area (focus pane)
        for (i, pane) in layout.panes.iter().enumerate() {
            if is_in_rect(col, row, &pane.area) {
                return Action::PaneFocusClick { pane_index: i };
            }
        }

        Action::None
    }

    // ─── Rename Mode ──────────────────────────────────

    fn handle_rename_mode(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Esc cancels rename
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::CancelRename
            }

            // Enter confirms rename
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let (agent_id, new_name) = if let InputMode::Rename {
                    agent_id,
                    ref input,
                } = self.mode
                {
                    (agent_id, input.clone())
                } else {
                    return Action::None;
                };
                self.mode = InputMode::Normal;
                Action::ConfirmRename { agent_id, new_name }
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if let InputMode::Rename { ref mut input, .. } = self.mode {
                    input.pop();
                }
                Action::None
            }

            // Ctrl+U clears the input
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                if let InputMode::Rename { ref mut input, .. } = self.mode {
                    input.clear();
                }
                Action::None
            }

            // Character input
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                if let InputMode::Rename { ref mut input, .. } = self.mode {
                    input.push(c);
                }
                Action::None
            }

            _ => Action::None,
        }
    }

    // ─── Rename Project Mode ────────────────────────────

    fn handle_rename_project_mode(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Esc cancels rename
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::CancelRenameProject
            }

            // Enter confirms rename
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let (old_name, new_name) = if let InputMode::RenameProject {
                    ref old_name,
                    ref input,
                } = self.mode
                {
                    (old_name.clone(), input.clone())
                } else {
                    return Action::None;
                };
                self.mode = InputMode::Normal;
                Action::ConfirmRenameProject { old_name, new_name }
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if let InputMode::RenameProject { ref mut input, .. } = self.mode {
                    input.pop();
                }
                Action::None
            }

            // Ctrl+U clears the input
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                if let InputMode::RenameProject { ref mut input, .. } = self.mode {
                    input.clear();
                }
                Action::None
            }

            // Character input
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                if let InputMode::RenameProject { ref mut input, .. } = self.mode {
                    input.push(c);
                }
                Action::None
            }

            _ => Action::None,
        }
    }

    // ─── Spawn Picker Mode ─────────────────────────────

    fn handle_spawn_picker_mode(&mut self, key: KeyEvent) -> Action {
        const NUM_OPTIONS: usize = 4;

        match (key.modifiers, key.code) {
            // Esc closes the picker
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::CloseSpawnPicker
            }

            // Enter selects the highlighted option
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let selected = if let InputMode::SpawnPicker { selected } = self.mode {
                    selected
                } else {
                    0
                };
                self.mode = InputMode::Normal;
                let kind = match selected {
                    0 => SpawnKind::Claude,
                    1 => SpawnKind::ClaudeYolo,
                    2 => SpawnKind::ClaudeYoloWorktree,
                    3 => SpawnKind::Terminal,
                    _ => SpawnKind::Claude,
                };
                Action::SpawnVariant(kind)
            }

            // Down / j / Ctrl+n to move selection down (with wrap)
            (KeyModifiers::NONE, KeyCode::Down)
            | (KeyModifiers::NONE, KeyCode::Char('j'))
            | (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                if let InputMode::SpawnPicker { ref mut selected } = self.mode {
                    *selected = (*selected + 1) % NUM_OPTIONS;
                }
                Action::None
            }

            // Up / k / Ctrl+p to move selection up (with wrap)
            (KeyModifiers::NONE, KeyCode::Up)
            | (KeyModifiers::NONE, KeyCode::Char('k'))
            | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                if let InputMode::SpawnPicker { ref mut selected } = self.mode {
                    *selected = if *selected == 0 {
                        NUM_OPTIONS - 1
                    } else {
                        *selected - 1
                    };
                }
                Action::None
            }

            // Quick-select by number: 1-4
            (KeyModifiers::NONE, KeyCode::Char(c)) if ('1'..='4').contains(&c) => {
                self.mode = InputMode::Normal;
                let kind = match c {
                    '1' => SpawnKind::Claude,
                    '2' => SpawnKind::ClaudeYolo,
                    '3' => SpawnKind::ClaudeYoloWorktree,
                    '4' => SpawnKind::Terminal,
                    _ => unreachable!(),
                };
                Action::SpawnVariant(kind)
            }

            _ => Action::None,
        }
    }

    // ─── Search Mode ──────────────────────────────────

    fn handle_search_mode(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Esc cancels search and returns to Normal
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::None
            }

            // Enter confirms search, returns to Normal keeping results
            (KeyModifiers::NONE, KeyCode::Enter) => {
                self.mode = InputMode::Normal;
                Action::None
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if let InputMode::Search { ref mut query } = self.mode {
                    query.pop();
                }
                Action::None
            }

            // Character input
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                if let InputMode::Search { ref mut query } = self.mode {
                    query.push(c);
                }
                Action::None
            }

            _ => Action::None,
        }
    }

    // ─── New Project Mode ───────────────────────────────

    fn handle_new_project_mode(&mut self, key: KeyEvent) -> Action {
        // Determine which step we're in
        let step = if let InputMode::NewProject { ref step, .. } = self.mode {
            step.clone()
        } else {
            return Action::None;
        };

        match step {
            NewProjectStep::Name => self.handle_new_project_name_step(key),
            NewProjectStep::Path => self.handle_new_project_path_step(key),
        }
    }

    fn handle_new_project_name_step(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let name_empty = if let InputMode::NewProject { ref name, .. } = self.mode {
                    name.is_empty()
                } else {
                    true
                };
                if name_empty {
                    Action::None
                } else {
                    Action::NewProjectAdvance
                }
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if let InputMode::NewProject { ref mut name, .. } = self.mode {
                    name.pop();
                }
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                if let InputMode::NewProject { ref mut name, .. } = self.mode {
                    name.push(c);
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_new_project_path_step(&mut self, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = InputMode::Normal;
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let (name, path) = if let InputMode::NewProject {
                    ref name,
                    ref path_input,
                    ..
                } = self.mode
                {
                    (name.clone(), path_input.clone())
                } else {
                    return Action::None;
                };
                self.mode = InputMode::Normal;
                Action::CreateProject { name, path }
            }
            (KeyModifiers::NONE, KeyCode::Tab) => Action::NewProjectTabComplete,
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                if let InputMode::NewProject {
                    ref mut path_input,
                    ref mut selected_completion,
                    ..
                } = self.mode
                {
                    path_input.pop();
                    *selected_completion = 0;
                }
                Action::NewProjectPathChanged
            }
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                if let InputMode::NewProject {
                    ref completions,
                    ref mut selected_completion,
                    ..
                } = self.mode
                {
                    if !completions.is_empty() {
                        *selected_completion = (*selected_completion + 1) % completions.len();
                    }
                }
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                if let InputMode::NewProject {
                    ref completions,
                    ref mut selected_completion,
                    ..
                } = self.mode
                {
                    if !completions.is_empty() {
                        *selected_completion = if *selected_completion == 0 {
                            completions.len() - 1
                        } else {
                            *selected_completion - 1
                        };
                    }
                }
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                if let InputMode::NewProject {
                    ref mut path_input,
                    ref mut selected_completion,
                    ..
                } = self.mode
                {
                    path_input.push(c);
                    *selected_completion = 0;
                }
                Action::NewProjectPathChanged
            }
            _ => Action::None,
        }
    }
}

/// Check if a position is within a rectangle.
fn is_in_rect(col: u16, row: u16, rect: &Rect) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// Check if a position is over any terminal pane.
fn is_over_pane(col: u16, row: u16, layout: &AppLayout) -> bool {
    layout.panes.iter().any(|p| is_in_rect(col, row, &p.area))
}

/// Convert a crossterm `KeyEvent` to the raw bytes that should be sent to a PTY.
///
/// This handles the translation from structured key events back to the byte
/// sequences that terminal applications expect. The mapping follows the xterm
/// protocol, the de facto standard for terminal emulators.
///
/// Key considerations:
/// - **Ctrl+C** (`0x03`), **Ctrl+D** (`0x04`), **Ctrl+Z** (`0x1a`) are forwarded,
///   allowing the user to send interrupt, EOF, and suspend signals.
/// - **Alt+key** is sent as `ESC + key` (the Meta key convention).
/// - **UTF-8 characters** are properly encoded.
pub fn key_event_to_bytes(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        // ── Printable characters ──
        KeyCode::Char(c) if ctrl => {
            // Ctrl+A..Z → 0x01..0x1A
            if c.is_ascii_lowercase() || c.is_ascii_uppercase() {
                let byte = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                if alt {
                    vec![0x1b, byte]
                } else {
                    vec![byte]
                }
            } else {
                vec![]
            }
        }
        KeyCode::Char(c) => {
            let mut bytes = Vec::new();
            if alt {
                bytes.push(0x1b); // ESC prefix for Alt
            }
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            bytes.extend_from_slice(encoded.as_bytes());
            bytes
        }

        // ── Special keys ──
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                vec![0x1b, b'[', b'Z'] // Shift+Tab → CSI Z (reverse tab)
            } else {
                vec![b'\t']
            }
        }
        KeyCode::BackTab => vec![0x1b, b'[', b'Z'],
        KeyCode::Backspace => vec![0x7f], // DEL
        KeyCode::Esc => vec![0x1b],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],

        // ── Arrow keys ──
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],

        // ── Page navigation ──
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],

        // ── Function keys (F1-F12) ──
        KeyCode::F(1) => vec![0x1b, b'O', b'P'],
        KeyCode::F(2) => vec![0x1b, b'O', b'Q'],
        KeyCode::F(3) => vec![0x1b, b'O', b'R'],
        KeyCode::F(4) => vec![0x1b, b'O', b'S'],
        KeyCode::F(5) => vec![0x1b, b'[', b'1', b'5', b'~'],
        KeyCode::F(6) => vec![0x1b, b'[', b'1', b'7', b'~'],
        KeyCode::F(7) => vec![0x1b, b'[', b'1', b'8', b'~'],
        KeyCode::F(8) => vec![0x1b, b'[', b'1', b'9', b'~'],
        KeyCode::F(9) => vec![0x1b, b'[', b'2', b'0', b'~'],
        KeyCode::F(10) => vec![0x1b, b'[', b'2', b'1', b'~'],
        KeyCode::F(11) => vec![0x1b, b'[', b'2', b'3', b'~'],
        KeyCode::F(12) => vec![0x1b, b'[', b'2', b'4', b'~'],
        KeyCode::F(_) => vec![],

        // ── Insert key ──
        KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],

        // ── Null / unrecognized ──
        KeyCode::Null => vec![0x00],

        // ── Catch-all ──
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    // ─── key_event_to_bytes tests ─────────────────────

    #[test]
    fn test_printable_char() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![b'a']);
    }

    #[test]
    fn test_uppercase_char() {
        let key = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert_eq!(key_event_to_bytes(key), vec![b'A']);
    }

    #[test]
    fn test_ctrl_c() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(key), vec![0x03]);
    }

    #[test]
    fn test_ctrl_d() {
        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(key), vec![0x04]);
    }

    #[test]
    fn test_ctrl_z() {
        let key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(key), vec![0x1a]);
    }

    #[test]
    fn test_enter() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![b'\r']);
    }

    #[test]
    fn test_backspace() {
        let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x7f]);
    }

    #[test]
    fn test_tab() {
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![b'\t']);
    }

    #[test]
    fn test_backtab() {
        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'Z']);
    }

    #[test]
    fn test_shift_tab_as_modified_tab() {
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'Z']);
    }

    #[test]
    fn test_esc() {
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b]);
    }

    #[test]
    fn test_arrow_up() {
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'A']);
    }

    #[test]
    fn test_arrow_down() {
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'B']);
    }

    #[test]
    fn test_arrow_right() {
        let key = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'C']);
    }

    #[test]
    fn test_arrow_left() {
        let key = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'D']);
    }

    #[test]
    fn test_alt_x() {
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'x']);
    }

    #[test]
    fn test_ctrl_alt_a() {
        let key = KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert_eq!(key_event_to_bytes(key), vec![0x1b, 0x01]);
    }

    #[test]
    fn test_utf8_char() {
        let key = KeyEvent::new(KeyCode::Char('ö'), KeyModifiers::NONE);
        let bytes = key_event_to_bytes(key);
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "ö");
    }

    #[test]
    fn test_utf8_emoji() {
        let key = KeyEvent::new(KeyCode::Char('🎉'), KeyModifiers::NONE);
        let bytes = key_event_to_bytes(key);
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "🎉");
    }

    #[test]
    fn test_f1() {
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'O', b'P']);
    }

    #[test]
    fn test_f5() {
        let key = KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'1', b'5', b'~']);
    }

    #[test]
    fn test_f12() {
        let key = KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'2', b'4', b'~']);
    }

    #[test]
    fn test_delete() {
        let key = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'3', b'~']);
    }

    #[test]
    fn test_insert_key() {
        let key = KeyEvent::new(KeyCode::Insert, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'2', b'~']);
    }

    #[test]
    fn test_home() {
        let key = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'H']);
    }

    #[test]
    fn test_end() {
        let key = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'F']);
    }

    #[test]
    fn test_page_up() {
        let key = KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'5', b'~']);
    }

    #[test]
    fn test_page_down() {
        let key = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x1b, b'[', b'6', b'~']);
    }

    #[test]
    fn test_null() {
        let key = KeyEvent::new(KeyCode::Null, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), vec![0x00]);
    }

    #[test]
    fn test_unknown_fkey_returns_empty() {
        let key = KeyEvent::new(KeyCode::F(99), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(key), Vec::<u8>::new());
    }

    // ─── Normal Mode tests ────────────────────────────

    #[test]
    fn test_normal_j_selects_next() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::SelectNext);
    }

    #[test]
    fn test_normal_k_selects_prev() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::SelectPrev);
    }

    #[test]
    fn test_normal_down_selects_next() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::SelectNext);
    }

    #[test]
    fn test_normal_up_selects_prev() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::SelectPrev);
    }

    #[test]
    fn test_normal_shift_j_next_project() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('J'), KeyModifiers::SHIFT);
        assert_eq!(handler.handle_key(key), Action::NextProject);
    }

    #[test]
    fn test_normal_shift_k_prev_project() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT);
        assert_eq!(handler.handle_key(key), Action::PrevProject);
    }

    #[test]
    fn test_normal_enter_enters_insert() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::EnterInsertMode);
    }

    #[test]
    fn test_normal_i_enters_insert() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::EnterInsertMode);
    }

    #[test]
    fn test_normal_number_jump() {
        let mut handler = InputHandler::new();
        for n in 1..=9u8 {
            let c = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            assert_eq!(handler.handle_key(key), Action::JumpToAgent(n as usize));
        }
    }

    #[test]
    fn test_normal_colon_opens_palette() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::OpenCommandPalette);
    }

    #[test]
    fn test_normal_ctrl_p_opens_palette() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert_eq!(handler.handle_key(key), Action::OpenCommandPalette);
    }

    #[test]
    fn test_normal_shift_p_enters_new_project_mode() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('P'), KeyModifiers::SHIFT);
        let action = handler.handle_key(key);
        assert_eq!(action, Action::EnterNewProjectMode);
    }

    #[test]
    fn test_normal_n_opens_spawn_picker() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::OpenSpawnPicker);
    }

    #[test]
    fn test_normal_d_kills_agent() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::KillAgent);
    }

    #[test]
    fn test_normal_r_restarts_agent() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::RestartAgent);
    }

    #[test]
    fn test_normal_q_quits() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::Quit);
    }

    #[test]
    fn test_normal_question_help() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::ToggleHelp);
    }

    #[test]
    fn test_normal_slash_search() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::EnterSearchMode);
    }

    #[test]
    fn test_normal_tab_cycle_focus() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::CyclePaneFocus);
    }

    #[test]
    fn test_normal_ctrl_u_scroll_up() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(handler.handle_key(key), Action::ScrollUp);
    }

    #[test]
    fn test_normal_ctrl_d_scroll_down() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(handler.handle_key(key), Action::ScrollDown);
    }

    #[test]
    fn test_normal_unknown_key_is_none() {
        let mut handler = InputHandler::new();
        let key = KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::None);
    }

    // ─── Insert Mode tests ────────────────────────────

    #[test]
    fn test_insert_mode_forwards_keys() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handler.handle_key(key);
        assert_eq!(action, Action::SendToPty(vec![b'a']));
    }

    #[test]
    fn test_insert_mode_esc_forwards_to_pty() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = handler.handle_key(key);
        // Esc is forwarded to the PTY so terminal apps (Claude Code) can use it
        assert_eq!(action, Action::SendToPty(vec![0x1b]));
        assert!(matches!(handler.mode(), InputMode::Insert { .. }));
    }

    #[test]
    #[test]
    fn test_insert_mode_ctrl_g_escapes() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        let action = handler.handle_key(key);
        assert_eq!(action, Action::ExitInsertMode);
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_insert_mode_ctrl_c_forwarded() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = handler.handle_key(key);
        assert_eq!(action, Action::SendToPty(vec![0x03]));
        assert!(matches!(handler.mode(), InputMode::Insert { .. }));
    }

    #[test]
    fn test_insert_mode_forwards_enter() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(handler.handle_key(key), Action::SendToPty(vec![b'\r']));
    }

    #[test]
    fn test_insert_mode_forwards_backtab() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let action = handler.handle_key(key);
        assert_eq!(action, Action::SendToPty(vec![0x1b, b'[', b'Z']));
    }

    #[test]
    fn test_insert_mode_forwards_shift_tab() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        let action = handler.handle_key(key);
        assert_eq!(action, Action::SendToPty(vec![0x1b, b'[', b'Z']));
    }

    #[test]
    fn test_insert_mode_forwards_arrows() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Insert {
            agent_name: "test".into(),
        });

        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(
            handler.handle_key(key),
            Action::SendToPty(vec![0x1b, b'[', b'A'])
        );
    }

    // ─── Command Mode tests ───────────────────────────

    #[test]
    fn test_command_mode_text_input() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Command {
            input: String::new(),
            selected: 0,
        });

        handler.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        handler.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));

        if let InputMode::Command { ref input, .. } = handler.mode() {
            assert_eq!(input, "sp");
        } else {
            panic!("Expected Command mode");
        }
    }

    #[test]
    fn test_command_mode_backspace() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Command {
            input: "abc".into(),
            selected: 0,
        });

        handler.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

        if let InputMode::Command { ref input, .. } = handler.mode() {
            assert_eq!(input, "ab");
        } else {
            panic!("Expected Command mode");
        }
    }

    #[test]
    fn test_command_mode_esc_closes() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Command {
            input: "test".into(),
            selected: 0,
        });

        let action = handler.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(action, Action::CloseCommandPalette);
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_command_mode_enter_executes() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Command {
            input: "kill".into(),
            selected: 0,
        });

        let action = handler.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, Action::ExecuteCommand("kill".into(), 0));
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_command_mode_down_increments_selected() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Command {
            input: String::new(),
            selected: 0,
        });

        handler.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        if let InputMode::Command { selected, .. } = handler.mode() {
            assert_eq!(*selected, 1);
        } else {
            panic!("Expected Command mode");
        }
    }

    #[test]
    fn test_command_mode_up_decrements_selected() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Command {
            input: String::new(),
            selected: 3,
        });

        handler.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        if let InputMode::Command { selected, .. } = handler.mode() {
            assert_eq!(*selected, 2);
        } else {
            panic!("Expected Command mode");
        }
    }

    #[test]
    fn test_command_mode_up_saturates_at_zero() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Command {
            input: String::new(),
            selected: 0,
        });

        handler.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        if let InputMode::Command { selected, .. } = handler.mode() {
            assert_eq!(*selected, 0);
        } else {
            panic!("Expected Command mode");
        }
    }

    // ─── Search Mode tests ────────────────────────────

    #[test]
    fn test_search_mode_text_input() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Search {
            query: String::new(),
        });

        handler.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        handler.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        handler.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

        if let InputMode::Search { ref query } = handler.mode() {
            assert_eq!(query, "foo");
        } else {
            panic!("Expected Search mode");
        }
    }

    #[test]
    fn test_search_mode_esc_returns_to_normal() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Search {
            query: "test".into(),
        });

        handler.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_search_mode_enter_returns_to_normal() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Search {
            query: "test".into(),
        });

        handler.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_search_mode_backspace() {
        let mut handler = InputHandler::new();
        handler.set_mode(InputMode::Search {
            query: "abc".into(),
        });

        handler.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

        if let InputMode::Search { ref query } = handler.mode() {
            assert_eq!(query, "ab");
        } else {
            panic!("Expected Search mode");
        }
    }

    // ─── Default trait test ───────────────────────────

    #[test]
    fn test_default_handler() {
        let handler = InputHandler::default();
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    // ─── Mouse handling tests ────────────────────────

    fn mock_layout(width: u16, height: u16, sidebar_width: u16) -> AppLayout {
        crate::ui::layout::calculate_layout(
            Rect::new(0, 0, width, height),
            sidebar_width,
            &crate::ui::layout::ActiveLayout::Single,
        )
    }

    fn mock_split_layout(width: u16, height: u16, sidebar_width: u16) -> AppLayout {
        crate::ui::layout::calculate_layout(
            Rect::new(0, 0, width, height),
            sidebar_width,
            &crate::ui::layout::ActiveLayout::SplitVertical,
        )
    }

    fn mock_mouse_click(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mock_scroll_up(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mock_scroll_down(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn test_click_in_sidebar() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        let action = handler.handle_mouse(mock_mouse_click(10, 5), &layout);
        assert!(matches!(action, Action::SidebarClick { .. }));
    }

    #[test]
    fn test_click_in_pane_inner_starts_selection() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        let action = handler.handle_mouse(mock_mouse_click(60, 10), &layout);
        // Click in pane inner area starts a text selection
        assert!(matches!(action, Action::StartSelection { .. }));
    }

    #[test]
    fn test_click_in_pane_returns_correct_selection_index() {
        let layout = mock_split_layout(120, 40, 28);
        let mut handler = InputHandler::new();

        // Click well inside first pane inner area (left side, after sidebar)
        // Pane 0 inner: x=29, w=44, y=1, h=37
        let action = handler.handle_mouse(mock_mouse_click(40, 10), &layout);
        assert!(
            matches!(action, Action::StartSelection { pane_index: 0, .. }),
            "Expected StartSelection for pane 0, got: {:?}",
            action
        );

        // Click in second pane inner area (right side)
        let action = handler.handle_mouse(mock_mouse_click(100, 10), &layout);
        assert!(matches!(
            action,
            Action::StartSelection { pane_index: 1, .. }
        ));
    }

    #[test]
    fn test_scroll_up_in_pane() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        let action = handler.handle_mouse(mock_scroll_up(60, 10), &layout);
        assert_eq!(action, Action::ScrollUp);
    }

    #[test]
    fn test_scroll_down_in_pane() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        let action = handler.handle_mouse(mock_scroll_down(60, 10), &layout);
        assert_eq!(action, Action::ScrollDown);
    }

    #[test]
    fn test_scroll_in_sidebar_ignored() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        // Scroll in sidebar area (col=10, within sidebar width=28)
        let action = handler.handle_mouse(mock_scroll_up(10, 10), &layout);
        assert_eq!(action, Action::None);
    }

    #[test]
    fn test_click_on_status_bar_ignored() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        // Click on the bottom row (status bar at row 39)
        let action = handler.handle_mouse(mock_mouse_click(60, 39), &layout);
        assert_eq!(action, Action::None);
    }

    #[test]
    fn test_sidebar_click_relative_row() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        // Click at row 5 in sidebar (sidebar starts at y=0)
        let action = handler.handle_mouse(mock_mouse_click(10, 5), &layout);
        assert_eq!(action, Action::SidebarClick { row: 5 });
    }

    #[test]
    fn test_mouse_move_ignored() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        let mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 60,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        let action = handler.handle_mouse(mouse, &layout);
        assert_eq!(action, Action::None);
    }

    #[test]
    fn test_right_click_ignored() {
        let layout = mock_layout(120, 40, 28);
        let mut handler = InputHandler::new();
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 60,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        let action = handler.handle_mouse(mouse, &layout);
        assert_eq!(action, Action::None);
    }

    // ─── New Project Mode tests ──────────────────────────

    use crate::input::mode::NewProjectStep;

    fn enter_new_project_name_mode(handler: &mut InputHandler) {
        handler.set_mode(InputMode::NewProject {
            step: NewProjectStep::Name,
            name: String::new(),
            path_input: "~/".into(),
            completions: vec![],
            selected_completion: 0,
        });
    }

    fn enter_new_project_path_mode(handler: &mut InputHandler) {
        handler.set_mode(InputMode::NewProject {
            step: NewProjectStep::Path,
            name: "myapp".into(),
            path_input: "~/".into(),
            completions: vec!["~/dev".into(), "~/docs".into()],
            selected_completion: 0,
        });
    }

    #[test]
    fn test_new_project_name_typing() {
        let mut handler = InputHandler::new();
        enter_new_project_name_mode(&mut handler);

        handler.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        handler.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        handler.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

        if let InputMode::NewProject { ref name, .. } = handler.mode() {
            assert_eq!(name, "foo");
        } else {
            panic!("Expected NewProject mode");
        }
    }

    #[test]
    fn test_new_project_name_backspace() {
        let mut handler = InputHandler::new();
        enter_new_project_name_mode(&mut handler);

        handler.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        handler.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        handler.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

        if let InputMode::NewProject { ref name, .. } = handler.mode() {
            assert_eq!(name, "a");
        } else {
            panic!("Expected NewProject mode");
        }
    }

    #[test]
    fn test_new_project_name_enter_advances() {
        let mut handler = InputHandler::new();
        enter_new_project_name_mode(&mut handler);

        handler.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        let action = handler.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, Action::NewProjectAdvance);
    }

    #[test]
    fn test_new_project_name_enter_empty_no_advance() {
        let mut handler = InputHandler::new();
        enter_new_project_name_mode(&mut handler);

        let action = handler.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn test_new_project_name_esc_cancels() {
        let mut handler = InputHandler::new();
        enter_new_project_name_mode(&mut handler);

        handler.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_new_project_path_typing() {
        let mut handler = InputHandler::new();
        enter_new_project_path_mode(&mut handler);

        let action = handler.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(action, Action::NewProjectPathChanged);

        if let InputMode::NewProject { ref path_input, .. } = handler.mode() {
            assert_eq!(path_input, "~/d");
        } else {
            panic!("Expected NewProject mode");
        }
    }

    #[test]
    fn test_new_project_path_tab_completes() {
        let mut handler = InputHandler::new();
        enter_new_project_path_mode(&mut handler);

        let action = handler.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(action, Action::NewProjectTabComplete);
    }

    #[test]
    fn test_new_project_path_down_cycles_completion() {
        let mut handler = InputHandler::new();
        enter_new_project_path_mode(&mut handler);

        handler.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        if let InputMode::NewProject {
            selected_completion,
            ..
        } = handler.mode()
        {
            assert_eq!(*selected_completion, 1);
        } else {
            panic!("Expected NewProject mode");
        }
    }

    #[test]
    fn test_new_project_path_up_wraps() {
        let mut handler = InputHandler::new();
        enter_new_project_path_mode(&mut handler);

        // selected starts at 0, Up should wrap to last (index 1)
        handler.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        if let InputMode::NewProject {
            selected_completion,
            ..
        } = handler.mode()
        {
            assert_eq!(*selected_completion, 1);
        } else {
            panic!("Expected NewProject mode");
        }
    }

    #[test]
    fn test_new_project_path_enter_creates() {
        let mut handler = InputHandler::new();
        enter_new_project_path_mode(&mut handler);

        let action = handler.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            action,
            Action::CreateProject {
                name: "myapp".into(),
                path: "~/".into(),
            }
        );
        assert_eq!(*handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_new_project_path_esc_cancels() {
        let mut handler = InputHandler::new();
        enter_new_project_path_mode(&mut handler);

        handler.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(*handler.mode(), InputMode::Normal);
    }
}
