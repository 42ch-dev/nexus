//! Degradation policy for runtime mode fallback behavior.
//!
//! When platform becomes unavailable, the system gracefully degrades
//! from `cloud_enhanced` → `local_first` → local_only.
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

use super::errors::DomainError;
use super::runtime_guard::check_operation;
use super::runtime_mode::DomainRuntimeMode;
use nexus_contracts::local::domain::RuntimeMode;

/// Degradation policy for runtime mode fallback behavior.
///
/// When platform becomes unavailable, the system gracefully degrades
/// from `cloud_enhanced` → `local_first` → local_only.
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
    /// Default: 2 (`cloud_enhanced` → `local_first` → local_only)
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
    #[must_use]
    /// Create a policy with all fields explicitly specified.
    pub const fn new(
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
    #[must_use]
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
    /// Degraded one level (e.g., `cloud_enhanced` → `local_first`).
    DegradedLevel1,
    /// Degraded two levels (e.g., `cloud_enhanced` → `local_only`).
    DegradedLevel2,
    /// Platform unreachable, forced to `local_only`.
    ForcedLocalOnly,
}

impl DegradationState {
    /// Returns the numeric depth of this degradation state.
    /// 0 = Normal, 1 = `DegradedLevel1`, 2 = DegradedLevel2/ForcedLocalOnly.
    #[must_use]
    pub const fn depth(&self) -> u32 {
        match self {
            DegradationState::Normal => 0,
            DegradationState::DegradedLevel1 => 1,
            DegradationState::DegradedLevel2 | DegradationState::ForcedLocalOnly => 2,
        }
    }
    #[must_use]
    /// Check if further degradation is possible given the max depth.
    pub fn can_degrade_more(&self, max_depth: u32) -> bool {
        self.depth() < max_depth
    }

    /// Whether this state represents any form of degradation.
    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        !matches!(self, DegradationState::Normal)
    }

    /// Human-readable label for CLI output.
    pub const fn display_label(&self) -> &'static str {
        match self {
            DegradationState::Normal => "Normal",
            DegradationState::DegradedLevel1 => "Degraded (Level 1)",
            DegradationState::DegradedLevel2 => "Degraded (Level 2)",
            DegradationState::ForcedLocalOnly => "Forced local_only",
        }
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

/// Serializable snapshot of degradation guard state for config persistence.
///
/// This lightweight struct captures the display-relevant degradation state
/// and can be stored inline in `config.json` (V1.2 MVP strategy).
/// It is decoupled from the full [`DegradationGuard`] which holds runtime policy
/// and mutable state not suitable for simple persistence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DegradationSnapshot {
    /// Current degradation state.
    pub state: DegradationState,
    /// Number of consecutive platform failures.
    pub failure_count: u32,
    /// Last health check result (if any).
    pub last_health_check: Option<HealthCheckSnapshot>,
    /// ISO 8601 timestamp of last upgrade attempt (for cooldown persistence across restarts).
    /// Old snapshots without this field deserialize as `None`.
    #[serde(default)]
    pub last_upgrade_attempt: Option<String>,
}

/// Serializable record of a platform health check for config persistence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct HealthCheckSnapshot {
    /// Whether the platform responded successfully.
    pub is_healthy: bool,
    /// ISO 8601 timestamp of the check.
    pub checked_at: String,
}

impl DegradationSnapshot {
    /// Create a new snapshot with the given state and failure count.
    pub const fn new(state: DegradationState, failure_count: u32) -> Self {
        Self {
            state,
            failure_count,
            last_health_check: None,
            last_upgrade_attempt: None,
        }
    }

    /// Create a new snapshot with an explicit `last_upgrade_attempt`.
    pub const fn with_upgrade_attempt(
        state: DegradationState,
        failure_count: u32,
        last_upgrade_attempt: Option<String>,
    ) -> Self {
        Self {
            state,
            failure_count,
            last_health_check: None,
            last_upgrade_attempt,
        }
    }

    /// Create a snapshot from a live [`DegradationGuard`].
    pub fn from_guard(guard: &DegradationGuard) -> Self {
        Self {
            state: guard.degradation_state(),
            failure_count: guard.failure_count(),
            last_health_check: guard.last_health_check().map(|h| HealthCheckSnapshot {
                is_healthy: h.is_healthy,
                checked_at: h.checked_at.to_rfc3339(),
            }),
            last_upgrade_attempt: guard.last_upgrade_attempt().map(|dt| dt.to_rfc3339()),
        }
    }
}

/// Guard that monitors platform health and triggers degradation.
///
/// S-003: `DegradationGuard` is Send + Sync because all its fields are
/// thread-safe: `DegradationPolicy` (primitive types), `DomainRuntimeMode` (Copy),
/// `DegradationState` (Copy enum), `chrono::DateTime`<chrono::Utc> (Send+Sync),
/// and `HealthCheckResult` (Send+Sync). The guard is designed to be shared
/// across async tasks (e.g., in a tokio runtime) via Arc<Mutex<DegradationGuard>>.
pub struct DegradationGuard {
    policy: DegradationPolicy,
    current_mode: DomainRuntimeMode,
    degradation_state: DegradationState,
    /// Individual failure timestamps for correct window-based counting.
    failure_timestamps: Vec<chrono::DateTime<chrono::Utc>>,
    last_health_check: Option<HealthCheckResult>,
    last_upgrade_attempt: Option<chrono::DateTime<chrono::Utc>>,
}

// S-003: Compile-time verification that DegradationGuard is Send + Sync.
// This will fail to compile if the struct holds non-thread-safe types.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DegradationGuard>();
};

impl DegradationGuard {
    /// Create a new guard with the given policy and initial runtime mode.
    pub const fn new(policy: DegradationPolicy, initial_mode: DomainRuntimeMode) -> Self {
        Self {
            policy,
            current_mode: initial_mode,
            degradation_state: DegradationState::Normal,
            failure_timestamps: Vec::new(),
            last_upgrade_attempt: None,
            last_health_check: None,
        }
    }

    /// Create a new guard with default policy and the given initial mode.
    pub fn with_defaults(initial_mode: DomainRuntimeMode) -> Self {
        Self::new(DegradationPolicy::default(), initial_mode)
    }

    /// Restore a guard directly from a persisted snapshot.
    ///
    /// Sets the degradation state and effective mode without replaying
    /// failures, avoiding unintended re-degradation (C-001).
    ///
    /// The `effective_mode` is computed from `original_mode` by applying
    /// `state.depth()` downgrades. Cooldown and health-check timestamps
    /// are intentionally cleared so the system can re-evaluate health.
    #[must_use]
    pub fn restore_from_snapshot(
        snap: &DegradationSnapshot,
        original_mode: DomainRuntimeMode,
    ) -> Self {
        let mut effective_mode = original_mode;
        for _ in 0..snap.state.depth() {
            if let Some(downgraded) = effective_mode.downgrade() {
                effective_mode = downgraded;
            } else {
                break;
            }
        }

        // Cap synthetic timestamps to failure_threshold - 1 so that restored
        // failures alone can never trigger immediate re-degradation. Only NEW
        // real failures should push past the threshold.
        let capped_count = snap.failure_count.min(
            DegradationPolicy::default()
                .failure_threshold
                .saturating_sub(1),
        );

        Self {
            policy: DegradationPolicy::default(),
            current_mode: effective_mode,
            degradation_state: snap.state,
            // Replicate failure_count as synthetic timestamps so window logic works.
            // Place them at "now" so they'll expire naturally via the window.
            failure_timestamps: (0..capped_count).map(|_| chrono::Utc::now()).collect(),
            last_health_check: None,
            last_upgrade_attempt: snap
                .last_upgrade_attempt
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
        }
    }

    /// Access the current runtime mode (may have been downgraded by degradation).
    pub const fn current_mode(&self) -> &DomainRuntimeMode {
        &self.current_mode
    }

    /// Access the current degradation state.
    pub const fn degradation_state(&self) -> DegradationState {
        self.degradation_state
    }

    /// Access the current degradation state (convenience alias).
    pub const fn state(&self) -> DegradationState {
        self.degradation_state
    }

    /// Access the degradation policy.
    pub const fn policy(&self) -> &DegradationPolicy {
        &self.policy
    }

    /// Access the last health check result (if any).
    pub const fn last_health_check(&self) -> Option<&HealthCheckResult> {
        self.last_health_check.as_ref()
    }

    /// Access the last upgrade attempt timestamp (if any).
    pub const fn last_upgrade_attempt(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.last_upgrade_attempt
    }

    /// Access the current failure count (number of timestamps in window).
    pub const fn failure_count(&self) -> u32 {
        self.failure_timestamps.len() as u32
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Check if operation requires platform and if degradation allows it.
    /// Returns error if operation is blocked by degradation state.
    ///
    /// This combines the existing `runtime_guard` classification with
    /// degradation-awareness: platform-required operations are blocked
    /// when the current (possibly degraded) mode is `local_only`.
    pub fn check_operation(&self, operation: &str) -> Result<(), DomainError> {
        check_operation(&self.current_mode, operation)
    }

    /// Record a platform operation result.
    /// Updates failure timestamps and potentially triggers degradation.
    pub fn record_platform_result(&mut self, success: bool, error: Option<String>) {
        if success {
            self.failure_timestamps.clear();
            self.update_health_check(true, None, None);
        } else {
            self.failure_timestamps.push(chrono::Utc::now());
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
            self.failure_timestamps.push(chrono::Utc::now());
            if self.should_degrade() {
                self.degrade();
            }
        } else {
            self.failure_timestamps.clear();
        }
    }

    /// Check if we should degrade based on failure threshold within the window.
    ///
    /// Prunes timestamps older than `failure_window_secs`, then checks if
    /// the remaining count meets or exceeds `failure_threshold`.
    fn should_degrade(&self) -> bool {
        let cutoff =
            chrono::Utc::now() - chrono::Duration::seconds(self.policy.failure_window_secs as i64);
        let within_window: Vec<_> = self
            .failure_timestamps
            .iter()
            .filter(|ts| **ts >= cutoff)
            .collect();

        if within_window.len() as u32 >= self.policy.failure_threshold {
            return self
                .degradation_state
                .can_degrade_more(self.policy.max_degradation_depth);
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
            self.failure_timestamps.clear(); // Reset after degradation
        }
    }

    /// Force immediate degradation to `local_only` regardless of threshold.
    pub fn force_local_only(&mut self) {
        self.degradation_state = DegradationState::ForcedLocalOnly;
        self.current_mode = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        self.failure_timestamps.clear();
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
        if self
            .last_health_check
            .as_ref()
            .is_some_and(|h| h.is_healthy)
        {
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
        self.failure_timestamps.clear();
        self.last_upgrade_attempt = None;
    }

    /// Set degradation state directly for testing purposes.
    #[cfg(test)]
    pub(crate) fn set_state_for_testing(&mut self, state: DegradationState) {
        self.degradation_state = state;
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

    // ── DegradationSnapshot tests ──────────────────────────────────────

    #[test]
    fn snapshot_new_creates_default() {
        let snap = DegradationSnapshot::new(DegradationState::Normal, 0);
        assert_eq!(snap.state, DegradationState::Normal);
        assert_eq!(snap.failure_count, 0);
        assert!(snap.last_health_check.is_none());
    }

    #[test]
    fn snapshot_from_guard_captures_state() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let mut guard = DegradationGuard::with_defaults(mode);

        // Record some failures to get non-trivial state
        guard.record_platform_result(false, Some("err".into()));
        guard.record_platform_result(false, Some("err".into()));

        let snap = DegradationSnapshot::from_guard(&guard);
        assert_eq!(snap.state, DegradationState::Normal); // not yet at threshold
        assert_eq!(snap.failure_count, 2);
        assert!(snap.last_health_check.is_some());
        assert!(!snap.last_health_check.as_ref().unwrap().is_healthy);
    }

    #[test]
    fn snapshot_serde_roundtrip() {
        let mut snap = DegradationSnapshot::new(DegradationState::DegradedLevel1, 3);
        snap.last_health_check = Some(HealthCheckSnapshot {
            is_healthy: false,
            checked_at: "2026-04-15T10:30:00Z".to_string(),
        });

        let json = serde_json::to_string(&snap).unwrap();
        let back: DegradationSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
    }

    // ── DegradationState::display_label tests ──────────────────────────

    #[test]
    fn display_label_returns_human_readable() {
        assert_eq!(DegradationState::Normal.display_label(), "Normal");
        assert_eq!(
            DegradationState::DegradedLevel1.display_label(),
            "Degraded (Level 1)"
        );
        assert_eq!(
            DegradationState::DegradedLevel2.display_label(),
            "Degraded (Level 2)"
        );
        assert_eq!(
            DegradationState::ForcedLocalOnly.display_label(),
            "Forced local_only"
        );
    }

    // ── T6.10: Degradation Behavior Tests ────────────────────────────────

    #[test]
    fn degradation_guard_triggers_after_threshold() {
        let policy = DegradationPolicy {
            failure_threshold: 3,
            failure_window_secs: 60,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("cloud_enhanced").unwrap());

        // Record 3 failures within window
        guard.record_platform_result(false, Some("timeout".into()));
        guard.record_platform_result(false, Some("timeout".into()));
        guard.record_platform_result(false, Some("timeout".into()));

        assert_eq!(guard.state(), DegradationState::DegradedLevel1);
        assert_eq!(guard.current_mode().to_string(), "local_first");
    }

    #[test]
    fn degradation_reset_after_success() {
        let policy = DegradationPolicy {
            failure_threshold: 2,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("cloud_enhanced").unwrap());

        guard.record_platform_result(false, Some("err".into()));
        guard.record_platform_result(false, Some("err".into()));
        assert_eq!(guard.state(), DegradationState::DegradedLevel1);

        // Success resets failure count
        guard.record_platform_result(true, None);
        assert_eq!(guard.failure_count(), 0);
    }

    #[test]
    fn upgrade_blocked_without_cooldown() {
        // When cooldown is 0, upgrade is blocked if no healthy platform check
        let policy = DegradationPolicy {
            upgrade_cooldown_secs: 0,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("local_first").unwrap());
        guard.set_state_for_testing(DegradationState::DegradedLevel1);

        // Try upgrade without healthy check - blocked
        // (upgrade_cooldown_secs is 0 but last_upgrade_attempt is None, so this passes)
        // Then it checks last_health_check which is None - so no upgrade
        // Actually with cooldown=0, the upgrade will proceed if health is healthy
        // Since we haven't recorded a health check, last_health_check is None
        // and the upgrade fails because platform is not confirmed healthy
        assert!(!guard.try_upgrade());
    }

    #[test]
    fn max_degradation_depth_respects_policy() {
        let policy = DegradationPolicy {
            max_degradation_depth: 1, // Only one level allowed
            failure_threshold: 1,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("cloud_enhanced").unwrap());

        guard.record_platform_result(false, Some("err".into()));
        assert_eq!(guard.state(), DegradationState::DegradedLevel1);

        // Cannot degrade further (policy depth = 1)
        guard.record_platform_result(false, Some("err".into()));
        assert_ne!(guard.state(), DegradationState::DegradedLevel2); // stays at level 1
    }

    // ── R3: last_upgrade_attempt persistence tests ─────────────────────

    #[test]
    fn snapshot_from_guard_preserves_last_upgrade_attempt() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let policy = DegradationPolicy {
            upgrade_cooldown_secs: 0,
            ..Default::default()
        };
        let mut guard = DegradationGuard::new(policy, mode);

        // Degrade and then upgrade to set last_upgrade_attempt
        for _ in 0..3 {
            guard.record_platform_result(false, Some("fail".into()));
        }
        guard.record_platform_result(true, None);
        assert!(guard.try_upgrade());

        // Verify the guard has a last_upgrade_attempt
        assert!(guard.last_upgrade_attempt().is_some());

        // Snapshot should capture it
        let snap = DegradationSnapshot::from_guard(&guard);
        assert!(snap.last_upgrade_attempt.is_some());
    }

    #[test]
    fn restore_from_snapshot_restores_last_upgrade_attempt() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let iso_time = "2026-04-16T10:30:00+00:00";

        let snap = DegradationSnapshot::with_upgrade_attempt(
            DegradationState::DegradedLevel1,
            2,
            Some(iso_time.to_string()),
        );

        let guard = DegradationGuard::restore_from_snapshot(&snap, mode);

        // last_upgrade_attempt should be restored
        let restored = guard
            .last_upgrade_attempt()
            .expect("last_upgrade_attempt should be restored");
        assert_eq!(restored.to_rfc3339(), iso_time);
    }

    #[test]
    fn snapshot_without_last_upgrade_attempt_deserializes_as_none() {
        // Old-format JSON without last_upgrade_attempt field
        let json = r#"{"state":"DegradedLevel1","failure_count":2}"#;
        let snap: DegradationSnapshot = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(snap.state, DegradationState::DegradedLevel1);
        assert_eq!(snap.failure_count, 2);
        assert!(snap.last_upgrade_attempt.is_none());
    }

    #[test]
    fn snapshot_with_upgrade_attempt_serde_roundtrip() {
        let snap = DegradationSnapshot::with_upgrade_attempt(
            DegradationState::DegradedLevel1,
            3,
            Some("2026-04-16T10:30:00+00:00".to_string()),
        );

        let json = serde_json::to_string(&snap).unwrap();
        let back: DegradationSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
        assert_eq!(
            back.last_upgrade_attempt.as_deref(),
            Some("2026-04-16T10:30:00+00:00")
        );
    }

    // ── R1: Window-based failure timestamp tests ───────────────────────

    #[test]
    fn three_failures_within_window_triggers_degradation() {
        let policy = DegradationPolicy {
            failure_threshold: 3,
            failure_window_secs: 5,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("cloud_enhanced").unwrap());

        guard.record_platform_result(false, Some("fail 1".into()));
        assert_eq!(guard.degradation_state(), DegradationState::Normal);

        guard.record_platform_result(false, Some("fail 2".into()));
        assert_eq!(guard.degradation_state(), DegradationState::Normal);

        guard.record_platform_result(false, Some("fail 3".into()));
        // All 3 failures within 5s window → should degrade
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);
    }

    #[test]
    fn failure_outside_window_does_not_count() {
        // Use a very short window (2s) so we can reliably test time-based expiry
        let policy = DegradationPolicy {
            failure_threshold: 3,
            failure_window_secs: 2,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("cloud_enhanced").unwrap());

        // Record 2 failures now
        guard.record_platform_result(false, Some("fail 1".into()));
        guard.record_platform_result(false, Some("fail 2".into()));
        assert_eq!(guard.failure_count(), 2);
        assert_eq!(guard.degradation_state(), DegradationState::Normal);

        // Wait for the window to expire
        std::thread::sleep(std::time::Duration::from_millis(2100));

        // Record 1 more failure. The 2 old failures are now outside the window.
        guard.record_platform_result(false, Some("fail 3".into()));

        // Only 1 failure within the window → should NOT degrade
        assert_eq!(
            guard.degradation_state(),
            DegradationState::Normal,
            "should not degrade: only 1 failure within window"
        );
    }

    #[test]
    fn failure_just_at_window_boundary_counts() {
        // Use a 1s window. 3 rapid failures should all be within window.
        let policy = DegradationPolicy {
            failure_threshold: 3,
            failure_window_secs: 1,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("cloud_enhanced").unwrap());

        guard.record_platform_result(false, Some("fail 1".into()));
        guard.record_platform_result(false, Some("fail 2".into()));
        guard.record_platform_result(false, Some("fail 3".into()));

        // All 3 within 1s window → degrades
        assert_eq!(guard.degradation_state(), DegradationState::DegradedLevel1);
    }

    #[test]
    fn success_clears_all_failure_timestamps() {
        let policy = DegradationPolicy {
            failure_threshold: 3,
            failure_window_secs: 60,
            ..Default::default()
        };
        let mut guard =
            DegradationGuard::new(policy, DomainRuntimeMode::parse("cloud_enhanced").unwrap());

        guard.record_platform_result(false, Some("fail 1".into()));
        guard.record_platform_result(false, Some("fail 2".into()));
        assert_eq!(guard.failure_count(), 2);

        // Success clears all timestamps
        guard.record_platform_result(true, None);
        assert_eq!(guard.failure_count(), 0);

        // Need a full 3 new failures to trigger degradation
        guard.record_platform_result(false, Some("fail 3".into()));
        assert_eq!(guard.degradation_state(), DegradationState::Normal);
    }

    // ── S-003: Send + Sync verification tests ───────────────────────────

    #[test]
    fn degradation_guard_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<DegradationGuard>();
    }

    #[test]
    fn degradation_guard_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<DegradationGuard>();
    }

    // ── QC1-W1: restored failures at threshold do not trigger immediate re-degradation ─

    #[test]
    fn restored_failures_at_threshold_do_not_trigger_immediate_degradation() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let threshold = DegradationPolicy::default().failure_threshold;

        // Restore with exactly failure_threshold failures
        let snap = DegradationSnapshot::new(DegradationState::Normal, threshold);
        let guard = DegradationGuard::restore_from_snapshot(&snap, mode);

        // The guard should NOT want to degrade immediately
        assert!(
            !guard.should_degrade(),
            "restored guard with failure_threshold failures should not trigger immediate degradation"
        );

        // Verify the failure count is capped to threshold - 1
        assert_eq!(
            guard.failure_count(),
            threshold - 1,
            "restored synthetic timestamps should be capped to threshold - 1"
        );
    }

    #[test]
    fn restored_failures_below_threshold_preserved() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);

        // Restore with fewer failures than threshold
        let snap = DegradationSnapshot::new(DegradationState::Normal, 1);
        let guard = DegradationGuard::restore_from_snapshot(&snap, mode);

        assert_eq!(guard.failure_count(), 1);
        assert!(!guard.should_degrade());
    }

    #[test]
    fn restored_with_zero_failures_has_no_timestamps() {
        let mode = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);

        let snap = DegradationSnapshot::new(DegradationState::Normal, 0);
        let guard = DegradationGuard::restore_from_snapshot(&snap, mode);

        assert_eq!(guard.failure_count(), 0);
        assert!(!guard.should_degrade());
    }
}
