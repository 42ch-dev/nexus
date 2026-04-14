//! Degradation policy for runtime mode fallback behavior.
//!
//! When platform becomes unavailable, the system gracefully degrades
//! from cloud_enhanced → local_first → local_only.
//!
//! Spec reference: `local-first-runtime-policy-v1.md` §5.3, §6
//!
//! # Degradation Chain
//!
//! ```text
//! cloud_enhanced → local_first → local_only
//! ```
//!
//! The [`DegradationGuard`] monitors platform health and triggers automatic
//! degradation when consecutive failures exceed the configured threshold.

use crate::{DomainError, DomainRuntimeMode};
use nexus_contracts::RuntimeMode;

/// Degradation policy for runtime mode fallback behavior.
///
/// When platform becomes unavailable, the system gracefully degrades
/// from cloud_enhanced → local_first → local_only.
///
/// Spec reference: `local-first-runtime-policy-v1.md` §5.3, §6
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DegradationPolicy {
    /// Timeout for platform health check (milliseconds).
    /// Default: 5000ms
    pub health_check_timeout_ms: u64,

    /// Number of consecutive failures before triggering degradation.
    /// Default: 3
    pub failure_threshold: u32,

    /// Time window for counting failures (seconds).
    /// Default: 60s
    pub failure_window_secs: u64,

    /// Cooldown period before attempting upgrade (seconds).
    /// Default: 300s (5 minutes)
    pub upgrade_cooldown_secs: u64,

    /// Maximum degradation depth (how many levels can degrade).
    /// 0 = no degradation allowed, 1 = one level down, 2 = two levels down.
    /// Default: 2 (cloud_enhanced → local_first → local_only)
    pub max_degradation_depth: u32,
}

impl Default for DegradationPolicy {
    fn default() -> Self {
        Self {
            health_check_timeout_ms: 5000,
            failure_threshold: 3,
            failure_window_secs: 60,
            upgrade_cooldown_secs: 300,
            max_degradation_depth: 2,
        }
    }
}

impl DegradationPolicy {
    /// Create a policy with all fields explicitly specified.
    pub fn new(
        health_check_timeout_ms: u64,
        failure_threshold: u32,
        failure_window_secs: u64,
        upgrade_cooldown_secs: u64,
        max_degradation_depth: u32,
    ) -> Self {
        Self {
            health_check_timeout_ms,
            failure_threshold,
            failure_window_secs,
            upgrade_cooldown_secs,
            max_degradation_depth,
        }
    }

    /// Create a policy that disables degradation entirely.
    pub fn no_degradation() -> Self {
        Self {
            max_degradation_depth: 0,
            ..Default::default()
        }
    }
}

/// Current degradation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DegradationState {
    /// Operating at configured mode (no degradation).
    Normal,
    /// Degraded one level (e.g., cloud_enhanced → local_first).
    DegradedLevel1,
    /// Degraded two levels (e.g., cloud_enhanced → local_only).
    DegradedLevel2,
    /// Platform unreachable, forced to local_only.
    ForcedLocalOnly,
}

impl DegradationState {
    /// Returns the numeric depth of this degradation state.
    /// 0 = Normal, 1 = DegradedLevel1, 2 = DegradedLevel2/ForcedLocalOnly.
    pub fn depth(&self) -> u32 {
        match self {
            DegradationState::Normal => 0,
            DegradationState::DegradedLevel1 => 1,
            DegradationState::DegradedLevel2 | DegradationState::ForcedLocalOnly => 2,
        }
    }

    /// Check if further degradation is possible given the max depth.
    pub fn can_degrade_more(&self, max_depth: u32) -> bool {
        self.depth() < max_depth
    }

    /// Whether this state represents any form of degradation.
    pub fn is_degraded(&self) -> bool {
        !matches!(self, DegradationState::Normal)
    }
}

impl std::fmt::Display for DegradationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DegradationState::Normal => write!(f, "normal"),
            DegradationState::DegradedLevel1 => write!(f, "degraded_level_1"),
            DegradationState::DegradedLevel2 => write!(f, "degraded_level_2"),
            DegradationState::ForcedLocalOnly => write!(f, "forced_local_only"),
        }
    }
}

/// Record of platform health check result.
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Whether platform responded successfully.
    pub is_healthy: bool,
    /// Response time in milliseconds (if healthy).
    pub response_time_ms: Option<u64>,
    /// Error message (if unhealthy).
    pub error_message: Option<String>,
    /// Timestamp of the check.
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

impl HealthCheckResult {
    /// Create a healthy check result.
    pub fn healthy(response_time_ms: u64) -> Self {
        Self {
            is_healthy: true,
            response_time_ms: Some(response_time_ms),
            error_message: None,
            checked_at: chrono::Utc::now(),
        }
    }

    /// Create an unhealthy check result.
    pub fn unhealthy(error: impl Into<String>) -> Self {
        Self {
            is_healthy: false,
            response_time_ms: None,
            error_message: Some(error.into()),
            checked_at: chrono::Utc::now(),
        }
    }
}

/// Guard that monitors platform health and triggers degradation.
pub struct DegradationGuard {
    policy: DegradationPolicy,
    current_mode: DomainRuntimeMode,
    degradation_state: DegradationState,
    failure_count: u32,
    last_failure_time: Option<chrono::DateTime<chrono::Utc>>,
    last_health_check: Option<HealthCheckResult>,
    last_upgrade_attempt: Option<chrono::DateTime<chrono::Utc>>,
}

impl DegradationGuard {
    /// Create a new guard with the given policy and initial runtime mode.
    pub fn new(policy: DegradationPolicy, initial_mode: DomainRuntimeMode) -> Self {
        Self {
            policy,
            current_mode: initial_mode,
            degradation_state: DegradationState::Normal,
            failure_count: 0,
            last_failure_time: None,
            last_upgrade_attempt: None,
            last_health_check: None,
        }
    }

    /// Create a new guard with default policy and the given initial mode.
    pub fn with_defaults(initial_mode: DomainRuntimeMode) -> Self {
        Self::new(DegradationPolicy::default(), initial_mode)
    }

    /// Access the current runtime mode (may have been downgraded by degradation).
    pub fn current_mode(&self) -> &DomainRuntimeMode {
        &self.current_mode
    }

    /// Access the current degradation state.
    pub fn degradation_state(&self) -> DegradationState {
        self.degradation_state
    }

    /// Access the degradation policy.
    pub fn policy(&self) -> &DegradationPolicy {
        &self.policy
    }

    /// Access the last health check result (if any).
    pub fn last_health_check(&self) -> Option<&HealthCheckResult> {
        self.last_health_check.as_ref()
    }

    /// Access the current failure count.
    pub fn failure_count(&self) -> u32 {
        self.failure_count
    }

    /// Check if operation requires platform and if degradation allows it.
    /// Returns error if operation is blocked by degradation state.
    ///
    /// This combines the existing `runtime_guard` classification with
    /// degradation-awareness: platform-required operations are blocked
    /// when the current (possibly degraded) mode is local_only.
    pub fn check_operation(&self, operation: &str) -> Result<(), DomainError> {
        crate::runtime_guard::check_operation(&self.current_mode, operation)
    }

    /// Record a platform operation result.
    /// Updates failure count and potentially triggers degradation.
    pub fn record_platform_result(&mut self, success: bool, error: Option<String>) {
        if success {
            self.failure_count = 0;
            self.last_failure_time = None;
            self.update_health_check(true, None, None);
        } else {
            self.failure_count += 1;
            self.last_failure_time = Some(chrono::Utc::now());
            self.update_health_check(false, None, error);

            if self.should_degrade() {
                self.degrade();
            }
        }
    }

    /// Record a health check result directly.
    /// Updates the last health check and may trigger degradation.
    pub fn record_health_check(&mut self, result: HealthCheckResult) {
        let is_healthy = result.is_healthy;
        self.last_health_check = Some(result);
        if !is_healthy {
            self.failure_count += 1;
            self.last_failure_time = Some(chrono::Utc::now());
            if self.should_degrade() {
                self.degrade();
            }
        } else {
            self.failure_count = 0;
            self.last_failure_time = None;
        }
    }

    /// Check if we should degrade based on failure threshold.
    fn should_degrade(&self) -> bool {
        if self.failure_count >= self.policy.failure_threshold {
            if let Some(last) = self.last_failure_time {
                let elapsed = chrono::Utc::now().signed_duration_since(last).num_seconds();
                if elapsed <= self.policy.failure_window_secs as i64 {
                    return self
                        .degradation_state
                        .can_degrade_more(self.policy.max_degradation_depth);
                }
            }
        }
        false
    }

    /// Perform degradation (move one level down).
    fn degrade(&mut self) {
        let next_state = match self.degradation_state {
            DegradationState::Normal => {
                if self.current_mode.downgrade().is_some() {
                    Some(DegradationState::DegradedLevel1)
                } else {
                    None
                }
            }
            DegradationState::DegradedLevel1 => {
                if self.current_mode.downgrade().is_some() {
                    Some(DegradationState::DegradedLevel2)
                } else {
                    None
                }
            }
            DegradationState::DegradedLevel2 | DegradationState::ForcedLocalOnly => None,
        };

        if let Some(next) = next_state {
            self.degradation_state = next;
            if let Some(downgraded) = self.current_mode.downgrade() {
                self.current_mode = downgraded;
            }
            self.failure_count = 0; // Reset after degradation
        }
    }

    /// Force immediate degradation to local_only regardless of threshold.
    pub fn force_local_only(&mut self) {
        self.degradation_state = DegradationState::ForcedLocalOnly;
        self.current_mode = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        self.failure_count = 0;
    }

    /// Try to upgrade back to original mode.
    /// Returns true if upgrade succeeded.
    ///
    /// Upgrade requires:
    /// 1. Cooldown period has elapsed since last upgrade attempt
    /// 2. Platform is currently healthy (last health check succeeded)
    pub fn try_upgrade(&mut self) -> bool {
        // Check cooldown
        if let Some(last) = self.last_upgrade_attempt {
            let elapsed = chrono::Utc::now().signed_duration_since(last).num_seconds();
            if elapsed < self.policy.upgrade_cooldown_secs as i64 {
                return false;
            }
        }

        // Only upgrade if platform is healthy
        if self.last_health_check.as_ref().map(|h| h.is_healthy) == Some(true) {
            let upgraded = match self.degradation_state {
                DegradationState::DegradedLevel2 => {
                    if let Some(up) = self.current_mode.upgrade() {
                        self.current_mode = up;
                        self.degradation_state = DegradationState::DegradedLevel1;
                        true
                    } else {
                        false
                    }
                }
                DegradationState::DegradedLevel1 => {
                    if let Some(up) = self.current_mode.upgrade() {
                        self.current_mode = up;
                        self.degradation_state = DegradationState::Normal;
                        true
                    } else {
                        false
                    }
                }
                DegradationState::Normal | DegradationState::ForcedLocalOnly => false,
            };
            if upgraded {
                self.last_upgrade_attempt = Some(chrono::Utc::now());
            }
            return upgraded;
        }
        false
    }

    /// Reset the guard to normal state with the given mode.
    pub fn reset(&mut self, mode: DomainRuntimeMode) {
        self.current_mode = mode;
        self.degradation_state = DegradationState::Normal;
        self.failure_count = 0;
        self.last_failure_time = None;
        self.last_upgrade_attempt = None;
    }

    fn update_health_check(
        &mut self,
        is_healthy: bool,
        response_time: Option<u64>,
        error: Option<String>,
    ) {
        self.last_health_check = Some(HealthCheckResult {
            is_healthy,
            response_time_ms: response_time,
            error_message: error,
            checked_at: chrono::Utc::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DegradationPolicy tests ─────────────────────────────────────────

    #[test]
    fn default_policy_has_expected_values() {
        let policy = DegradationPolicy::default();
        assert_eq!(policy.health_check_timeout_ms, 5000);
        assert_eq!(policy.failure_threshold, 3);
        assert_eq!(policy.failure_window_secs, 60);
        assert_eq!(policy.upgrade_cooldown_secs, 300);
        assert_eq!(policy.max_degradation_depth, 2);
    }

    #[test]
    fn no_degradation_policy_disables_fallback() {
        let policy = DegradationPolicy::no_degradation();
        assert_eq!(policy.max_degradation_depth, 0);
        // Other fields should still have defaults
        assert_eq!(policy.failure_threshold, 3);
    }

    #[test]
    fn custom_policy_builder() {
        let policy = DegradationPolicy::new(1000, 5, 30, 60, 1);
        assert_eq!(policy.health_check_timeout_ms, 1000);
        assert_eq!(policy.failure_threshold, 5);
        assert_eq!(policy.failure_window_secs, 30);
        assert_eq!(policy.upgrade_cooldown_secs, 60);
        assert_eq!(policy.max_degradation_depth, 1);
    }

    #[test]
    fn policy_serde_roundtrip() {
        let policy = DegradationPolicy::default();
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: DegradationPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, deserialized);
    }

    // ── DegradationState tests ─────────────────────────────────────────

    #[test]
    fn degradation_state_depths() {
        assert_eq!(DegradationState::Normal.depth(), 0);
        assert_eq!(DegradationState::DegradedLevel1.depth(), 1);
        assert_eq!(DegradationState::DegradedLevel2.depth(), 2);
        assert_eq!(DegradationState::ForcedLocalOnly.depth(), 2);
    }

    #[test]
    fn can_degrade_more_respects_max_depth() {
        assert!(DegradationState::Normal.can_degrade_more(2));
        assert!(DegradationState::Normal.can_degrade_more(1));
        assert!(!DegradationState::Normal.can_degrade_more(0));

        assert!(DegradationState::DegradedLevel1.can_degrade_more(2));
        assert!(!DegradationState::DegradedLevel1.can_degrade_more(1));

        assert!(!DegradationState::DegradedLevel2.can_degrade_more(2));
        assert!(!DegradationState::ForcedLocalOnly.can_degrade_more(2));
    }

    #[test]
    fn is_degraded() {
        assert!(!DegradationState::Normal.is_degraded());
        assert!(DegradationState::DegradedLevel1.is_degraded());
        assert!(DegradationState::DegradedLevel2.is_degraded());
        assert!(DegradationState::ForcedLocalOnly.is_degraded());
    }

    #[test]
    fn degradation_state_display() {
        assert_eq!(DegradationState::Normal.to_string(), "normal");
        assert_eq!(
            DegradationState::DegradedLevel1.to_string(),
            "degraded_level_1"
        );
        assert_eq!(
            DegradationState::DegradedLevel2.to_string(),
            "degraded_level_2"
        );
        assert_eq!(
            DegradationState::ForcedLocalOnly.to_string(),
            "forced_local_only"
        );
    }

    #[test]
    fn degradation_state_serde_roundtrip() {
        for state in [
            DegradationState::Normal,
            DegradationState::DegradedLevel1,
            DegradationState::DegradedLevel2,
            DegradationState::ForcedLocalOnly,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: DegradationState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, deserialized, "roundtrip failed for {state:?}");
        }
    }

    // ── HealthCheckResult tests ─────────────────────────────────────────

    #[test]
    fn healthy_check_result() {
        let result = HealthCheckResult::healthy(42);
        assert!(result.is_healthy);
        assert_eq!(result.response_time_ms, Some(42));
        assert!(result.error_message.is_none());
    }

    #[test]
    fn unhealthy_check_result() {
        let result = HealthCheckResult::unhealthy("connection refused");
        assert!(!result.is_healthy);
        assert!(result.response_time_ms.is_none());
        assert_eq!(result.error_message, Some("connection refused".to_string()));
    }

    // ── DegradationGuard tests ─────────────────────────────────────────

    #[test]
    fn guard_starts_in_normal_state() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let guard = DegradationGuard::with_defaults(mode);
        assert_eq!(guard.degradation_state(), DegradationState::Normal);
        assert_eq!(*guard.current_mode(), mode);
        assert_eq!(guard.failure_count(), 0);
    }

    #[test]
    fn failure_threshold_triggers_degradation() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy::new(5000, 3, 60, 300, 2);
        let mut guard = DegradationGuard::new(policy, mode);

        // Record failures up to threshold
        guard.record_platform_result(false, Some("error 1".into()));
        assert_eq!(guard.degradation_state(), DegradationState::Normal);

        guard.record_platform_result(false, Some("error 2".into()));
        assert_eq!(guard.degradation_state(), DegradationState::Normal);

        guard.record_platform_result(false, Some("error 3".into()));
        // Third failure should trigger degradation
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::LocalFirst)
        );
    }

    #[test]
    fn consecutive_failures_can_degrade_two_levels() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy::new(5000, 3, 60, 300, 2);
        let mut guard = DegradationGuard::new(policy, mode);

        // First degradation: cloud_enhanced → local_first
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);

        // Second degradation: local_first → local_only
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel2);
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::LocalOnly)
        );
    }

    #[test]
    fn success_resets_failure_count() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let mut guard = DegradationGuard::with_defaults(mode);

        guard.record_platform_result(false, Some("fail".into()));
        assert_eq!(guard.failure_count(), 1);

        guard.record_platform_result(true, None);
        assert_eq!(guard.failure_count(), 0);

        // Two more failures shouldn't trigger degradation (not at threshold)
        guard.record_platform_result(false, Some("fail".into()));
        guard.record_platform_result(false, Some("fail".into()));
        assert_eq!(guard.degradation_state(), DegradationState::Normal);
    }

    #[test]
    fn check_operation_respects_degraded_mode() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy::new(5000, 3, 60, 300, 2);
        let mut guard = DegradationGuard::new(policy, mode);

        // sync is platform-required: should be allowed in cloud_enhanced
        assert!(guard.check_operation("sync").is_ok());

        // Degrade to local_first (still allows platform)
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);
        assert!(guard.check_operation("sync").is_ok());

        // Degrade to local_only (blocks platform)
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel2);
        assert!(guard.check_operation("sync").is_err());
    }

    #[test]
    fn upgrade_requires_healthy_platform() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy::new(5000, 3, 60, 0, 2); // no cooldown
        let mut guard = DegradationGuard::new(policy, mode);

        // Degrade to level 1
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);

        // Try upgrade without healthy check — should fail
        assert!(!guard.try_upgrade());

        // Record healthy result
        guard.record_platform_result(true, None);
        // Now upgrade should succeed
        assert!(guard.try_upgrade());
        assert_eq!(guard.degradation_state(), DegradationState::Normal);
    }

    #[test]
    fn upgrade_respects_cooldown() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy::new(5000, 3, 60, 300, 2); // 300s cooldown
        let mut guard = DegradationGuard::new(policy, mode);

        // Degrade to level 1
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        guard.record_platform_result(true, None);

        // First upgrade succeeds
        assert!(guard.try_upgrade());
        assert_eq!(guard.degradation_state(), DegradationState::Normal);

        // Immediately degrade again
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        guard.record_platform_result(true, None);

        // Second upgrade should fail due to cooldown
        assert!(!guard.try_upgrade());
    }

    #[test]
    fn force_local_only() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let mut guard = DegradationGuard::with_defaults(mode);

        guard.force_local_only();
        assert_eq!(guard.degradation_state(), DegradationState::ForcedLocalOnly);
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::LocalOnly)
        );
        assert!(guard.check_operation("sync").is_err());
    }

    #[test]
    fn reset_clears_degradation() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let mut guard = DegradationGuard::with_defaults(mode);

        guard.force_local_only();
        assert_eq!(guard.degradation_state(), DegradationState::ForcedLocalOnly);

        let new_mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        guard.reset(new_mode);
        assert_eq!(guard.degradation_state(), DegradationState::Normal);
        assert_eq!(*guard.current_mode(), new_mode);
        assert_eq!(guard.failure_count(), 0);
    }

    #[test]
    fn no_degradation_policy_prevents_fallback() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy::no_degradation();
        let mut guard = DegradationGuard::new(policy, mode);

        // Even with many failures, no degradation should occur
        for _ in 0..10 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::Normal);
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::CloudEnhanced)
        );
    }

    #[test]
    fn starting_from_local_first_degrades_to_local_only() {
        let mode = DomainRuntimeMode::new(RuntimeMode::LocalFirst);
        let policy = DegradationPolicy::new(5000, 3, 60, 300, 2);
        let mut guard = DegradationGuard::new(policy, mode);

        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::LocalOnly)
        );
    }

    #[test]
    fn starting_from_local_only_stays_local_only() {
        let mode = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        let policy = DegradationPolicy::new(5000, 3, 60, 300, 2);
        let mut guard = DegradationGuard::new(policy, mode);

        // Cannot degrade further from local_only
        for _ in 0..10 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        assert_eq!(guard.degradation_state(), DegradationState::Normal);
        // Mode stays local_only because downgrade() returns None
        assert_eq!(
            *guard.current_mode(),
            DomainRuntimeMode::new(RuntimeMode::LocalOnly)
        );
    }
}
