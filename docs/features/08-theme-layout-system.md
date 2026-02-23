# Feature 08: UI Theme & Layout System

## Overview

Implement the color theme system (defining the visual palette for all UI elements) and the layout calculation engine (determining how the screen is divided between sidebar, terminal panes, and status bar). These two concerns are grouped because they're both foundational to rendering but neither produces visible widgets on their own.

## Dependencies

- **Feature 01** (Project Scaffold) — module structure.
- **Feature 02** (Configuration System) — `ThemeConfig`, `UiConfig`, `LayoutMode`.

## Technical Specification

### Theme System (`src/ui/theme.rs`)

The theme defines colors and styles for every visual element. It's a struct, not a trait, because we want concrete types that Ratatui can use directly.

```rust
use ratatui::style::{Color, Modifier, Style};

/// Complete color/style theme for the UI.
#[derive(Debug, Clone)]
pub struct Theme {
    // ─── Agent Status Colors ───────────────────────
    pub status_spawning: Style,
    pub status_running: Style,
    pub status_waiting: Style,
    pub status_idle: Style,
    pub status_completed: Style,
    pub status_errored: Style,

    // ─── Sidebar ───────────────────────────────────
    pub sidebar_bg: Color,
    pub sidebar_fg: Color,
    pub sidebar_selected_bg: Color,
    pub sidebar_selected_fg: Color,
    pub sidebar_project_header: Style,
    pub sidebar_agent_name: Style,
    pub sidebar_agent_name_selected: Style,
    pub sidebar_uptime: Style,
    pub sidebar_border: Style,

    // ─── Terminal Pane ─────────────────────────────
    pub terminal_border: Style,
    pub terminal_title: Style,
    pub terminal_title_status_indicator: Style,

    // ─── Status Bar ────────────────────────────────
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    pub status_bar_mode_normal: Style,
    pub status_bar_mode_insert: Style,
    pub status_bar_mode_command: Style,
    pub status_bar_keybinding_hint: Style,

    // ─── Command Palette ───────────────────────────
    pub palette_bg: Color,
    pub palette_fg: Color,
    pub palette_border: Style,
    pub palette_input: Style,
    pub palette_selected: Style,
    pub palette_description: Style,

    // ─── General ───────────────────────────────────
    pub help_overlay_bg: Color,
    pub help_overlay_fg: Color,
    pub help_key: Style,
    pub help_description: Style,
}

impl Theme {
    /// Create the default dark theme.
    pub fn default_dark() -> Self {
        Self {
            // Agent status
            status_spawning: Style::default().fg(Color::Gray),
            status_running: Style::default().fg(Color::Green),
            status_waiting: Style::default().fg(Color::Yellow),
            status_idle: Style::default().fg(Color::DarkGray),
            status_completed: Style::default().fg(Color::LightGreen),
            status_errored: Style::default().fg(Color::Red),

            // Sidebar
            sidebar_bg: Color::Rgb(30, 30, 40),
            sidebar_fg: Color::White,
            sidebar_selected_bg: Color::Rgb(60, 60, 80),
            sidebar_selected_fg: Color::White,
            sidebar_project_header: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            sidebar_agent_name: Style::default().fg(Color::White),
            sidebar_agent_name_selected: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            sidebar_uptime: Style::default().fg(Color::DarkGray),
            sidebar_border: Style::default().fg(Color::Rgb(60, 60, 80)),

            // Terminal pane
            terminal_border: Style::default().fg(Color::Rgb(60, 60, 80)),
            terminal_title: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            terminal_title_status_indicator: Style::default(), // colored per-status

            // Status bar
            status_bar_bg: Color::Rgb(40, 40, 55),
            status_bar_fg: Color::White,
            status_bar_mode_normal: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            status_bar_mode_insert: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            status_bar_mode_command: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            status_bar_keybinding_hint: Style::default().fg(Color::DarkGray),

            // Command palette
            palette_bg: Color::Rgb(35, 35, 50),
            palette_fg: Color::White,
            palette_border: Style::default().fg(Color::Cyan),
            palette_input: Style::default().fg(Color::White),
            palette_selected: Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(60, 60, 90)),
            palette_description: Style::default().fg(Color::DarkGray),

            // Help overlay
            help_overlay_bg: Color::Rgb(25, 25, 35),
            help_overlay_fg: Color::White,
            help_key: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            help_description: Style::default().fg(Color::Gray),
        }
    }

    /// Create a light theme variant.
    pub fn light() -> Self {
        Self {
            status_spawning: Style::default().fg(Color::Gray),
            status_running: Style::default().fg(Color::DarkGreen),
            status_waiting: Style::default().fg(Color::Rgb(180, 140, 0)),
            status_idle: Style::default().fg(Color::Gray),
            status_completed: Style::default().fg(Color::Green),
            status_errored: Style::default().fg(Color::Red),

            sidebar_bg: Color::Rgb(240, 240, 245),
            sidebar_fg: Color::Black,
            sidebar_selected_bg: Color::Rgb(200, 210, 230),
            sidebar_selected_fg: Color::Black,
            sidebar_project_header: Style::default()
                .fg(Color::DarkGreen)
                .add_modifier(Modifier::BOLD),
            sidebar_agent_name: Style::default().fg(Color::Black),
            sidebar_agent_name_selected: Style::default()
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            sidebar_uptime: Style::default().fg(Color::Gray),
            sidebar_border: Style::default().fg(Color::Rgb(180, 180, 190)),

            terminal_border: Style::default().fg(Color::Rgb(180, 180, 190)),
            terminal_title: Style::default()
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            terminal_title_status_indicator: Style::default(),

            status_bar_bg: Color::Rgb(220, 220, 230),
            status_bar_fg: Color::Black,
            status_bar_mode_normal: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            status_bar_mode_insert: Style::default()
                .fg(Color::DarkGreen)
                .add_modifier(Modifier::BOLD),
            status_bar_mode_command: Style::default()
                .fg(Color::Rgb(180, 140, 0))
                .add_modifier(Modifier::BOLD),
            status_bar_keybinding_hint: Style::default().fg(Color::Gray),

            palette_bg: Color::Rgb(245, 245, 250),
            palette_fg: Color::Black,
            palette_border: Style::default().fg(Color::Blue),
            palette_input: Style::default().fg(Color::Black),
            palette_selected: Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(200, 210, 230)),
            palette_description: Style::default().fg(Color::Gray),

            help_overlay_bg: Color::Rgb(240, 240, 245),
            help_overlay_fg: Color::Black,
            help_key: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            help_description: Style::default().fg(Color::DarkGray),
        }
    }

    /// Create a Gruvbox-inspired theme.
    pub fn gruvbox() -> Self {
        let bg = Color::Rgb(40, 40, 40);
        let fg = Color::Rgb(235, 219, 178);
        let red = Color::Rgb(204, 36, 29);
        let green = Color::Rgb(152, 151, 26);
        let yellow = Color::Rgb(215, 153, 33);
        let blue = Color::Rgb(69, 133, 136);
        let aqua = Color::Rgb(104, 157, 106);
        let orange = Color::Rgb(214, 93, 14);
        let gray = Color::Rgb(146, 131, 116);
        let dark_gray = Color::Rgb(80, 73, 69);

        Self {
            status_spawning: Style::default().fg(gray),
            status_running: Style::default().fg(green),
            status_waiting: Style::default().fg(yellow),
            status_idle: Style::default().fg(dark_gray),
            status_completed: Style::default().fg(aqua),
            status_errored: Style::default().fg(red),

            sidebar_bg: Color::Rgb(50, 48, 47),
            sidebar_fg: fg,
            sidebar_selected_bg: Color::Rgb(80, 73, 69),
            sidebar_selected_fg: fg,
            sidebar_project_header: Style::default()
                .fg(aqua)
                .add_modifier(Modifier::BOLD),
            sidebar_agent_name: Style::default().fg(fg),
            sidebar_agent_name_selected: Style::default()
                .fg(fg)
                .add_modifier(Modifier::BOLD),
            sidebar_uptime: Style::default().fg(gray),
            sidebar_border: Style::default().fg(dark_gray),

            terminal_border: Style::default().fg(dark_gray),
            terminal_title: Style::default()
                .fg(fg)
                .add_modifier(Modifier::BOLD),
            terminal_title_status_indicator: Style::default(),

            status_bar_bg: Color::Rgb(60, 56, 54),
            status_bar_fg: fg,
            status_bar_mode_normal: Style::default()
                .fg(blue)
                .add_modifier(Modifier::BOLD),
            status_bar_mode_insert: Style::default()
                .fg(green)
                .add_modifier(Modifier::BOLD),
            status_bar_mode_command: Style::default()
                .fg(orange)
                .add_modifier(Modifier::BOLD),
            status_bar_keybinding_hint: Style::default().fg(gray),

            palette_bg: Color::Rgb(50, 48, 47),
            palette_fg: fg,
            palette_border: Style::default().fg(aqua),
            palette_input: Style::default().fg(fg),
            palette_selected: Style::default().fg(fg).bg(Color::Rgb(80, 73, 69)),
            palette_description: Style::default().fg(gray),

            help_overlay_bg: bg,
            help_overlay_fg: fg,
            help_key: Style::default()
                .fg(aqua)
                .add_modifier(Modifier::BOLD),
            help_description: Style::default().fg(gray),
        }
    }

    /// Load a theme by name from config.
    pub fn from_name(name: &str) -> Self {
        match name {
            "light" => Self::light(),
            "gruvbox" => Self::gruvbox(),
            "default" | "dark" | _ => Self::default_dark(),
        }
    }

    /// Get the status style for a given agent state color key.
    pub fn status_style(&self, color_key: &str) -> Style {
        match color_key {
            "spawning" => self.status_spawning,
            "running" => self.status_running,
            "waiting" => self.status_waiting,
            "idle" => self.status_idle,
            "completed" => self.status_completed,
            "errored" => self.status_errored,
            _ => Style::default(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}
```

### Layout System (`src/ui/layout.rs`)

Calculates the screen regions for each UI component based on the current layout mode and terminal dimensions.

```rust
use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The calculated layout regions for rendering.
#[derive(Debug, Clone)]
pub struct AppLayout {
    /// The sidebar area (project tree).
    pub sidebar: Rect,
    /// The main content area(s) — one or more terminal panes.
    pub panes: Vec<PaneLayout>,
    /// The status bar area (bottom row).
    pub status_bar: Rect,
}

/// A single terminal pane's layout.
#[derive(Debug, Clone)]
pub struct PaneLayout {
    /// The area for this pane (including border).
    pub area: Rect,
    /// The inner area (excluding border) — actual terminal content.
    pub inner: Rect,
    /// Which agent is displayed in this pane (if any).
    pub agent_index: Option<usize>,
}

/// Layout mode determines how the main area is divided.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveLayout {
    /// Single pane — one agent visible at a time.
    Single,
    /// Horizontal split — 2 panes stacked vertically.
    SplitHorizontal,
    /// Vertical split — 2 panes side by side.
    SplitVertical,
    /// Grid — 2x2 layout with 4 panes.
    Grid,
}

/// Calculate the complete application layout.
///
/// # Arguments
/// * `area` — the total terminal area.
/// * `sidebar_width` — configured sidebar width in columns.
/// * `layout` — the active layout mode.
pub fn calculate_layout(
    area: Rect,
    sidebar_width: u16,
    layout: &ActiveLayout,
) -> AppLayout {
    // Step 1: Split into [sidebar | main] and [status bar]
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),       // sidebar + main (everything except status bar)
            Constraint::Length(1),     // status bar
        ])
        .split(area);

    let main_area = vertical[0];
    let status_bar = vertical[1];

    // Step 2: Split main_area into [sidebar | content]
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sidebar_width),
            Constraint::Min(20),  // minimum content width
        ])
        .split(main_area);

    let sidebar = horizontal[0];
    let content = horizontal[1];

    // Step 3: Split content into panes based on layout mode
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
            // 2x2 grid
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
                PaneLayout { area: top[0], inner: inner_rect(top[0]), agent_index: Some(0) },
                PaneLayout { area: top[1], inner: inner_rect(top[1]), agent_index: Some(1) },
                PaneLayout { area: bottom[0], inner: inner_rect(bottom[0]), agent_index: Some(2) },
                PaneLayout { area: bottom[1], inner: inner_rect(bottom[1]), agent_index: Some(3) },
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

/// Calculate the PTY size for a given pane's inner area.
/// This is used to set/resize the PTY dimensions.
pub fn pane_to_pty_size(inner: &Rect) -> portable_pty::PtySize {
    portable_pty::PtySize {
        rows: inner.height,
        cols: inner.width,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Calculate the overlay area for the command palette (centered, 60% width).
pub fn command_palette_area(area: Rect) -> Rect {
    let width = (area.width as f32 * 0.6) as u16;
    let height = 12.min(area.height.saturating_sub(4));
    let x = (area.width - width) / 2;
    let y = area.height / 4; // upper third

    Rect { x, y, width, height }
}

/// Calculate the overlay area for the help popup (centered, 50% of screen).
pub fn help_overlay_area(area: Rect) -> Rect {
    let width = (area.width as f32 * 0.5) as u16;
    let height = (area.height as f32 * 0.6) as u16;
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;

    Rect { x, y, width, height }
}
```

### Minimum Terminal Size

Maestro requires a minimum terminal size to function. If the terminal is too small:

```rust
/// Minimum terminal dimensions for Maestro to render.
pub const MIN_COLS: u16 = 60;
pub const MIN_ROWS: u16 = 10;

/// Check if the terminal is large enough.
pub fn is_terminal_large_enough(area: Rect) -> bool {
    area.width >= MIN_COLS && area.height >= MIN_ROWS
}
```

If the terminal is too small, the App should render a "Terminal too small" message instead of the normal UI.

## Implementation Steps

1. **Implement `src/ui/theme.rs`**
   - `Theme` struct with all style fields.
   - `default_dark()`, `light()`, `gruvbox()` constructors.
   - `from_name()` factory method.
   - `status_style()` lookup.

2. **Implement `src/ui/layout.rs`**
   - `AppLayout`, `PaneLayout`, `ActiveLayout` structs/enums.
   - `calculate_layout()` for all 4 layout modes.
   - `inner_rect()` helper.
   - `pane_to_pty_size()` for PTY resize.
   - `command_palette_area()` and `help_overlay_area()` for overlays.
   - `MIN_COLS`, `MIN_ROWS` constants and `is_terminal_large_enough()`.

3. **Update `src/ui/mod.rs`**
   - Re-export `Theme`, `AppLayout`, `PaneLayout`, `ActiveLayout`, `calculate_layout`.

## Error Handling

| Scenario | Handling |
|---|---|
| Unknown theme name in config | Fall back to `default_dark()` with a log warning. |
| Terminal too small | Render "Terminal too small (need {MIN_COLS}x{MIN_ROWS})" instead of normal UI. |
| Zero-size pane (extreme terminal resize) | `saturating_sub` prevents underflow. Pane with 0 width/height is skipped during rendering. |

## Testing Strategy

### Unit Tests — Theme

```rust
#[test]
fn test_default_theme_loads() {
    let theme = Theme::default_dark();
    assert_eq!(theme.status_running.fg, Some(Color::Green));
}

#[test]
fn test_theme_from_name() {
    assert_eq!(Theme::from_name("light").sidebar_bg, Theme::light().sidebar_bg);
    assert_eq!(Theme::from_name("gruvbox").sidebar_bg, Theme::gruvbox().sidebar_bg);
    assert_eq!(Theme::from_name("unknown").sidebar_bg, Theme::default_dark().sidebar_bg);
}

#[test]
fn test_status_style_lookup() {
    let theme = Theme::default_dark();
    let running_style = theme.status_style("running");
    assert_eq!(running_style.fg, Some(Color::Green));
}
```

### Unit Tests — Layout

```rust
#[test]
fn test_single_layout_one_pane() {
    let area = Rect::new(0, 0, 120, 40);
    let layout = calculate_layout(area, 28, &ActiveLayout::Single);

    assert_eq!(layout.sidebar.width, 28);
    assert_eq!(layout.panes.len(), 1);
    assert_eq!(layout.status_bar.height, 1);
    assert_eq!(layout.status_bar.y, 39); // bottom row
}

#[test]
fn test_split_h_layout_two_panes() {
    let area = Rect::new(0, 0, 120, 40);
    let layout = calculate_layout(area, 28, &ActiveLayout::SplitHorizontal);

    assert_eq!(layout.panes.len(), 2);
    // Top pane should be above bottom pane
    assert!(layout.panes[0].area.y < layout.panes[1].area.y);
}

#[test]
fn test_split_v_layout_two_panes() {
    let area = Rect::new(0, 0, 120, 40);
    let layout = calculate_layout(area, 28, &ActiveLayout::SplitVertical);

    assert_eq!(layout.panes.len(), 2);
    // Left pane should be left of right pane
    assert!(layout.panes[0].area.x < layout.panes[1].area.x);
}

#[test]
fn test_grid_layout_four_panes() {
    let area = Rect::new(0, 0, 120, 40);
    let layout = calculate_layout(area, 28, &ActiveLayout::Grid);

    assert_eq!(layout.panes.len(), 4);
}

#[test]
fn test_inner_rect_shrinks_by_one() {
    let area = Rect::new(10, 5, 50, 20);
    let inner = inner_rect(area);
    assert_eq!(inner.x, 11);
    assert_eq!(inner.y, 6);
    assert_eq!(inner.width, 48);
    assert_eq!(inner.height, 18);
}

#[test]
fn test_small_terminal_detection() {
    assert!(!is_terminal_large_enough(Rect::new(0, 0, 50, 8)));
    assert!(is_terminal_large_enough(Rect::new(0, 0, 120, 40)));
}

#[test]
fn test_pane_to_pty_size() {
    let inner = Rect::new(30, 1, 90, 38);
    let size = pane_to_pty_size(&inner);
    assert_eq!(size.rows, 38);
    assert_eq!(size.cols, 90);
}
```

## Acceptance Criteria

- [ ] Three themes available: `default`/`dark`, `light`, `gruvbox`.
- [ ] `Theme::from_name()` handles unknown names gracefully (falls back to default).
- [ ] `Theme::status_style()` returns the correct style for each agent state.
- [ ] `calculate_layout()` produces correct regions for Single, SplitH, SplitV, and Grid modes.
- [ ] Sidebar width matches the configured `sidebar_width`.
- [ ] Status bar is always exactly 1 row at the bottom.
- [ ] Pane inner areas account for 1-cell borders.
- [ ] `pane_to_pty_size()` converts pane dimensions to PTY dimensions correctly.
- [ ] Minimum terminal size check works.
- [ ] Command palette and help overlay are centered correctly.
- [ ] All unit tests pass.
