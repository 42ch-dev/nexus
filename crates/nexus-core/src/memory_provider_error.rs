//! The provider's own error taxonomy projected onto the core's (v1.245).
//!
//! `nexus-memory-provider` returns `ProviderError` from the entry points that are
//! not wire ops — the opens, the legacy migration pass, the review-disposition
//! section and the SOUL-invalidation recovery row — and names no core type. The
//! `From` impl and [`provider_error_with_resource`] are the whole boundary: a
//! core call site converts with `?` and no caller matches on `ProviderError`
//! itself.
//!
//! The `NotAuthorized` resource belongs to the call site, not to the provider
//! variant (I-000507): the shared `From` label only fits the legacy migration,
//! so every other entry converts through [`provider_error_with_resource`] with
//! the resource it controls.
//!
//! The projection is deliberately not one-to-one, because the provider's
//! taxonomy is narrower than the core's and two of its classes carry a retained
//! meaning a caller acts on: a missing `admin` grant reads `forbidden`, and a
//! conflicting adopted body reads `idempotency_conflict`, so an operator can act
//! on a migration refusal instead of seeing one flat internal error.

use nexus_memory_provider::ProviderError;

use crate::error::CoreError;

/// The conflict code a migration pass reports when its recorded manifest no
/// longer matches the legacy source, or when an adopted object's stored body
/// differs from it.
const IDEMPOTENCY_CONFLICT_CODE: &str = "idempotency_conflict";

/// The resource the provider open names when it refuses.
pub const PROVIDER_OPEN_RESOURCE: &str = "memory: provider-open";

/// The resource the review-disposition section names when it refuses.
pub const REVIEW_DISPOSITION_RESOURCE: &str = "memory: review-disposition";

/// The resource the SOUL-invalidation recovery row names when it refuses.
pub const SOUL_RECOVERY_RESOURCE: &str = "memory: soul-recovery";

/// The resource the legacy migration pass names when it refuses.
pub const LEGACY_MIGRATION_RESOURCE: &str = "memory: migrate-legacy";

/// Project one provider refusal naming the core entry point that raised it.
///
/// `NotAuthorized` carries no resource in the provider's taxonomy, so the entry
/// point supplies its own; every other variant keeps the shared projection.
pub fn provider_error_with_resource(error: ProviderError, resource: &str) -> CoreError {
    match error {
        ProviderError::NotAuthorized(reason) => CoreError::ForbiddenReason {
            resource: resource.to_string(),
            reason,
        },
        other => other.into(),
    }
}

impl From<ProviderError> for CoreError {
    fn from(error: ProviderError) -> Self {
        match error {
            ProviderError::Internal(reason) => Self::Internal { category: reason },
            ProviderError::ReadOnly(reason) => Self::Forbidden { resource: reason },
            ProviderError::NotAuthorized(reason) => Self::ForbiddenReason {
                resource: LEGACY_MIGRATION_RESOURCE.to_string(),
                reason,
            },
            ProviderError::IdempotencyConflict => Self::ActorConflict {
                code: IDEMPOTENCY_CONFLICT_CODE.to_string(),
                message: error.message(),
            },
            ProviderError::NotFound(target) => Self::NotFound { resource: target },
            other => Self::Internal {
                category: other.message(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_review_disposition_refusal_names_its_own_resource() {
        let error = provider_error_with_resource(
            ProviderError::NotAuthorized("principal holds no grant".to_string()),
            REVIEW_DISPOSITION_RESOURCE,
        );
        match error {
            CoreError::ForbiddenReason { resource, reason } => {
                assert_eq!(resource, "memory: review-disposition");
                assert_eq!(reason, "principal holds no grant");
            }
            other => panic!("expected a forbidden reason, got {other:?}"),
        }
    }

    #[test]
    fn a_soul_recovery_refusal_names_its_own_resource() {
        let error = provider_error_with_resource(
            ProviderError::NotAuthorized("the recovery row is unreadable".to_string()),
            SOUL_RECOVERY_RESOURCE,
        );
        match error {
            CoreError::ForbiddenReason { resource, reason } => {
                assert_eq!(resource, "memory: soul-recovery");
                assert_eq!(reason, "the recovery row is unreadable");
            }
            other => panic!("expected a forbidden reason, got {other:?}"),
        }
    }

    #[test]
    fn the_shared_conversion_keeps_the_migration_label() {
        let error = CoreError::from(ProviderError::NotAuthorized("no admin grant".to_string()));
        match error {
            CoreError::ForbiddenReason { resource, .. } => {
                assert_eq!(resource, "memory: migrate-legacy");
            }
            other => panic!("expected a forbidden reason, got {other:?}"),
        }
    }
}
