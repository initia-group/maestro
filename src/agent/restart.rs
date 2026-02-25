//! Auto-restart policy and tracking for agents.
//!
//! Implements configurable auto-restart with exponential backoff.
//! When an agent exits, the `RestartTracker` determines whether it should
//! be restarted and with what delay.
//! See Feature 20 (Output Export & Stream-JSON) for the full spec.

use serde::Deserialize;
use std::time::{Duration, Instant};

/// Configurable restart policy for an agent.
///
/// Controls whether an agent is automatically restarted on exit,
/// how many times, and with what delay between restarts.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RestartPolicy {
    /// Whether auto-restart is enabled.
    pub auto_restart: bool,
    /// Maximum number of restarts before giving up.
    pub max_restarts: u32,
    /// Base delay in seconds before the first restart.
    pub restart_delay_secs: u64,
    /// Exponential backoff multiplier applied to the delay for each
    /// successive restart.
    pub restart_backoff_multiplier: f64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            auto_restart: false,
            max_restarts: 3,
            restart_delay_secs: 5,
            restart_backoff_multiplier: 2.0,
        }
    }
}

/// Tracks restart attempts for a single agent, applying the policy.
#[derive(Debug)]
pub struct RestartTracker {
    /// Number of restarts performed so far.
    restart_count: u32,
    /// When the last restart occurred.
    last_restart: Option<Instant>,
    /// The policy governing restart behavior.
    policy: RestartPolicy,
}

impl RestartTracker {
    /// Create a new tracker with the given policy.
    pub fn new(policy: RestartPolicy) -> Self {
        Self {
            restart_count: 0,
            last_restart: None,
            policy,
        }
    }

    /// Should this agent be auto-restarted?
    ///
    /// Returns `true` if auto-restart is enabled and the restart count
    /// has not reached the maximum.
    pub fn should_restart(&self) -> bool {
        self.policy.auto_restart && self.restart_count < self.policy.max_restarts
    }

    /// Calculate the delay before the next restart.
    ///
    /// Uses exponential backoff: `base * multiplier^count`.
    /// The delay is capped at 300 seconds (5 minutes).
    pub fn next_delay(&self) -> Duration {
        let base = self.policy.restart_delay_secs as f64;
        let multiplied = base
            * self
                .policy
                .restart_backoff_multiplier
                .powi(self.restart_count as i32);
        Duration::from_secs_f64(multiplied.min(300.0))
    }

    /// Record that a restart has occurred.
    pub fn record_restart(&mut self) {
        self.restart_count += 1;
        self.last_restart = Some(Instant::now());
    }

    /// Get the number of restarts performed so far.
    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Get the maximum number of restarts allowed.
    pub fn max_restarts(&self) -> u32 {
        self.policy.max_restarts
    }

    /// Get the time of the last restart, if any.
    pub fn last_restart(&self) -> Option<Instant> {
        self.last_restart
    }

    /// Whether auto-restart is enabled in the policy.
    pub fn is_enabled(&self) -> bool {
        self.policy.auto_restart
    }

    /// Reset the restart counter (e.g., after a successful long run).
    pub fn reset(&mut self) {
        self.restart_count = 0;
        self.last_restart = None;
    }

    /// Format a status string for sidebar display.
    ///
    /// Returns something like "Restarts: 2/3" or "No auto-restart".
    pub fn status_display(&self) -> String {
        if !self.policy.auto_restart {
            "No auto-restart".to_string()
        } else if self.restart_count >= self.policy.max_restarts {
            format!(
                "Restart limit reached ({}/{})",
                self.restart_count, self.policy.max_restarts
            )
        } else {
            format!(
                "Restarts: {}/{}",
                self.restart_count, self.policy.max_restarts
            )
        }
    }
}

impl Default for RestartTracker {
    fn default() -> Self {
        Self::new(RestartPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RestartPolicy tests ---

    #[test]
    fn test_restart_policy_defaults() {
        let policy = RestartPolicy::default();
        assert!(!policy.auto_restart);
        assert_eq!(policy.max_restarts, 3);
        assert_eq!(policy.restart_delay_secs, 5);
        assert_eq!(policy.restart_backoff_multiplier, 2.0);
    }

    #[test]
    fn test_restart_policy_deserialize() {
        let toml_str = r#"
            auto_restart = true
            max_restarts = 5
            restart_delay_secs = 10
            restart_backoff_multiplier = 1.5
        "#;
        let policy: RestartPolicy = toml::from_str(toml_str).unwrap();
        assert!(policy.auto_restart);
        assert_eq!(policy.max_restarts, 5);
        assert_eq!(policy.restart_delay_secs, 10);
        assert_eq!(policy.restart_backoff_multiplier, 1.5);
    }

    #[test]
    fn test_restart_policy_partial_deserialize() {
        let toml_str = r#"
            auto_restart = true
        "#;
        let policy: RestartPolicy = toml::from_str(toml_str).unwrap();
        assert!(policy.auto_restart);
        // Defaults for the rest
        assert_eq!(policy.max_restarts, 3);
        assert_eq!(policy.restart_delay_secs, 5);
        assert_eq!(policy.restart_backoff_multiplier, 2.0);
    }

    // --- RestartTracker tests ---

    #[test]
    fn test_should_restart_when_enabled() {
        let tracker = RestartTracker::new(RestartPolicy {
            auto_restart: true,
            max_restarts: 3,
            ..Default::default()
        });
        assert!(tracker.should_restart());
    }

    #[test]
    fn test_should_not_restart_when_disabled() {
        let tracker = RestartTracker::new(RestartPolicy {
            auto_restart: false,
            max_restarts: 3,
            ..Default::default()
        });
        assert!(!tracker.should_restart());
    }

    #[test]
    fn test_should_not_restart_at_limit() {
        let mut tracker = RestartTracker::new(RestartPolicy {
            auto_restart: true,
            max_restarts: 3,
            ..Default::default()
        });
        tracker.record_restart();
        tracker.record_restart();
        tracker.record_restart();
        assert!(!tracker.should_restart());
        assert_eq!(tracker.restart_count(), 3);
    }

    #[test]
    fn test_backoff_delay_initial() {
        let tracker = RestartTracker::new(RestartPolicy {
            restart_delay_secs: 5,
            restart_backoff_multiplier: 2.0,
            ..Default::default()
        });
        // 5 * 2^0 = 5
        assert_eq!(tracker.next_delay(), Duration::from_secs(5));
    }

    #[test]
    fn test_backoff_delay_after_one_restart() {
        let mut tracker = RestartTracker::new(RestartPolicy {
            restart_delay_secs: 5,
            restart_backoff_multiplier: 2.0,
            ..Default::default()
        });
        tracker.record_restart();
        // 5 * 2^1 = 10
        assert_eq!(tracker.next_delay(), Duration::from_secs(10));
    }

    #[test]
    fn test_backoff_delay_after_two_restarts() {
        let mut tracker = RestartTracker::new(RestartPolicy {
            restart_delay_secs: 5,
            restart_backoff_multiplier: 2.0,
            ..Default::default()
        });
        tracker.record_restart();
        tracker.record_restart();
        // 5 * 2^2 = 20
        assert_eq!(tracker.next_delay(), Duration::from_secs(20));
    }

    #[test]
    fn test_backoff_delay_capped_at_five_minutes() {
        let mut tracker = RestartTracker::new(RestartPolicy {
            restart_delay_secs: 100,
            restart_backoff_multiplier: 10.0,
            max_restarts: 100,
            ..Default::default()
        });
        tracker.record_restart();
        tracker.record_restart();
        tracker.record_restart();
        // Would be 100 * 10^3 = 100_000, but capped at 300
        assert_eq!(tracker.next_delay(), Duration::from_secs(300));
    }

    #[test]
    fn test_record_restart_updates_count() {
        let mut tracker = RestartTracker::new(RestartPolicy::default());
        assert_eq!(tracker.restart_count(), 0);
        assert!(tracker.last_restart().is_none());

        tracker.record_restart();
        assert_eq!(tracker.restart_count(), 1);
        assert!(tracker.last_restart().is_some());

        tracker.record_restart();
        assert_eq!(tracker.restart_count(), 2);
    }

    #[test]
    fn test_reset_tracker() {
        let mut tracker = RestartTracker::new(RestartPolicy {
            auto_restart: true,
            max_restarts: 3,
            ..Default::default()
        });
        tracker.record_restart();
        tracker.record_restart();
        tracker.record_restart();
        assert!(!tracker.should_restart());

        tracker.reset();
        assert_eq!(tracker.restart_count(), 0);
        assert!(tracker.last_restart().is_none());
        assert!(tracker.should_restart());
    }

    #[test]
    fn test_status_display_disabled() {
        let tracker = RestartTracker::default();
        assert_eq!(tracker.status_display(), "No auto-restart");
    }

    #[test]
    fn test_status_display_enabled() {
        let tracker = RestartTracker::new(RestartPolicy {
            auto_restart: true,
            max_restarts: 5,
            ..Default::default()
        });
        assert_eq!(tracker.status_display(), "Restarts: 0/5");
    }

    #[test]
    fn test_status_display_limit_reached() {
        let mut tracker = RestartTracker::new(RestartPolicy {
            auto_restart: true,
            max_restarts: 2,
            ..Default::default()
        });
        tracker.record_restart();
        tracker.record_restart();
        assert_eq!(tracker.status_display(), "Restart limit reached (2/2)");
    }

    #[test]
    fn test_is_enabled() {
        let tracker_off = RestartTracker::new(RestartPolicy::default());
        assert!(!tracker_off.is_enabled());

        let tracker_on = RestartTracker::new(RestartPolicy {
            auto_restart: true,
            ..Default::default()
        });
        assert!(tracker_on.is_enabled());
    }

    #[test]
    fn test_max_restarts_accessor() {
        let tracker = RestartTracker::new(RestartPolicy {
            max_restarts: 7,
            ..Default::default()
        });
        assert_eq!(tracker.max_restarts(), 7);
    }

    #[test]
    fn test_default_tracker() {
        let tracker = RestartTracker::default();
        assert_eq!(tracker.restart_count(), 0);
        assert!(!tracker.is_enabled());
        assert!(!tracker.should_restart());
    }

    #[test]
    fn test_backoff_with_fractional_multiplier() {
        let tracker = RestartTracker::new(RestartPolicy {
            restart_delay_secs: 10,
            restart_backoff_multiplier: 1.5,
            ..Default::default()
        });
        // 10 * 1.5^0 = 10
        assert_eq!(tracker.next_delay(), Duration::from_secs(10));
    }
}
