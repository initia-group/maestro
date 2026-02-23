//! Configuration loader — file resolution and parsing.
//!
//! Implements the 3-level config resolution: CLI flag → environment
//! variable → XDG default path. Handles file reading, TOML parsing,
//! and validation.

use crate::config::settings::MaestroConfig;
use color_eyre::eyre::{bail, Result, WrapErr};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Expand `~` to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
        warn!("Could not determine home directory for tilde expansion");
    }
    path.to_path_buf()
}

/// Discover the config file path using the resolution order:
/// 1. Explicit CLI path
/// 2. `MAESTRO_CONFIG` env var
/// 3. `~/.config/maestro/config.toml`
pub fn discover_config_path(cli_path: Option<&Path>) -> Option<PathBuf> {
    // 1. CLI flag
    if let Some(path) = cli_path {
        return Some(expand_tilde(path));
    }

    // 2. Environment variable
    if let Ok(env_path) = std::env::var("MAESTRO_CONFIG") {
        let path = expand_tilde(Path::new(&env_path));
        if path.exists() {
            return Some(path);
        }
        warn!(
            "MAESTRO_CONFIG points to non-existent file: {}",
            path.display()
        );
    }

    // 3. XDG default
    if let Some(config_dir) = dirs::config_dir() {
        let path = config_dir.join("maestro").join("config.toml");
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Load and validate the configuration.
/// Returns defaults if no config file is found.
pub fn load_config(cli_path: Option<&Path>) -> Result<MaestroConfig> {
    match discover_config_path(cli_path) {
        Some(path) => {
            info!("Loading config from: {}", path.display());
            let content = std::fs::read_to_string(&path)
                .wrap_err_with(|| format!("Failed to read config file: {}", path.display()))?;
            let mut config: MaestroConfig = toml::from_str(&content)
                .wrap_err_with(|| format!("Failed to parse config file: {}", path.display()))?;
            expand_config_paths(&mut config);
            validate_config(&config)?;
            Ok(config)
        }
        None => {
            info!("No config file found, using defaults");
            Ok(MaestroConfig::default())
        }
    }
}

/// Apply tilde expansion to all path fields in the config.
fn expand_config_paths(config: &mut MaestroConfig) {
    config.global.log_dir = expand_tilde(&config.global.log_dir);

    for project in &mut config.project {
        project.path = expand_tilde(&project.path);
        for agent in &mut project.agent {
            if let Some(ref cwd) = agent.cwd {
                agent.cwd = Some(expand_tilde(cwd));
            }
        }
    }

    // Expand paths in profiles
    for profile in &mut config.profile {
        for project in &mut profile.project {
            project.path = expand_tilde(&project.path);
            for agent in &mut project.agent {
                if let Some(ref cwd) = agent.cwd {
                    agent.cwd = Some(expand_tilde(cwd));
                }
            }
        }
    }

    // Expand paths in templates
    for template in &mut config.template {
        if let Some(ref cwd) = template.cwd {
            template.cwd = Some(expand_tilde(cwd));
        }
    }
}

/// Validate configuration values for correctness.
pub fn validate_config(config: &MaestroConfig) -> Result<()> {
    // Global validations
    if config.global.max_agents == 0 {
        bail!("global.max_agents must be > 0");
    }
    if config.global.max_agents > 50 {
        warn!(
            "global.max_agents is set to {} — this may impact performance",
            config.global.max_agents
        );
    }
    if config.global.state_check_interval_ms < 50 {
        bail!("global.state_check_interval_ms must be >= 50");
    }

    // UI validations
    if config.ui.fps == 0 || config.ui.fps > 120 {
        bail!("ui.fps must be between 1 and 120");
    }
    if config.ui.sidebar_width < 15 || config.ui.sidebar_width > 60 {
        bail!("ui.sidebar_width must be between 15 and 60");
    }

    // Project validations
    let mut project_names = HashSet::new();
    for project in &config.project {
        if project.name.is_empty() {
            bail!("Project name must not be empty");
        }
        if !project_names.insert(&project.name) {
            bail!("Duplicate project name: '{}'", project.name);
        }
        // Agent name uniqueness within project
        let mut agent_names = HashSet::new();
        for agent in &project.agent {
            if agent.name.is_empty() {
                bail!(
                    "Agent name must not be empty in project '{}'",
                    project.name
                );
            }
            if !agent_names.insert(&agent.name) {
                bail!(
                    "Duplicate agent name '{}' in project '{}'",
                    agent.name,
                    project.name
                );
            }
        }
    }

    // Template name uniqueness
    let mut template_names = HashSet::new();
    for template in &config.template {
        if template.name.is_empty() {
            bail!("Template name must not be empty");
        }
        if !template_names.insert(&template.name) {
            bail!("Duplicate template name: '{}'", template.name);
        }
    }

    // Profile name uniqueness and validation
    let mut profile_names = HashSet::new();
    for profile in &config.profile {
        if profile.name.is_empty() {
            bail!("Profile name must not be empty");
        }
        if !profile_names.insert(&profile.name) {
            bail!("Duplicate profile name: '{}'", profile.name);
        }
        // Validate project and agent names within each profile
        let mut prof_project_names = HashSet::new();
        for project in &profile.project {
            if project.name.is_empty() {
                bail!(
                    "Project name must not be empty in profile '{}'",
                    profile.name
                );
            }
            if !prof_project_names.insert(&project.name) {
                bail!(
                    "Duplicate project name '{}' in profile '{}'",
                    project.name,
                    profile.name
                );
            }
            let mut prof_agent_names = HashSet::new();
            for agent in &project.agent {
                if agent.name.is_empty() {
                    bail!(
                        "Agent name must not be empty in project '{}' of profile '{}'",
                        project.name,
                        profile.name
                    );
                }
                if !prof_agent_names.insert(&agent.name) {
                    bail!(
                        "Duplicate agent name '{}' in project '{}' of profile '{}'",
                        agent.name,
                        project.name,
                        profile.name
                    );
                }
            }
        }
    }

    // Validate active_profile references a defined profile (if set)
    if let Some(ref active) = config.active_profile {
        if !config.profile.iter().any(|p| p.name == *active) {
            let available: Vec<&str> = config.profile.iter().map(|p| p.name.as_str()).collect();
            bail!(
                "Active profile '{}' not found. Available: {}",
                active,
                if available.is_empty() {
                    "(none defined)".to_string()
                } else {
                    available.join(", ")
                }
            );
        }
    }

    // Detection pattern validation — compile regex to catch errors early
    for pattern in &config.detection.tool_approval_patterns {
        regex::Regex::new(pattern)
            .wrap_err_with(|| format!("Invalid tool_approval_pattern regex: '{pattern}'"))?;
    }
    for pattern in &config.detection.error_patterns {
        regex::Regex::new(pattern)
            .wrap_err_with(|| format!("Invalid error_pattern regex: '{pattern}'"))?;
    }
    for pattern in &config.detection.input_prompt_patterns {
        regex::Regex::new(pattern)
            .wrap_err_with(|| format!("Invalid input_prompt_pattern regex: '{pattern}'"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::ProjectConfig;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_valid_config_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
            [global]
            max_agents = 10

            [[project]]
            name = "test"
            path = "/tmp/test"
        "#
        )
        .unwrap();

        let config = load_config(Some(file.path())).unwrap();
        assert_eq!(config.global.max_agents, 10);
        assert_eq!(config.project.len(), 1);
        assert_eq!(config.project[0].name, "test");
    }

    #[test]
    fn test_load_nonexistent_cli_path_errors() {
        let result = load_config(Some(Path::new("/nonexistent/config.toml")));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_no_config_returns_defaults() {
        // With no CLI path and no env var and no XDG file, returns defaults.
        // We can't fully control the env in a unit test, but we can at least
        // verify that load_config(None) doesn't panic.
        let config = load_config(None);
        // This may succeed (defaults) or fail (if XDG config exists and is invalid),
        // but it should not panic.
        if let Ok(config) = config {
            assert_eq!(config.global.max_agents, 15);
        }
    }

    #[test]
    fn test_validate_rejects_zero_max_agents() {
        let mut config = MaestroConfig::default();
        config.global.max_agents = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_low_state_check_interval() {
        let mut config = MaestroConfig::default();
        config.global.state_check_interval_ms = 10;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_zero_fps() {
        let mut config = MaestroConfig::default();
        config.ui.fps = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_high_fps() {
        let mut config = MaestroConfig::default();
        config.ui.fps = 121;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_narrow_sidebar() {
        let mut config = MaestroConfig::default();
        config.ui.sidebar_width = 10;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_wide_sidebar() {
        let mut config = MaestroConfig::default();
        config.ui.sidebar_width = 65;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_duplicate_project_names() {
        let mut config = MaestroConfig::default();
        config.project = vec![
            ProjectConfig {
                name: "dup".into(),
                path: "/a".into(),
                agent: vec![],
            },
            ProjectConfig {
                name: "dup".into(),
                path: "/b".into(),
                agent: vec![],
            },
        ];
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_empty_project_name() {
        let mut config = MaestroConfig::default();
        config.project = vec![ProjectConfig {
            name: "".into(),
            path: "/a".into(),
            agent: vec![],
        }];
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_duplicate_template_names() {
        let mut config = MaestroConfig::default();
        config.template = vec![
            crate::config::settings::TemplateConfig {
                name: "dup".into(),
                command: "cmd".into(),
                args: vec![],
                description: None,
                default_project: None,
                env: std::collections::HashMap::new(),
                cwd: None,
                mode: Default::default(),
            },
            crate::config::settings::TemplateConfig {
                name: "dup".into(),
                command: "cmd".into(),
                args: vec![],
                description: None,
                default_project: None,
                env: std::collections::HashMap::new(),
                cwd: None,
                mode: Default::default(),
            },
        ];
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_invalid_regex() {
        let mut config = MaestroConfig::default();
        config.detection.tool_approval_patterns = vec!["[invalid".into()];
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_invalid_error_regex() {
        let mut config = MaestroConfig::default();
        config.detection.error_patterns = vec!["(unclosed".into()];
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_invalid_input_prompt_regex() {
        let mut config = MaestroConfig::default();
        config.detection.input_prompt_patterns = vec!["*bad".into()];
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_accepts_valid_config() {
        let config = MaestroConfig::default();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde(Path::new("~/test/path"));
        assert!(!expanded.starts_with("~"));
        assert!(expanded.ends_with("test/path"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let path = Path::new("/absolute/path");
        let expanded = expand_tilde(path);
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_tilde_just_tilde() {
        let expanded = expand_tilde(Path::new("~"));
        assert!(!expanded.to_string_lossy().contains('~'));
    }

    #[test]
    fn test_config_path_expansion_on_load() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
            [global]
            log_dir = "~/maestro-logs"

            [[project]]
            name = "test"
            path = "~/projects/test"
        "#
        )
        .unwrap();

        let config = load_config(Some(file.path())).unwrap();
        assert!(!config.global.log_dir.starts_with("~"));
        assert!(!config.project[0].path.starts_with("~"));
    }

    #[test]
    fn test_default_toml_roundtrip() {
        let toml_str = r#"
[global]
claude_binary = "claude"
default_shell = "/bin/zsh"
max_agents = 15
log_dir = "~/.local/share/maestro/logs"
state_check_interval_ms = 250
idle_timeout_secs = 3

[ui]
fps = 30
sidebar_width = 28
default_layout = "single"
show_uptime = true
mouse_enabled = true

[ui.theme]
name = "default"

[notifications]
enabled = true
cooldown_secs = 10
notify_on_input_prompt = false

[session]
enabled = true
autosave_interval_secs = 60
max_scrollback_bytes = 5242880
"#;
        let config: MaestroConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.global.max_agents, 15);
        assert_eq!(config.ui.fps, 30);
        assert!(config.notifications.enabled);
        assert!(config.session.enabled);
    }
}
