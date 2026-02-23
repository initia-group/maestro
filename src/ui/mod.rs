//! UI widgets and layout system.
//!
//! All Ratatui widgets and the layout calculation engine.

pub mod command_palette;
pub mod layout;
pub mod pane_manager;
pub mod sidebar;
pub mod spawn_picker;
pub mod status_bar;
pub mod terminal_pane;
pub mod theme;

// Re-export core types for convenient access.
pub use layout::{ActiveLayout, AppLayout, PaneLayout, calculate_layout};
pub use pane_manager::PaneManager;
pub use sidebar::{Sidebar, SidebarItem, SidebarState};
pub use terminal_pane::{EmptyPane, TerminalPane, cursor_position};
pub use theme::Theme;
