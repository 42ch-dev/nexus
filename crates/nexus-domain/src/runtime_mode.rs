//! Runtime mode domain model.
//!
//! Wraps the `RuntimeMode` enum (defined in `nexus-contracts::enum_conversions`)
//! with domain-level convenience methods for mode-aware operation checks.

use crate::DomainError;
use nexus_contracts::RuntimeMode;
use std::fmt;
use std::str::FromStr as _;

/// Domain wrapper for RuntimeMode with additional constraint queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DomainRuntimeMode(pub RuntimeMode);

impl DomainRuntimeMode {
    /// V1.2 default runtime mode.
    pub const DEFAULT: DomainRuntimeMode = DomainRuntimeMode(RuntimeMode::LocalOnly);

    /// Create from generated enum.
    pub const fn new(mode: RuntimeMode) -> Self {
        Self(mode)
    }

    /// Access the inner generated enum.
    pub const fn inner(&self) -> &RuntimeMode {
        &self.0
    }

    /// Whether this mode completely prohibits platform HTTP dependencies.
    /// Only `local_only` returns true.
    pub fn is_local_only(&self) -> bool {
        matches!(self.0, RuntimeMode::LocalOnly)
    }

    /// Whether this mode allows optional platform use.
    /// `local_first` and `cloud_enhanced` return true.
    pub fn allows_platform(&self) -> bool {
        !self.is_local_only()
    }

    /// Whether this mode allows platform LLM on behalf of Creator.
    /// Only `cloud_enhanced` returns true (ADR-015 D1).
    pub fn allows_platform_llm(&self) -> bool {
        matches!(self.0, RuntimeMode::CloudEnhanced)
    }

    /// Parse from string (matches JSON Schema enum values).
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        RuntimeMode::from_str(s)
            .map(Self::new)
            .map_err(DomainError::ValidationError)
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
}
