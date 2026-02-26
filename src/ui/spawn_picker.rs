//! Spawn picker overlay — select what kind of agent to spawn.
//!
//! Provides a compact overlay with 4 fixed options for different
//! agent types: Claude, Claude YOLO, Claude YOLO + worktree, and
//! a plain terminal shell.

use crate::ui::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Clear, Widget};

/// The spawn picker menu items: (label, description).
pub const SPAWN_OPTIONS: &[(&str, &str)] = &[
    ("1. claude", "Regular Claude Code"),
    ("2. claude --dangerously-skip-permissions", "Claude + --dangerously-skip-permissions"),
    ("3. claude --dangerously-skip-permissions -w", "Claude YOLO in a worktree"),
    ("4. terminal", "Plain shell (default shell)"),
];

/// Overlay widget for the spawn picker.
pub struct SpawnPicker<'a> {
    selected: usize,
    theme: &'a Theme,
}

impl<'a> SpawnPicker<'a> {
    pub fn new(selected: usize, theme: &'a Theme) -> Self {
        Self { selected, theme }
    }
}

impl<'a> Widget for SpawnPicker<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Spawn Agent ")
            .borders(Borders::ALL)
            .border_style(self.theme.palette_border)
            .style(
                ratatui::style::Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_fg),
            );

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 10 {
            return;
        }

        for (i, (label, description)) in SPAWN_OPTIONS.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }
            let y = inner.y + i as u16;
            let is_selected = i == self.selected;

            let style = if is_selected {
                self.theme.palette_selected
            } else {
                ratatui::style::Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_fg)
            };

            // Fill background for the row
            for x in inner.x..inner.x + inner.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(style);
                }
            }

            // Render label
            buf.set_string(inner.x + 2, y, label, style);

            // Render description after the label
            let desc_x = inner.x + 2 + label.len() as u16 + 2;
            if desc_x < inner.x + inner.width - 2 {
                let desc_style = if is_selected {
                    style
                } else {
                    self.theme.palette_description
                };
                buf.set_string(desc_x, y, description, desc_style);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_options_has_four_entries() {
        assert_eq!(SPAWN_OPTIONS.len(), 4);
    }

    #[test]
    fn spawn_picker_renders_without_panic() {
        let theme = Theme::default_dark();
        let widget = SpawnPicker::new(0, &theme);
        let area = Rect::new(10, 5, 60, 8);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
    }

    #[test]
    fn spawn_picker_tiny_area_does_not_panic() {
        let theme = Theme::default_dark();
        let widget = SpawnPicker::new(2, &theme);
        let area = Rect::new(0, 0, 5, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
    }

    #[test]
    fn spawn_picker_renders_title() {
        let theme = Theme::default_dark();
        let widget = SpawnPicker::new(0, &theme);
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let mut content = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    content.push_str(cell.symbol());
                }
            }
        }
        assert!(content.contains("Spawn Agent"));
    }
}
