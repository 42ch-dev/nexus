//! Runtime mode guard functions.
//!
//! Provides fail-fast checks for operations that require platform connectivity.
//! Under `local_only`, any platform-dependent operation returns an error immediately.
//!
//! Usage:
//! ```ignore
//! runtime_guard::require_platform(mode, "sync")?;
//! ```

use crate::{DomainError, DomainRuntimeMode};

/// Categories of CLI operations for mode-aware routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationCategory {
    /// Always available regardless of mode (local workspace, identity, db, agent, session, policy).
    LocalOnly,
    /// Requires platform connectivity (sync, publish, auth login, context assemble, explore).
    PlatformRequired,
    /// Available in `local_only` but enhanced by platform (creator manage, world fork, manuscript read).
    LocalWithPlatformEnhancement,
}
#[must_use]
/// Classify a named operation into its category.
/// This is the single source of truth for what requires platform.
pub fn classify_operation(operation: &str) -> OperationCategory {
    match operation {
        // Local-only operations (always available)
        "init" | "db" | "identity" | "daemon" | "agent" | "session" | "policy" | "runtime_mode" => {
            OperationCategory::LocalOnly
        }

        // Platform-required operations (blocked in local_only)
        "sync" | "publish" | "auth_login" | "auth_register" | "context_assemble" | "explore" => {
            OperationCategory::PlatformRequired
        }

        // Local with platform enhancement (available in local_only, enhanced in other modes)
        "creator" | "world" | "manuscript" | "research" => {
            OperationCategory::LocalWithPlatformEnhancement
        }

        // V1.2 residual R4 (runtime, nit): fail-open default for unknown ops
        // Unknown operations fail-open (allow) as conservative default; log warning
        _ => OperationCategory::LocalOnly,
    }
}

/// Check that the current mode allows a platform-dependent operation.
/// Returns `Err(PlatformOperationProhibited)` if the mode is `local_only`.
///
/// # Arguments
/// * `mode` - Current runtime mode
/// * `operation` - Human-readable operation name (used in error message)
pub fn require_platform(mode: &DomainRuntimeMode, operation: &str) -> Result<(), DomainError> {
    if mode.is_local_only() {
        return Err(DomainError::PlatformOperationProhibited {
            mode: mode.to_string(),
            operation: operation.to_string(),
        });
    }
    Ok(())
}
///
/// # Errors
/// Returns `Err(DomainError::...)` if validation fails.
///
/// # Errors
/// Returns `Err(DomainError::...)` if validation fails.
/// Check that the current mode allows an operation by its category.
pub fn check_operation(mode: &DomainRuntimeMode, operation: &str) -> Result<(), DomainError> {
    match classify_operation(operation) {
        OperationCategory::LocalOnly => Ok(()),
        OperationCategory::PlatformRequired => require_platform(mode, operation),
        OperationCategory::LocalWithPlatformEnhancement => Ok(()),
    }
}

/// Returns a list of operation categories that are blocked under the given mode.
#[must_use]
pub fn blocked_categories(mode: &DomainRuntimeMode) -> Vec<OperationCategory> {
    if mode.is_local_only() {
        vec![OperationCategory::PlatformRequired]
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_blocks_sync() {
        let mode = DomainRuntimeMode::parse("local_only").unwrap();
        assert!(require_platform(&mode, "sync").is_err());
    }

    #[test]
    fn local_first_allows_sync() {
        let mode = DomainRuntimeMode::parse("local_first").unwrap();
        assert!(require_platform(&mode, "sync").is_ok());
    }

    #[test]
    fn cloud_enhanced_allows_sync() {
        let mode = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        assert!(require_platform(&mode, "sync").is_ok());
    }

    #[test]
    fn classify_known_operations() {
        assert_eq!(
            classify_operation("sync"),
            OperationCategory::PlatformRequired
        );
        assert_eq!(
            classify_operation("publish"),
            OperationCategory::PlatformRequired
        );
        assert_eq!(classify_operation("identity"), OperationCategory::LocalOnly);
        assert_eq!(classify_operation("agent"), OperationCategory::LocalOnly);
        assert_eq!(
            classify_operation("creator"),
            OperationCategory::LocalWithPlatformEnhancement
        );
    }

    #[test]
    fn check_operation_passes_for_local_ops_in_any_mode() {
        for mode_str in ["local_only", "local_first", "cloud_enhanced"] {
            let mode = DomainRuntimeMode::parse(mode_str).unwrap();
            assert!(
                check_operation(&mode, "identity").is_ok(),
                "mode: {mode_str}"
            );
        }
    }

    #[test]
    fn check_operation_blocks_platform_in_local_only() {
        let mode = DomainRuntimeMode::parse("local_only").unwrap();
        assert!(check_operation(&mode, "sync").is_err());
        assert!(check_operation(&mode, "publish").is_err());
    }

    #[test]
    fn blocked_categories_local_only() {
        let mode = DomainRuntimeMode::parse("local_only").unwrap();
        let blocked = blocked_categories(&mode);
        assert!(blocked.contains(&OperationCategory::PlatformRequired));
    }

    #[test]
    fn blocked_categories_cloud_enhanced() {
        let mode = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        let blocked = blocked_categories(&mode);
        assert!(blocked.is_empty());
    }
}
