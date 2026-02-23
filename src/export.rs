//! Agent output export — export terminal output to Markdown files.
//!
//! Provides `OutputExporter` for exporting an agent's conversation to a
//! Markdown file, supporting both screen-content and raw-scrollback modes.
//! See Feature 20 (Output Export & Stream-JSON) for the full spec.

use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Configuration for the export subsystem.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    /// Directory for exported files (supports `~`).
    pub output_dir: PathBuf,
    /// Automatically export when an agent completes.
    pub auto_export_on_complete: bool,
    /// Parser column count for scrollback re-parsing.
    pub export_cols: u16,
    /// Parser row count for scrollback re-parsing.
    pub export_rows: u16,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("~/.local/share/maestro/exports"),
            auto_export_on_complete: false,
            export_cols: 120,
            export_rows: 50,
        }
    }
}

/// Handles exporting agent output to files.
pub struct OutputExporter;

impl OutputExporter {
    /// Export an agent's output to a Markdown file.
    ///
    /// Generates a Markdown document containing the agent name, project,
    /// start time, duration, status, and the full terminal output text.
    /// The file is written to `output_path` with a timestamped filename.
    pub fn export_to_markdown(
        agent_name: &str,
        project_name: &str,
        started_at: DateTime<Utc>,
        state_label: &str,
        screen_contents: &str,
        output_path: &Path,
    ) -> Result<PathBuf> {
        let duration = Utc::now() - started_at;
        let duration_str = if duration.num_hours() > 0 {
            format!(
                "{}h {}m",
                duration.num_hours(),
                duration.num_minutes() % 60
            )
        } else {
            format!("{}m", duration.num_minutes())
        };

        let markdown = format!(
            "# Agent: {} @ {}\n\
             **Started:** {}\n\
             **Duration:** {}\n\
             **Status:** {}\n\
             \n\
             ---\n\
             \n\
             ## Terminal Output\n\
             \n\
             ```\n\
             {}\n\
             ```\n",
            agent_name,
            project_name,
            started_at.format("%Y-%m-%d %H:%M:%S UTC"),
            duration_str,
            state_label,
            screen_contents,
        );

        // Generate filename
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let sanitized_label = state_label.to_lowercase().replace(' ', "_");
        let filename = format!(
            "{}_{}_{}_{}.md",
            project_name, agent_name, sanitized_label, timestamp,
        );
        let file_path = output_path.join(filename);

        std::fs::create_dir_all(output_path)?;
        std::fs::write(&file_path, markdown)?;

        tracing::info!("Exported agent output to {}", file_path.display());
        Ok(file_path)
    }

    /// Export using the scrollback buffer for complete history.
    ///
    /// Re-parses raw PTY bytes through a fresh vt100 parser to extract
    /// clean text, then delegates to `export_to_markdown`.
    pub fn export_from_scrollback(
        agent_name: &str,
        project_name: &str,
        started_at: DateTime<Utc>,
        state_label: &str,
        raw_bytes: &[u8],
        output_path: &Path,
        config: &ExportConfig,
    ) -> Result<PathBuf> {
        // Re-parse the raw bytes through a fresh vt100 parser to get clean text
        let mut parser = vt100::Parser::new(config.export_rows, config.export_cols, 0);
        parser.process(raw_bytes);
        let screen_contents = parser.screen().contents();

        Self::export_to_markdown(
            agent_name,
            project_name,
            started_at,
            state_label,
            &screen_contents,
            output_path,
        )
    }

    /// Expand `~` in a path to the user's home directory.
    pub fn expand_output_dir(path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy();
        if let Some(stripped) = path_str.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = OutputExporter::export_to_markdown(
            "agent",
            "project",
            Utc::now(),
            "Completed",
            "hello world",
            dir.path(),
        )
        .unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Agent: agent @ project"));
        assert!(content.contains("hello world"));
    }

    #[test]
    fn test_export_markdown_contains_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let started = Utc::now();
        let path = OutputExporter::export_to_markdown(
            "backend-refactor",
            "myapp",
            started,
            "Completed",
            "some output",
            dir.path(),
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Agent: backend-refactor @ myapp"));
        assert!(content.contains("**Started:**"));
        assert!(content.contains("**Duration:**"));
        assert!(content.contains("**Status:** Completed"));
        assert!(content.contains("## Terminal Output"));
        assert!(content.contains("some output"));
    }

    #[test]
    fn test_export_filename_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = OutputExporter::export_to_markdown(
            "agent",
            "project",
            Utc::now(),
            "Completed",
            "test",
            dir.path(),
        )
        .unwrap();
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(filename.starts_with("project_agent_completed_"));
        assert!(filename.ends_with(".md"));
    }

    #[test]
    fn test_export_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested_path = dir.path().join("nested").join("subdir");
        let path = OutputExporter::export_to_markdown(
            "agent",
            "project",
            Utc::now(),
            "Completed",
            "test",
            &nested_path,
        )
        .unwrap();
        assert!(path.exists());
        assert!(nested_path.exists());
    }

    #[test]
    fn test_export_state_label_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = OutputExporter::export_to_markdown(
            "agent",
            "project",
            Utc::now(),
            "Waiting For Input",
            "test",
            dir.path(),
        )
        .unwrap();
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(filename.contains("waiting_for_input"));
    }

    #[test]
    fn test_export_from_scrollback() {
        let dir = tempfile::tempdir().unwrap();
        let config = ExportConfig::default();
        let raw_bytes = b"Hello from scrollback\r\nSecond line";
        let path = OutputExporter::export_from_scrollback(
            "agent",
            "project",
            Utc::now(),
            "Completed",
            raw_bytes,
            dir.path(),
            &config,
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Hello from scrollback"));
        assert!(content.contains("Second line"));
    }

    #[test]
    fn test_export_from_scrollback_strips_ansi() {
        let dir = tempfile::tempdir().unwrap();
        let config = ExportConfig::default();
        // Bytes with ANSI color codes
        let raw_bytes = b"\x1b[31mRed text\x1b[0m normal text";
        let path = OutputExporter::export_from_scrollback(
            "agent",
            "project",
            Utc::now(),
            "Completed",
            raw_bytes,
            dir.path(),
            &config,
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // vt100 parser strips ANSI codes, so we get clean text
        assert!(content.contains("Red text"));
        assert!(content.contains("normal text"));
        assert!(!content.contains("\x1b["));
    }

    #[test]
    fn test_expand_output_dir_with_tilde() {
        let path = Path::new("~/some/path");
        let expanded = OutputExporter::expand_output_dir(path);
        // Should not start with ~/ anymore (unless HOME is not set)
        if dirs::home_dir().is_some() {
            assert!(!expanded.to_string_lossy().starts_with("~/"));
        }
    }

    #[test]
    fn test_expand_output_dir_without_tilde() {
        let path = Path::new("/absolute/path");
        let expanded = OutputExporter::expand_output_dir(path);
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_export_config_defaults() {
        let config = ExportConfig::default();
        assert_eq!(
            config.output_dir,
            PathBuf::from("~/.local/share/maestro/exports")
        );
        assert!(!config.auto_export_on_complete);
        assert_eq!(config.export_cols, 120);
        assert_eq!(config.export_rows, 50);
    }

    #[test]
    fn test_export_config_deserialize() {
        let toml_str = r#"
            output_dir = "/tmp/exports"
            auto_export_on_complete = true
            export_cols = 200
            export_rows = 80
        "#;
        let config: ExportConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output_dir, PathBuf::from("/tmp/exports"));
        assert!(config.auto_export_on_complete);
        assert_eq!(config.export_cols, 200);
        assert_eq!(config.export_rows, 80);
    }
}
