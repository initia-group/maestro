//! Workspace profile management.
//!
//! `ProfileManager` handles named workspace profiles — collections of projects
//! and agents that can be switched as a unit. Switching profiles kills all
//! current agents and spawns the new profile's agents.

use crate::config::settings::ProfileConfig;

/// Manages workspace profiles: listing, switching, and active tracking.
pub struct ProfileManager {
    /// All defined profiles.
    profiles: Vec<ProfileConfig>,
    /// Currently active profile name (None = use top-level project definitions).
    active_profile: Option<String>,
}

impl ProfileManager {
    /// Create a new profile manager.
    ///
    /// `profiles` is the list of defined profiles from config.
    /// `active` is the initially active profile name (from config or CLI).
    pub fn new(profiles: Vec<ProfileConfig>, active: Option<String>) -> Self {
        Self {
            profiles,
            active_profile: active,
        }
    }

    /// Get the currently active profile.
    pub fn active(&self) -> Option<&ProfileConfig> {
        self.active_profile.as_ref().and_then(|name| {
            self.profiles.iter().find(|p| p.name == *name)
        })
    }

    /// Get the name of the currently active profile.
    pub fn active_name(&self) -> Option<&str> {
        self.active_profile.as_deref()
    }

    /// List all available profiles.
    pub fn list(&self) -> &[ProfileConfig] {
        &self.profiles
    }

    /// Switch to a different profile.
    /// Returns the new profile's config if found.
    pub fn switch(&mut self, name: &str) -> Option<&ProfileConfig> {
        if self.profiles.iter().any(|p| p.name == name) {
            self.active_profile = Some(name.to_string());
            self.active()
        } else {
            None
        }
    }

    /// Check whether a profile with the given name exists.
    pub fn has_profile(&self, name: &str) -> bool {
        self.profiles.iter().any(|p| p.name == name)
    }

    /// Format a list of available profile names for error messages.
    pub fn available_names(&self) -> Vec<&str> {
        self.profiles.iter().map(|p| p.name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profiles() -> Vec<ProfileConfig> {
        vec![
            ProfileConfig {
                name: "dev".into(),
                description: Some("Development workflow".into()),
                project: vec![],
            },
            ProfileConfig {
                name: "review".into(),
                description: None,
                project: vec![],
            },
        ]
    }

    #[test]
    fn test_profile_switch() {
        let profiles = make_profiles();
        let mut manager = ProfileManager::new(profiles, Some("dev".into()));

        assert_eq!(manager.active().unwrap().name, "dev");

        manager.switch("review");
        assert_eq!(manager.active().unwrap().name, "review");
    }

    #[test]
    fn test_profile_switch_nonexistent() {
        let profiles = vec![ProfileConfig {
            name: "dev".into(),
            description: None,
            project: vec![],
        }];
        let mut manager = ProfileManager::new(profiles, None);
        assert!(manager.switch("nonexistent").is_none());
        assert!(manager.active().is_none());
    }

    #[test]
    fn test_profile_list() {
        let profiles = vec![
            ProfileConfig {
                name: "a".into(),
                description: None,
                project: vec![],
            },
            ProfileConfig {
                name: "b".into(),
                description: None,
                project: vec![],
            },
        ];
        let manager = ProfileManager::new(profiles, None);
        assert_eq!(manager.list().len(), 2);
    }

    #[test]
    fn test_active_name() {
        let profiles = make_profiles();
        let manager = ProfileManager::new(profiles.clone(), Some("dev".into()));
        assert_eq!(manager.active_name(), Some("dev"));

        let manager = ProfileManager::new(profiles, None);
        assert_eq!(manager.active_name(), None);
    }

    #[test]
    fn test_has_profile() {
        let profiles = make_profiles();
        let manager = ProfileManager::new(profiles, None);
        assert!(manager.has_profile("dev"));
        assert!(manager.has_profile("review"));
        assert!(!manager.has_profile("nonexistent"));
    }

    #[test]
    fn test_available_names() {
        let profiles = make_profiles();
        let manager = ProfileManager::new(profiles, None);
        let names = manager.available_names();
        assert_eq!(names, vec!["dev", "review"]);
    }

    #[test]
    fn test_no_profiles() {
        let manager = ProfileManager::new(vec![], None);
        assert!(manager.active().is_none());
        assert!(manager.active_name().is_none());
        assert!(manager.list().is_empty());
        assert!(manager.available_names().is_empty());
    }

    #[test]
    fn test_active_profile_not_in_list() {
        let profiles = make_profiles();
        let manager = ProfileManager::new(profiles, Some("nonexistent".into()));
        // active_name returns Some even if not in list
        assert_eq!(manager.active_name(), Some("nonexistent"));
        // But active() returns None because it looks up by name
        assert!(manager.active().is_none());
    }

    #[test]
    fn test_switch_returns_profile() {
        let profiles = make_profiles();
        let mut manager = ProfileManager::new(profiles, None);
        let result = manager.switch("review");
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "review");
    }

    #[test]
    fn test_switch_preserves_state() {
        let profiles = make_profiles();
        let mut manager = ProfileManager::new(profiles, Some("dev".into()));
        assert_eq!(manager.active_name(), Some("dev"));

        manager.switch("review");
        assert_eq!(manager.active_name(), Some("review"));

        // Switch back
        manager.switch("dev");
        assert_eq!(manager.active_name(), Some("dev"));
    }
}
