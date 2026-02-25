//! Configuration structs — deserialization targets.
//!
//! Defines `MaestroConfig`, `GlobalConfig`, `UiConfig`, and all
//! nested configuration types that map to the TOML config file.

use crate::agent::RestartPolicy;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level Maestro configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaestroConfig {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub project: Vec<ProjectConfig>,
    #[serde(default)]
    pub template: Vec<TemplateConfig>,
    #[serde(default)]
    pub profile: Vec<ProfileConfig>,
    /// Active profile name (None = use top-level project definitions).
    #[serde(default)]
    pub active_profile: Option<String>,
    #[serde(default)]
    pub detection: DetectionConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub session: SessionConfig,
}

impl MaestroConfig {
    /// Reload config from disk and return a fresh instance.
    /// Does NOT apply changes — caller decides what to update.
    pub fn reload(cli_path: Option<&std::path::Path>) -> color_eyre::eyre::Result<MaestroConfig> {
        super::loader::load_config(cli_path)
    }
}

/// Global runtime settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    /// Path to the `claude` CLI binary. If not absolute, resolved via PATH.
    pub claude_binary: String,
    /// Default shell for PTY sessions.
    pub default_shell: String,
    /// Maximum number of concurrent agents.
    pub max_agents: usize,
    /// Log directory path. Supports `~` expansion.
    pub log_dir: PathBuf,
    /// State detection check interval in milliseconds.
    pub state_check_interval_ms: u64,
    /// Seconds of silence before marking an agent as Idle.
    pub idle_timeout_secs: u64,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            claude_binary: "claude".into(),
            default_shell: "/bin/zsh".into(),
            max_agents: 15,
            log_dir: "~/.local/share/maestro/logs".into(),
            state_check_interval_ms: 250,
            idle_timeout_secs: 3,
        }
    }
}

/// UI appearance and behavior settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Target frames per second for rendering.
    pub fps: u32,
    /// Sidebar width in terminal columns.
    pub sidebar_width: u16,
    /// Default layout mode.
    pub default_layout: LayoutMode,
    /// Whether to show agent uptime in the sidebar.
    pub show_uptime: bool,
    /// Whether mouse input is enabled.
    pub mouse_enabled: bool,
    /// Theme configuration.
    pub theme: ThemeConfig,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            fps: 30,
            sidebar_width: 28,
            default_layout: LayoutMode::Single,
            show_uptime: true,
            mouse_enabled: true,
            theme: ThemeConfig::default(),
        }
    }
}

/// Layout mode for the main panel.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutMode {
    Single,
    SplitH,
    SplitV,
    Grid,
}

/// Theme selection.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Built-in theme name: "default", "dark", "light", "gruvbox".
    pub name: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
        }
    }
}

/// A project definition — a directory containing one or more agents.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    /// Display name for the project.
    pub name: String,
    /// Absolute path to the project directory.
    pub path: PathBuf,
    /// Agents defined under this project.
    #[serde(default)]
    pub agent: Vec<AgentConfig>,
}

/// Agent mode — how the agent interacts with its process.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentModeConfig {
    /// Full interactive PTY mode (default).
    #[default]
    Interactive,
    /// Stream-JSON mode for background/autonomous agents.
    StreamJson,
}

/// An agent definition within a project.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// Display name for the agent.
    pub name: String,
    /// Command to run (default: value of global.claude_binary).
    pub command: Option<String>,
    /// Additional CLI arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Whether to start this agent automatically on Maestro launch.
    #[serde(default)]
    pub auto_start: bool,
    /// Optional override for the working directory (default: project path).
    pub cwd: Option<PathBuf>,
    /// Optional environment variables to set.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Agent mode: "interactive" (default) or "stream-json".
    #[serde(default)]
    pub mode: AgentModeConfig,
    /// Auto-restart policy for this agent.
    #[serde(default, flatten)]
    pub restart_policy: RestartPolicy,
}

/// A reusable agent template for the command palette.
#[derive(Debug, Clone, Deserialize)]
pub struct TemplateConfig {
    /// Template name (used in command palette).
    pub name: String,
    /// Command to run.
    pub command: String,
    /// CLI arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Default project for this template (can be overridden at spawn time).
    pub default_project: Option<String>,
    /// Environment variables set for agents from this template.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Working directory override.
    pub cwd: Option<PathBuf>,
    /// Agent mode: "interactive" (default) or "stream-json".
    #[serde(default)]
    pub mode: AgentModeConfig,
}

/// A workspace profile — a named set of projects and auto-start agents.
///
/// Switching profiles kills all current agents and spawns the new
/// profile's agents. Each profile has its own set of projects.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileConfig {
    /// Profile name (e.g., "dev", "review", "deploy").
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Projects and agents in this profile.
    #[serde(default)]
    pub project: Vec<ProjectConfig>,
}

/// State detection pattern configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DetectionConfig {
    /// Additional regex patterns for tool approval detection.
    pub tool_approval_patterns: Vec<String>,
    /// Additional regex patterns for error detection.
    pub error_patterns: Vec<String>,
    /// Additional regex patterns for input prompt detection.
    pub input_prompt_patterns: Vec<String>,
    /// Additional regex patterns for AskUserQuestion detection.
    pub ask_user_question_patterns: Vec<String>,
    /// Number of bottom screen lines to scan for patterns.
    pub scan_lines: usize,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            tool_approval_patterns: vec![],
            error_patterns: vec![],
            input_prompt_patterns: vec![],
            ask_user_question_patterns: vec![],
            scan_lines: 10,
        }
    }
}

/// Notification settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    /// Whether desktop notifications are enabled.
    pub enabled: bool,
    /// Minimum seconds between notifications for the same agent.
    pub cooldown_secs: u64,
    /// Whether to notify on input prompts.
    pub notify_on_input_prompt: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cooldown_secs: 10,
            notify_on_input_prompt: false,
        }
    }
}

/// Session persistence settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    /// Whether session persistence is enabled.
    pub enabled: bool,
    /// Autosave interval in seconds.
    pub autosave_interval_secs: u64,
    /// Maximum scrollback buffer size in bytes.
    pub max_scrollback_bytes: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            autosave_interval_secs: 60,
            max_scrollback_bytes: 5_242_880, // 5 MiB
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = MaestroConfig::default();
        assert_eq!(config.global.max_agents, 15);
        assert_eq!(config.global.claude_binary, "claude");
        assert_eq!(config.global.default_shell, "/bin/zsh");
        assert_eq!(config.global.state_check_interval_ms, 250);
        assert_eq!(config.global.idle_timeout_secs, 3);
        assert_eq!(config.ui.fps, 30);
        assert_eq!(config.ui.sidebar_width, 28);
        assert_eq!(config.ui.default_layout, LayoutMode::Single);
        assert!(config.ui.show_uptime);
        assert!(config.ui.mouse_enabled);
        assert_eq!(config.ui.theme.name, "default");
        assert!(config.project.is_empty());
        assert!(config.template.is_empty());
        assert_eq!(config.detection.scan_lines, 10);
        assert!(config.notifications.enabled);
        assert_eq!(config.notifications.cooldown_secs, 10);
        assert!(config.session.enabled);
        assert_eq!(config.session.autosave_interval_secs, 60);
    }

    #[test]
    fn test_deserialize_minimal_toml() {
        let toml_str = r#"
            [[project]]
            name = "myapp"
            path = "/tmp/myapp"
        "#;
        let config: MaestroConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project.len(), 1);
        assert_eq!(config.project[0].name, "myapp");
        // Defaults should be applied
        assert_eq!(config.global.max_agents, 15);
        assert_eq!(config.ui.fps, 30);
    }

    #[test]
    fn test_deserialize_full_toml() {
        let toml_str = r#"
            [global]
            claude_binary = "/usr/local/bin/claude"
            default_shell = "/bin/bash"
            max_agents = 10
            log_dir = "/tmp/maestro/logs"
            state_check_interval_ms = 500
            idle_timeout_secs = 5

            [ui]
            fps = 60
            sidebar_width = 32
            default_layout = "split-h"
            show_uptime = false
            mouse_enabled = false

            [ui.theme]
            name = "gruvbox"

            [[project]]
            name = "web-app"
            path = "/home/user/web-app"

            [[project.agent]]
            name = "frontend"
            command = "claude"
            args = ["--model", "opus"]
            auto_start = true

            [[project.agent]]
            name = "backend"
            auto_start = false

            [[template]]
            name = "code-review"
            command = "claude"
            args = ["--review"]
            description = "Run a code review"

            [detection]
            tool_approval_patterns = ["approve\\?"]
            error_patterns = ["ERROR:"]
            input_prompt_patterns = [">>>"]
            scan_lines = 10

            [notifications]
            enabled = false
            cooldown_secs = 30
            notify_on_input_prompt = true

            [session]
            enabled = false
            autosave_interval_secs = 120
            max_scrollback_bytes = 10485760
        "#;
        let config: MaestroConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.global.claude_binary, "/usr/local/bin/claude");
        assert_eq!(config.global.max_agents, 10);
        assert_eq!(config.ui.fps, 60);
        assert_eq!(config.ui.sidebar_width, 32);
        assert_eq!(config.ui.default_layout, LayoutMode::SplitH);
        assert!(!config.ui.show_uptime);
        assert!(!config.ui.mouse_enabled);
        assert_eq!(config.ui.theme.name, "gruvbox");
        assert_eq!(config.project.len(), 1);
        assert_eq!(config.project[0].agent.len(), 2);
        assert_eq!(config.project[0].agent[0].name, "frontend");
        assert!(config.project[0].agent[0].auto_start);
        assert_eq!(config.template.len(), 1);
        assert_eq!(config.template[0].name, "code-review");
        assert_eq!(config.detection.scan_lines, 10);
        assert!(!config.notifications.enabled);
        assert_eq!(config.notifications.cooldown_secs, 30);
        assert!(!config.session.enabled);
        assert_eq!(config.session.max_scrollback_bytes, 10_485_760);
    }

    #[test]
    fn test_unknown_field_rejected() {
        let toml_str = r#"
            unknown_field = "oops"
        "#;
        let result: Result<MaestroConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_layout_mode_kebab_case() {
        let toml_str = r#"
            [ui]
            default_layout = "split-h"
        "#;
        let config: MaestroConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ui.default_layout, LayoutMode::SplitH);

        let toml_str = r#"
            [ui]
            default_layout = "split-v"
        "#;
        let config: MaestroConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ui.default_layout, LayoutMode::SplitV);

        let toml_str = r#"
            [ui]
            default_layout = "grid"
        "#;
        let config: MaestroConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ui.default_layout, LayoutMode::Grid);
    }

    #[test]
    fn test_agent_with_env_vars() {
        let toml_str = r#"
            [[project]]
            name = "test"
            path = "/tmp/test"

            [[project.agent]]
            name = "agent1"
            env = { RUST_LOG = "debug", MY_VAR = "value" }
        "#;
        let config: MaestroConfig = toml::from_str(toml_str).unwrap();
        let agent = &config.project[0].agent[0];
        assert_eq!(agent.env.get("RUST_LOG").unwrap(), "debug");
        assert_eq!(agent.env.get("MY_VAR").unwrap(), "value");
    }

    #[test]
    fn test_empty_toml_uses_defaults() {
        let config: MaestroConfig = toml::from_str("").unwrap();
        assert_eq!(config.global.max_agents, 15);
        assert_eq!(config.ui.fps, 30);
        assert!(config.project.is_empty());
    }
}
