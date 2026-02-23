# Feature 02: Configuration System

## Overview

Implement the TOML-based configuration system that loads, validates, and provides settings to every other component. The config system supports a layered approach: built-in defaults → user config file → CLI overrides. This feature defines all configuration structs, file discovery logic, validation rules, and default values.

## Dependencies

- **Feature 01** (Project Scaffold) — module structure and Cargo.toml must exist.

## Technical Specification

### Config File Location

Resolution order (first found wins):
1. CLI flag: `--config <path>`
2. Environment variable: `MAESTRO_CONFIG=<path>`
3. XDG-compliant default: `~/.config/maestro/config.toml` (via `dirs::config_dir()`)

If no config file is found, Maestro starts with built-in defaults and logs a warning.

### Configuration Structs

All structs derive `serde::Deserialize` and `Clone`. Fields use `Option<T>` where the user might omit them (layering with defaults).

#### `src/config/settings.rs`

```rust
use serde::Deserialize;
use std::path::PathBuf;

/// Top-level Maestro configuration.
#[derive(Debug, Clone, Deserialize)]
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
    pub detection: DetectionConfig,
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
    pub env: std::collections::HashMap<String, String>,
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
    /// Number of bottom screen lines to scan for patterns.
    pub scan_lines: usize,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            tool_approval_patterns: vec![],
            error_patterns: vec![],
            input_prompt_patterns: vec![],
            scan_lines: 5,
        }
    }
}
```

### Config Loader (`src/config/loader.rs`)

```rust
use crate::config::settings::MaestroConfig;
use color_eyre::eyre::{Result, WrapErr, bail};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Discover the config file path using the resolution order:
/// 1. Explicit CLI path
/// 2. MAESTRO_CONFIG env var
/// 3. ~/.config/maestro/config.toml
pub fn discover_config_path(cli_path: Option<&Path>) -> Option<PathBuf> {
    // 1. CLI flag
    if let Some(path) = cli_path {
        return Some(path.to_path_buf());
    }

    // 2. Environment variable
    if let Ok(env_path) = std::env::var("MAESTRO_CONFIG") {
        let path = PathBuf::from(env_path);
        if path.exists() {
            return Some(path);
        }
        warn!("MAESTRO_CONFIG points to non-existent file: {}", path.display());
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
            let config: MaestroConfig = toml::from_str(&content)
                .wrap_err_with(|| format!("Failed to parse config file: {}", path.display()))?;
            validate_config(&config)?;
            Ok(config)
        }
        None => {
            info!("No config file found, using defaults");
            Ok(MaestroConfig::default())
        }
    }
}

/// Validate configuration values for correctness.
fn validate_config(config: &MaestroConfig) -> Result<()> {
    // Global validations
    if config.global.max_agents == 0 {
        bail!("global.max_agents must be > 0");
    }
    if config.global.max_agents > 50 {
        warn!("global.max_agents is set to {} — this may impact performance", config.global.max_agents);
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
    let mut project_names = std::collections::HashSet::new();
    for project in &config.project {
        if !project_names.insert(&project.name) {
            bail!("Duplicate project name: '{}'", project.name);
        }
        // Agent name uniqueness within project
        let mut agent_names = std::collections::HashSet::new();
        for agent in &project.agent {
            if !agent_names.insert(&agent.name) {
                bail!("Duplicate agent name '{}' in project '{}'", agent.name, project.name);
            }
        }
    }

    // Template name uniqueness
    let mut template_names = std::collections::HashSet::new();
    for template in &config.template {
        if !template_names.insert(&template.name) {
            bail!("Duplicate template name: '{}'", template.name);
        }
    }

    // Detection pattern validation — compile regex to catch errors early
    for pattern in &config.detection.tool_approval_patterns {
        regex::Regex::new(pattern)
            .wrap_err_with(|| format!("Invalid tool_approval_pattern regex: '{}'", pattern))?;
    }
    for pattern in &config.detection.error_patterns {
        regex::Regex::new(pattern)
            .wrap_err_with(|| format!("Invalid error_pattern regex: '{}'", pattern))?;
    }

    Ok(())
}
```

> **Note**: Add `regex = "1"` to `Cargo.toml` dependencies for pattern validation.

### Path Expansion

The `log_dir` and project `path` fields may contain `~`. Implement a helper:

```rust
/// Expand `~` to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}
```

This should be applied during config loading (after deserialization) to all path fields.

### Default Implementation for MaestroConfig

```rust
impl Default for MaestroConfig {
    fn default() -> Self {
        Self {
            global: GlobalConfig::default(),
            ui: UiConfig::default(),
            project: vec![],
            template: vec![],
            detection: DetectionConfig::default(),
        }
    }
}
```

### Config Reload Support (for v0.2)

The config system should be designed to support hot-reload from the start:

```rust
impl MaestroConfig {
    /// Reload config from disk and return the diff.
    /// Does NOT apply changes — caller decides what to update.
    pub fn reload(cli_path: Option<&Path>) -> Result<MaestroConfig> {
        load_config(cli_path)
    }
}
```

For v0.1, this method exists but isn't called from the event loop. The config is loaded once at startup. The important thing is that the config struct is `Clone` and can be diffed against a new version later.

## Implementation Steps

1. **Add `regex` dependency** to `Cargo.toml`:
   ```toml
   regex = "1"
   ```

2. **Implement `src/config/settings.rs`**
   - Define all structs as specified above.
   - Implement `Default` for all structs that have sensible defaults.
   - Add `#[serde(deny_unknown_fields)]` on `MaestroConfig` to catch typos in config files.

3. **Implement `src/config/loader.rs`**
   - `discover_config_path()` with the 3-level resolution order.
   - `load_config()` that reads, parses, and validates.
   - `validate_config()` with all validation rules.
   - `expand_tilde()` helper.

4. **Implement `src/config/mod.rs`**
   - Re-export `MaestroConfig`, `load_config`, and key types.

5. **Update `config/default.toml`**
   - Ensure it matches the struct defaults.
   - Add comments explaining each field.

6. **Wire into `main.rs`**
   - Call `load_config(cli.config.as_deref())` at startup.
   - Print loaded config summary at `tracing::debug` level.

7. **Test roundtrip**
   - Write a test TOML file → load it → assert struct values match.

## Error Handling

| Error | How it's handled |
|---|---|
| Config file not found | Use defaults, log warning. Not an error. |
| TOML parse error | Return `color_eyre::Result` with file path context and line number. |
| Invalid field value | `validate_config()` returns descriptive error with field name. |
| Invalid regex pattern | Caught during validation with the pattern string in the error message. |
| Duplicate names | Caught during validation. |
| `~` expansion fails (no home dir) | Fall back to literal path, log warning. |
| File permission error | Propagated with `wrap_err` context. |

## Testing Strategy

### Unit Tests (`src/config/settings.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = MaestroConfig::default();
        // Should not panic
        assert_eq!(config.global.max_agents, 15);
        assert_eq!(config.ui.fps, 30);
        assert_eq!(config.ui.default_layout, LayoutMode::Single);
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
    }

    #[test]
    fn test_deserialize_full_toml() {
        // Test with every field specified — use the full example from PLAN.md
    }

    #[test]
    fn test_unknown_field_rejected() {
        let toml_str = r#"
            [global]
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
    }
}
```

### Unit Tests (`src/config/loader.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_load_valid_config_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, r#"
            [global]
            max_agents = 10

            [[project]]
            name = "test"
            path = "/tmp/test"
        "#).unwrap();

        let config = load_config(Some(file.path())).unwrap();
        assert_eq!(config.global.max_agents, 10);
    }

    #[test]
    fn test_load_nonexistent_file_returns_defaults() {
        let config = load_config(Some(Path::new("/nonexistent/config.toml")));
        // This should error since explicit path doesn't exist
        assert!(config.is_err());
    }

    #[test]
    fn test_load_no_config_returns_defaults() {
        // With no CLI path and no env var and no XDG file
        let config = load_config(None).unwrap();
        assert_eq!(config.global.max_agents, 15);
    }

    #[test]
    fn test_validate_rejects_zero_max_agents() {
        let mut config = MaestroConfig::default();
        config.global.max_agents = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_rejects_duplicate_project_names() {
        let mut config = MaestroConfig::default();
        config.project = vec![
            ProjectConfig { name: "dup".into(), path: "/a".into(), agent: vec![] },
            ProjectConfig { name: "dup".into(), path: "/b".into(), agent: vec![] },
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
    fn test_expand_tilde() {
        let expanded = expand_tilde(Path::new("~/test"));
        assert!(!expanded.starts_with("~"));
        assert!(expanded.ends_with("test"));
    }
}
```

### Integration Tests

- Create a temp directory, write a config file, verify `load_config()` reads it correctly.
- Test `MAESTRO_CONFIG` env var resolution (set env var in test, verify it's picked up).

## Acceptance Criteria

- [ ] `MaestroConfig` and all sub-structs compile and derive `Deserialize`, `Clone`, `Debug`.
- [ ] Defaults are correct and match `config/default.toml`.
- [ ] Config file discovery works for all 3 resolution levels (CLI, env var, XDG).
- [ ] Missing config file gracefully returns defaults.
- [ ] Invalid TOML produces a human-readable error with file path and context.
- [ ] Validation catches: zero max_agents, out-of-range FPS, duplicate names, invalid regex.
- [ ] `#[serde(deny_unknown_fields)]` rejects typos in config keys.
- [ ] `~` expansion works for path fields.
- [ ] All unit tests pass.
- [ ] The full example config from PLAN.md deserializes correctly.
