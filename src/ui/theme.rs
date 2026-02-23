//! Theme definitions — color palettes and style presets.
//!
//! Defines the [`Theme`] struct with styles for all UI elements
//! and provides built-in themes: [`Theme::default_dark`], [`Theme::light`],
//! and [`Theme::gruvbox`].

use ratatui::style::{Color, Modifier, Style};

/// Complete color/style theme for the UI.
///
/// Every visual element in Maestro has a corresponding style field here.
/// Themes are constructed via factory methods rather than manual field assignment.
#[derive(Debug, Clone)]
pub struct Theme {
    // ─── Agent Status Colors ───────────────────────
    /// Style for agents in the `Spawning` state.
    pub status_spawning: Style,
    /// Style for agents in the `Running` state.
    pub status_running: Style,
    /// Style for agents in the `Waiting` state.
    pub status_waiting: Style,
    /// Style for agents in the `Idle` state.
    pub status_idle: Style,
    /// Style for agents in the `Completed` state.
    pub status_completed: Style,
    /// Style for agents in the `Errored` state.
    pub status_errored: Style,

    // ─── Sidebar ───────────────────────────────────
    /// Background color for the sidebar area.
    pub sidebar_bg: Color,
    /// Foreground (text) color for the sidebar.
    pub sidebar_fg: Color,
    /// Background color for the selected sidebar item.
    pub sidebar_selected_bg: Color,
    /// Foreground color for the selected sidebar item.
    pub sidebar_selected_fg: Color,
    /// Style for the project header in the sidebar.
    pub sidebar_project_header: Style,
    /// Style for agent names in the sidebar (unselected).
    pub sidebar_agent_name: Style,
    /// Style for the selected agent name in the sidebar.
    pub sidebar_agent_name_selected: Style,
    /// Style for the uptime label in the sidebar.
    pub sidebar_uptime: Style,
    /// Style for the sidebar border.
    pub sidebar_border: Style,

    // ─── Terminal Pane ─────────────────────────────
    /// Style for unfocused terminal pane borders.
    pub terminal_border: Style,
    /// Style for terminal pane titles.
    pub terminal_title: Style,
    /// Base style for the status indicator in terminal titles.
    /// The foreground is typically overridden per-status.
    pub terminal_title_status_indicator: Style,

    // ─── Status Bar ────────────────────────────────
    /// Background color for the status bar.
    pub status_bar_bg: Color,
    /// Foreground (text) color for the status bar.
    pub status_bar_fg: Color,
    /// Style for the mode indicator in Normal mode.
    pub status_bar_mode_normal: Style,
    /// Style for the mode indicator in Insert mode.
    pub status_bar_mode_insert: Style,
    /// Style for the mode indicator in Command mode.
    pub status_bar_mode_command: Style,
    /// Style for keybinding hints in the status bar.
    pub status_bar_keybinding_hint: Style,

    // ─── Command Palette ───────────────────────────
    /// Background color for the command palette overlay.
    pub palette_bg: Color,
    /// Foreground color for the command palette overlay.
    pub palette_fg: Color,
    /// Style for the command palette border.
    pub palette_border: Style,
    /// Style for the input field in the command palette.
    pub palette_input: Style,
    /// Style for the selected item in the command palette.
    pub palette_selected: Style,
    /// Style for item descriptions in the command palette.
    pub palette_description: Style,

    // ─── Help Overlay ──────────────────────────────
    /// Background color for the help overlay.
    pub help_overlay_bg: Color,
    /// Foreground color for the help overlay.
    pub help_overlay_fg: Color,
    /// Style for keybinding labels in the help overlay.
    pub help_key: Style,
    /// Style for keybinding descriptions in the help overlay.
    pub help_description: Style,
}

impl Theme {
    /// Create the default dark theme (blue/cyan focused).
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
            terminal_title_status_indicator: Style::default(),

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
            status_running: Style::default().fg(Color::Green),
            status_waiting: Style::default().fg(Color::Rgb(180, 140, 0)),
            status_idle: Style::default().fg(Color::Gray),
            status_completed: Style::default().fg(Color::Green),
            status_errored: Style::default().fg(Color::Red),

            sidebar_bg: Color::Rgb(240, 240, 245),
            sidebar_fg: Color::Black,
            sidebar_selected_bg: Color::Rgb(200, 210, 230),
            sidebar_selected_fg: Color::Black,
            sidebar_project_header: Style::default()
                .fg(Color::Green)
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
                .fg(Color::Green)
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

    /// Create a Gruvbox-inspired theme (warm earth tones).
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

    /// Load a theme by name.
    ///
    /// Recognized names: `"dark"`, `"default"`, `"light"`, `"gruvbox"`.
    /// Unknown names fall back to [`Theme::default_dark`].
    pub fn from_name(name: &str) -> Self {
        match name {
            "light" => Self::light(),
            "gruvbox" => Self::gruvbox(),
            _ => Self::default_dark(),
        }
    }

    /// Look up the status style for a given agent state key.
    ///
    /// Valid keys: `"spawning"`, `"running"`, `"waiting"`, `"idle"`,
    /// `"completed"`, `"errored"`. Unknown keys return [`Style::default`].
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_loads() {
        let theme = Theme::default_dark();
        assert_eq!(theme.status_running.fg, Some(Color::Green));
    }

    #[test]
    fn theme_from_name_known() {
        assert_eq!(
            Theme::from_name("light").sidebar_bg,
            Theme::light().sidebar_bg
        );
        assert_eq!(
            Theme::from_name("gruvbox").sidebar_bg,
            Theme::gruvbox().sidebar_bg
        );
    }

    #[test]
    fn theme_from_name_unknown_falls_back() {
        assert_eq!(
            Theme::from_name("unknown").sidebar_bg,
            Theme::default_dark().sidebar_bg
        );
        assert_eq!(
            Theme::from_name("default").sidebar_bg,
            Theme::default_dark().sidebar_bg
        );
        assert_eq!(
            Theme::from_name("dark").sidebar_bg,
            Theme::default_dark().sidebar_bg
        );
    }

    #[test]
    fn status_style_lookup() {
        let theme = Theme::default_dark();
        assert_eq!(theme.status_style("running").fg, Some(Color::Green));
        assert_eq!(theme.status_style("errored").fg, Some(Color::Red));
        assert_eq!(theme.status_style("idle").fg, Some(Color::DarkGray));
    }

    #[test]
    fn status_style_unknown_returns_default() {
        let theme = Theme::default_dark();
        assert_eq!(theme.status_style("nonexistent"), Style::default());
    }

    #[test]
    fn default_trait_uses_dark_theme() {
        let theme = Theme::default();
        assert_eq!(theme.sidebar_bg, Theme::default_dark().sidebar_bg);
    }

    #[test]
    fn light_theme_has_light_colors() {
        let theme = Theme::light();
        // Light theme sidebar bg should be a light color (high RGB values).
        assert_eq!(theme.sidebar_bg, Color::Rgb(240, 240, 245));
        assert_eq!(theme.sidebar_fg, Color::Black);
    }

    #[test]
    fn gruvbox_theme_has_warm_tones() {
        let theme = Theme::gruvbox();
        // Gruvbox sidebar bg is a dark warm brown.
        assert_eq!(theme.sidebar_bg, Color::Rgb(50, 48, 47));
    }
}
