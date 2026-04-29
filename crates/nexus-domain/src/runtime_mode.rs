//! Runtime mode domain model.
//!
//! Wraps the `RuntimeMode` enum (defined in `nexus-contracts::enum_conversions`)
//! with domain-level convenience methods for mode-aware operation checks.

use crate::DomainError;
use nexus_contracts::local::domain::RuntimeMode;
use std::fmt;
use std::str::FromStr as _;

/// Domain wrapper for `RuntimeMode` with additional constraint queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DomainRuntimeMode(pub RuntimeMode);

impl DomainRuntimeMode {
    /// V1.2 default runtime mode.
    pub const DEFAULT: DomainRuntimeMode = DomainRuntimeMode(RuntimeMode::LocalOnly);
    #[must_use]
    /// Create from generated enum.
    pub const fn new(mode: RuntimeMode) -> Self {
        Self(mode)
    }

    /// Access the inner generated enum.
    #[must_use]
    pub const fn inner(&self) -> &RuntimeMode {
        &self.0
    }

    /// Whether this mode completely prohibits platform HTTP dependencies.
    /// Only `local_only` returns true.
    #[must_use]
    pub const fn is_local_only(&self) -> bool {
        matches!(self.0, RuntimeMode::LocalOnly)
    }
    #[must_use]
    /// Whether this mode allows optional platform use.
    /// `local_first` and `cloud_enhanced` return true.
    pub fn allows_platform(&self) -> bool {
        !self.is_local_only()
    }

    /// Whether this mode allows platform LLM on behalf of Creator.
    /// Only `cloud_enhanced` returns true (ADR-015 D1).
    #[must_use]
    pub const fn allows_platform_llm(&self) -> bool {
        matches!(self.0, RuntimeMode::CloudEnhanced)
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Parse from string (matches JSON Schema enum values).
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        RuntimeMode::from_str(s)
            .map(Self::new)
            .map_err(DomainError::ValidationError)
    }

    /// Downgrade to a more local-first mode.
    /// Chain: `cloud_enhanced` → `local_first` → local_only
    /// Returns None if already at `local_only` (cannot downgrade further).
    #[must_use]
    pub const fn downgrade(&self) -> Option<DomainRuntimeMode> {
        match self.0 {
            RuntimeMode::CloudEnhanced => Some(DomainRuntimeMode(RuntimeMode::LocalFirst)),
            RuntimeMode::LocalFirst => Some(DomainRuntimeMode(RuntimeMode::LocalOnly)),
            RuntimeMode::LocalOnly => None,
        }
    }

    /// Upgrade to a more cloud-enhanced mode.
    /// Chain: `local_only` → `local_first` → cloud_enhanced
    /// Returns None if already at `cloud_enhanced` (cannot upgrade further).
    #[must_use]
    pub const fn upgrade(&self) -> Option<DomainRuntimeMode> {
        match self.0 {
            RuntimeMode::LocalOnly => Some(DomainRuntimeMode(RuntimeMode::LocalFirst)),
            RuntimeMode::LocalFirst => Some(DomainRuntimeMode(RuntimeMode::CloudEnhanced)),
            RuntimeMode::CloudEnhanced => None,
        }
    }

    /// Calculate degradation depth from a target mode.
    /// 0 = same mode, 1 = one level down, 2 = two levels down.
    pub const fn degradation_depth_to(&self, target: &DomainRuntimeMode) -> u32 {
        match (self.0, target.0) {
            (RuntimeMode::CloudEnhanced, RuntimeMode::CloudEnhanced) => 0,
            (RuntimeMode::CloudEnhanced, RuntimeMode::LocalFirst) => 1,
            (RuntimeMode::CloudEnhanced, RuntimeMode::LocalOnly) => 2,
            (RuntimeMode::LocalFirst, RuntimeMode::LocalFirst) => 0,
            (RuntimeMode::LocalFirst, RuntimeMode::LocalOnly) => 1,
            (RuntimeMode::LocalOnly, RuntimeMode::LocalOnly) => 0,
            _ => 0, // Upgrades or invalid combinations
        }
    }
}

impl fmt::Display for DomainRuntimeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::default::Default for DomainRuntimeMode {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<RuntimeMode> for DomainRuntimeMode {
    fn from(mode: RuntimeMode) -> Self {
        Self(mode)
    }
}

impl From<DomainRuntimeMode> for RuntimeMode {
    fn from(domain: DomainRuntimeMode) -> Self {
        domain.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_local_only() {
        assert!(DomainRuntimeMode::default().is_local_only());
    }

    #[test]
    fn local_only_prohibits_platform() {
        let mode = DomainRuntimeMode::parse("local_only").unwrap();
        assert!(mode.is_local_only());
        assert!(!mode.allows_platform());
        assert!(!mode.allows_platform_llm());
    }

    #[test]
    fn local_first_allows_platform_but_not_llm() {
        let mode = DomainRuntimeMode::parse("local_first").unwrap();
        assert!(!mode.is_local_only());
        assert!(mode.allows_platform());
        assert!(!mode.allows_platform_llm());
    }

    #[test]
    fn cloud_enhanced_allows_all() {
        let mode = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        assert!(!mode.is_local_only());
        assert!(mode.allows_platform());
        assert!(mode.allows_platform_llm());
    }

    #[test]
    fn invalid_string_returns_error() {
        let result = DomainRuntimeMode::parse("invalid_mode");
        assert!(result.is_err());
    }

    #[test]
    fn display_matches_schema_values() {
        assert_eq!(
            DomainRuntimeMode::parse("local_only").unwrap().to_string(),
            "local_only"
        );
        assert_eq!(
            DomainRuntimeMode::parse("local_first").unwrap().to_string(),
            "local_first"
        );
        assert_eq!(
            DomainRuntimeMode::parse("cloud_enhanced")
                .unwrap()
                .to_string(),
            "cloud_enhanced"
        );
    }

    #[test]
    fn downgrade_chain() {
        let cloud = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let local_first = cloud.downgrade().unwrap();
        assert_eq!(local_first, DomainRuntimeMode::new(RuntimeMode::LocalFirst));

        let local_only = local_first.downgrade().unwrap();
        assert_eq!(local_only, DomainRuntimeMode::new(RuntimeMode::LocalOnly));

        // Cannot downgrade from local_only
        assert!(local_only.downgrade().is_none());
    }

    #[test]
    fn upgrade_chain() {
        let local_only = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        let local_first = local_only.upgrade().unwrap();
        assert_eq!(local_first, DomainRuntimeMode::new(RuntimeMode::LocalFirst));

        let cloud = local_first.upgrade().unwrap();
        assert_eq!(cloud, DomainRuntimeMode::new(RuntimeMode::CloudEnhanced));

        // Cannot upgrade from cloud_enhanced
        assert!(cloud.upgrade().is_none());
    }

    #[test]
    fn downgrade_upgrade_are_inverses() {
        let cloud = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let downgraded = cloud.downgrade().unwrap();
        let upgraded = downgraded.upgrade().unwrap();
        assert_eq!(cloud, upgraded);

        let local_only = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        let upgraded = local_only.upgrade().unwrap();
        let downgraded = upgraded.downgrade().unwrap();
        assert_eq!(local_only, downgraded);
    }

    #[test]
    fn degradation_depth_same_mode() {
        for mode_str in ["local_only", "local_first", "cloud_enhanced"] {
            let mode = DomainRuntimeMode::parse(mode_str).unwrap();
            assert_eq!(
                mode.degradation_depth_to(&mode),
                0,
                "same mode should have depth 0: {mode_str}"
            );
        }
    }

    #[test]
    fn degradation_depth_one_level() {
        let cloud = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let local_first = DomainRuntimeMode::new(RuntimeMode::LocalFirst);
        let local_only = DomainRuntimeMode::new(RuntimeMode::LocalOnly);

        assert_eq!(cloud.degradation_depth_to(&local_first), 1);
        assert_eq!(local_first.degradation_depth_to(&local_only), 1);
    }

    #[test]
    fn degradation_depth_two_levels() {
        let cloud = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        let local_only = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        assert_eq!(cloud.degradation_depth_to(&local_only), 2);
    }

    #[test]
    fn degradation_depth_upgrade_returns_zero() {
        let local_only = DomainRuntimeMode::new(RuntimeMode::LocalOnly);
        let cloud = DomainRuntimeMode::new(RuntimeMode::CloudEnhanced);
        // Upgrading (reverse direction) should return 0
        assert_eq!(local_only.degradation_depth_to(&cloud), 0);
    }

    // ── T6.9: Mode Switch Test Cases ─────────────────────────────────────

    #[test]
    fn mode_switch_chain_cloud_to_local_only() {
        let cloud = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        let local_first = cloud.downgrade().unwrap();
        assert_eq!(local_first.to_string(), "local_first");
        let local_only = local_first.downgrade().unwrap();
        assert_eq!(local_only.to_string(), "local_only");
        assert!(local_only.downgrade().is_none()); // floor
    }

    #[test]
    fn mode_upgrade_chain_local_only_to_cloud() {
        let local_only = DomainRuntimeMode::parse("local_only").unwrap();
        let local_first = local_only.upgrade().unwrap();
        assert_eq!(local_first.to_string(), "local_first");
        let cloud = local_first.upgrade().unwrap();
        assert_eq!(cloud.to_string(), "cloud_enhanced");
        assert!(cloud.upgrade().is_none()); // ceiling
    }

    #[test]
    fn degradation_depth_calculation() {
        let cloud = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        let local_first = DomainRuntimeMode::parse("local_first").unwrap();
        let local_only = DomainRuntimeMode::parse("local_only").unwrap();

        assert_eq!(cloud.degradation_depth_to(&cloud), 0);
        assert_eq!(cloud.degradation_depth_to(&local_first), 1);
        assert_eq!(cloud.degradation_depth_to(&local_only), 2);
        assert_eq!(local_first.degradation_depth_to(&local_only), 1);
    }

    #[test]
    fn mode_switch_preserves_constraints() {
        // Cloud enhanced: allows platform + allows LLM
        let cloud = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        assert!(cloud.allows_platform());
        assert!(cloud.allows_platform_llm());

        // Local first: allows platform, denies LLM
        let degraded = cloud.downgrade().unwrap();
        assert!(degraded.allows_platform());
        assert!(!degraded.allows_platform_llm());

        // Local only: denies both
        let floor = degraded.downgrade().unwrap();
        assert!(!floor.allows_platform());
        assert!(!floor.allows_platform_llm());
    }
}
