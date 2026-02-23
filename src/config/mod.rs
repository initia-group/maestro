//! Configuration subsystem.
//!
//! TOML configuration loading, validation, and settings structs.

pub mod loader;
pub mod profile;
pub mod settings;

// Re-exports for convenience.
pub use loader::{expand_tilde, load_config};
pub use profile::ProfileManager;
pub use settings::{
    AgentConfig, DetectionConfig, GlobalConfig, LayoutMode, MaestroConfig, NotificationConfig,
    ProfileConfig, ProjectConfig, SessionConfig, TemplateConfig, ThemeConfig, UiConfig,
};
