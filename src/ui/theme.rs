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
    /// Row background tint for agents in the Running state.
    pub sidebar_row_bg_running: Option<Color>,
    /// Row background tint for agents in the WaitingForInput state.
    pub sidebar_row_bg_waiting: Option<Color>,
    /// Row background tint for agents in the Completed state.
    pub sidebar_row_bg_completed: Option<Color>,
    /// Row background tint for agents in the Errored state.
    pub sidebar_row_bg_errored: Option<Color>,

    // ─── Status Symbol Background ─────────────────────
    /// Background color for the status symbol when agent is Running.
    pub status_symbol_bg_running: Option<Color>,
    /// Background color for the status symbol when agent is WaitingForInput.
    pub status_symbol_bg_waiting: Option<Color>,
    /// Background color for the status symbol when agent Completed (unread).
    pub status_symbol_bg_completed_unread: Option<Color>,
    /// Background color for the status symbol when agent Errored.
    pub status_symbol_bg_errored: Option<Color>,

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

    // ─── Pulse Animation ──────────────────────────
    /// Dim phase color for the waiting status symbol pulse.
    pub pulse_waiting_dim: Color,
    /// Bright phase color for the waiting status symbol pulse.
    pub pulse_waiting_bright: Color,
    /// Dim phase color for the waiting row background pulse.
    pub pulse_waiting_row_dim: Color,
    /// Bright phase color for the waiting row background pulse.
    pub pulse_waiting_row_bright: Color,

    // ─── AskUserQuestion Pulse (blue/purple) ─────
    /// Dim phase color for the AskUserQuestion symbol pulse.
    pub pulse_ask_dim: Color,
    /// Bright phase color for the AskUserQuestion symbol pulse.
    pub pulse_ask_bright: Color,
    /// Dim phase color for the AskUserQuestion row background pulse.
    pub pulse_ask_row_dim: Color,
    /// Bright phase color for the AskUserQuestion row background pulse.
    pub pulse_ask_row_bright: Color,
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
            sidebar_row_bg_running: Some(Color::Rgb(25, 45, 28)),
            sidebar_row_bg_waiting: Some(Color::Rgb(48, 38, 18)),
            sidebar_row_bg_completed: Some(Color::Rgb(22, 42, 42)),
            sidebar_row_bg_errored: Some(Color::Rgb(50, 22, 22)),
            status_symbol_bg_running: Some(Color::Rgb(30, 100, 50)),
            status_symbol_bg_waiting: Some(Color::Rgb(160, 120, 20)),
            status_symbol_bg_completed_unread: Some(Color::Rgb(20, 110, 110)),
            status_symbol_bg_errored: Some(Color::Rgb(160, 30, 30)),

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

            // Pulse animation
            pulse_waiting_dim: Color::Rgb(120, 90, 10),
            pulse_waiting_bright: Color::Rgb(220, 180, 30),
            pulse_waiting_row_dim: Color::Rgb(40, 33, 15),
            pulse_waiting_row_bright: Color::Rgb(60, 48, 22),

            // AskUserQuestion pulse (blue/purple)
            pulse_ask_dim: Color::Rgb(50, 50, 140),
            pulse_ask_bright: Color::Rgb(100, 120, 230),
            pulse_ask_row_dim: Color::Rgb(25, 25, 50),
            pulse_ask_row_bright: Color::Rgb(40, 40, 75),
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
            sidebar_row_bg_running: Some(Color::Rgb(215, 240, 215)),
            sidebar_row_bg_waiting: Some(Color::Rgb(245, 232, 195)),
            sidebar_row_bg_completed: Some(Color::Rgb(205, 235, 235)),
            sidebar_row_bg_errored: Some(Color::Rgb(250, 210, 210)),
            status_symbol_bg_running: Some(Color::Rgb(140, 200, 140)),
            status_symbol_bg_waiting: Some(Color::Rgb(220, 180, 60)),
            status_symbol_bg_completed_unread: Some(Color::Rgb(120, 200, 200)),
            status_symbol_bg_errored: Some(Color::Rgb(220, 100, 100)),

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

            // Pulse animation
            pulse_waiting_dim: Color::Rgb(200, 160, 40),
            pulse_waiting_bright: Color::Rgb(255, 210, 60),
            pulse_waiting_row_dim: Color::Rgb(240, 228, 190),
            pulse_waiting_row_bright: Color::Rgb(255, 240, 200),

            // AskUserQuestion pulse (blue/purple)
            pulse_ask_dim: Color::Rgb(120, 120, 200),
            pulse_ask_bright: Color::Rgb(80, 80, 220),
            pulse_ask_row_dim: Color::Rgb(220, 220, 245),
            pulse_ask_row_bright: Color::Rgb(200, 200, 240),
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
            sidebar_row_bg_running: Some(Color::Rgb(38, 50, 33)),
            sidebar_row_bg_waiting: Some(Color::Rgb(55, 47, 22)),
            sidebar_row_bg_completed: Some(Color::Rgb(30, 50, 48)),
            sidebar_row_bg_errored: Some(Color::Rgb(55, 27, 24)),
            status_symbol_bg_running: Some(Color::Rgb(60, 90, 45)),
            status_symbol_bg_waiting: Some(Color::Rgb(140, 100, 20)),
            status_symbol_bg_completed_unread: Some(Color::Rgb(40, 100, 90)),
            status_symbol_bg_errored: Some(Color::Rgb(140, 30, 25)),

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

            // Pulse animation
            pulse_waiting_dim: Color::Rgb(120, 85, 15),
            pulse_waiting_bright: Color::Rgb(215, 153, 33),
            pulse_waiting_row_dim: Color::Rgb(48, 40, 18),
            pulse_waiting_row_bright: Color::Rgb(65, 55, 28),

            // AskUserQuestion pulse (blue teal, gruvbox-friendly)
            pulse_ask_dim: Color::Rgb(40, 80, 85),
            pulse_ask_bright: Color::Rgb(69, 133, 136),
            pulse_ask_row_dim: Color::Rgb(30, 45, 48),
            pulse_ask_row_bright: Color::Rgb(40, 60, 65),
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

    /// Look up the row background tint color for a given agent state key.
    ///
    /// Returns `Some(Color)` for states that have a tint (running, waiting,
    /// completed, errored) and `None` for neutral states (spawning, idle).
    pub fn sidebar_row_state_bg(&self, color_key: &str) -> Option<Color> {
        match color_key {
            "running" => self.sidebar_row_bg_running,
            "waiting" => self.sidebar_row_bg_waiting,
            "completed" => self.sidebar_row_bg_completed,
            "errored" => self.sidebar_row_bg_errored,
            _ => None,
        }
    }

    /// Look up the status symbol background color for a given state.
    ///
    /// Returns `Some(Color)` for active/attention states, `None` for neutral.
    /// The `has_unread_result` flag differentiates completed-unread from
    /// completed-read (only unread completions get a background).
    pub fn status_symbol_bg(&self, color_key: &str, has_unread_result: bool) -> Option<Color> {
        match color_key {
            "running" => self.status_symbol_bg_running,
            "waiting" => self.status_symbol_bg_waiting,
            "completed" if has_unread_result => self.status_symbol_bg_completed_unread,
            "errored" => self.status_symbol_bg_errored,
            _ => None,
        }
    }

    /// Compute the interpolated pulse color for the waiting symbol background.
    ///
    /// `phase` is 0..7. Phases 0-3 ramp from dim to bright, 4-7 from bright to dim.
    pub fn pulse_waiting_symbol_color(&self, phase: u8) -> Color {
        lerp_color(self.pulse_waiting_dim, self.pulse_waiting_bright, phase)
    }

    /// Compute the interpolated pulse color for the waiting row background.
    pub fn pulse_waiting_row_color(&self, phase: u8) -> Color {
        lerp_color(self.pulse_waiting_row_dim, self.pulse_waiting_row_bright, phase)
    }

    /// Compute the interpolated pulse color for the AskUserQuestion symbol background.
    pub fn pulse_ask_symbol_color(&self, phase: u8) -> Color {
        lerp_color(self.pulse_ask_dim, self.pulse_ask_bright, phase)
    }

    /// Compute the interpolated pulse color for the AskUserQuestion row background.
    pub fn pulse_ask_row_color(&self, phase: u8) -> Color {
        lerp_color(self.pulse_ask_row_dim, self.pulse_ask_row_bright, phase)
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

/// Linearly interpolate between two RGB colors based on pulse phase (0..7).
///
/// Uses a triangle wave: phases 0-3 ramp dim to bright, phases 4-7 ramp bright to dim.
fn lerp_color(dim: Color, bright: Color, phase: u8) -> Color {
    let t = match phase % 8 {
        0 | 7 => 0.0f32,
        1 | 6 => 0.33,
        2 | 5 => 0.67,
        3 | 4 => 1.0,
        _ => 0.0,
    };

    match (dim, bright) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
            let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
            let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;
            Color::Rgb(r, g, b)
        }
        _ => bright, // Fallback: if colors are not RGB, use bright
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
    fn sidebar_row_state_bg_returns_tints_for_notable_states() {
        let theme = Theme::default_dark();
        assert!(theme.sidebar_row_state_bg("running").is_some());
        assert!(theme.sidebar_row_state_bg("waiting").is_some());
        assert!(theme.sidebar_row_state_bg("completed").is_some());
        assert!(theme.sidebar_row_state_bg("errored").is_some());
        assert!(theme.sidebar_row_state_bg("idle").is_none());
        assert!(theme.sidebar_row_state_bg("spawning").is_none());
        assert!(theme.sidebar_row_state_bg("unknown").is_none());
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

    #[test]
    fn status_symbol_bg_returns_colors_for_active_states() {
        let theme = Theme::default_dark();
        assert!(theme.status_symbol_bg("running", false).is_some());
        assert!(theme.status_symbol_bg("waiting", false).is_some());
        assert!(theme.status_symbol_bg("errored", false).is_some());
    }

    #[test]
    fn status_symbol_bg_completed_requires_unread() {
        let theme = Theme::default_dark();
        assert!(theme.status_symbol_bg("completed", true).is_some());
        assert!(theme.status_symbol_bg("completed", false).is_none());
    }

    #[test]
    fn status_symbol_bg_neutral_states_return_none() {
        let theme = Theme::default_dark();
        assert!(theme.status_symbol_bg("spawning", false).is_none());
        assert!(theme.status_symbol_bg("idle", false).is_none());
        assert!(theme.status_symbol_bg("unknown", false).is_none());
    }

    #[test]
    fn lerp_color_phase_0_returns_dim() {
        let dim = Color::Rgb(100, 100, 100);
        let bright = Color::Rgb(200, 200, 200);
        assert_eq!(lerp_color(dim, bright, 0), Color::Rgb(100, 100, 100));
    }

    #[test]
    fn lerp_color_phase_3_returns_bright() {
        let dim = Color::Rgb(100, 100, 100);
        let bright = Color::Rgb(200, 200, 200);
        assert_eq!(lerp_color(dim, bright, 3), Color::Rgb(200, 200, 200));
    }

    #[test]
    fn lerp_color_phase_4_returns_bright() {
        let dim = Color::Rgb(100, 100, 100);
        let bright = Color::Rgb(200, 200, 200);
        assert_eq!(lerp_color(dim, bright, 4), Color::Rgb(200, 200, 200));
    }

    #[test]
    fn lerp_color_phase_7_returns_dim() {
        let dim = Color::Rgb(100, 100, 100);
        let bright = Color::Rgb(200, 200, 200);
        assert_eq!(lerp_color(dim, bright, 7), Color::Rgb(100, 100, 100));
    }

    #[test]
    fn lerp_color_symmetric_wave() {
        let dim = Color::Rgb(0, 0, 0);
        let bright = Color::Rgb(255, 255, 255);
        assert_eq!(lerp_color(dim, bright, 1), lerp_color(dim, bright, 6));
        assert_eq!(lerp_color(dim, bright, 2), lerp_color(dim, bright, 5));
    }

    #[test]
    fn pulse_waiting_colors_are_defined() {
        let theme = Theme::default_dark();
        assert!(matches!(theme.pulse_waiting_dim, Color::Rgb(..)));
        assert!(matches!(theme.pulse_waiting_bright, Color::Rgb(..)));
        assert_ne!(theme.pulse_waiting_dim, theme.pulse_waiting_bright);
    }
}
