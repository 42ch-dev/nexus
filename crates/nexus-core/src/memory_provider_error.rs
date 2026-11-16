//! The provider's own error taxonomy projected onto the core's (v1.245).
//!
//! `nexus-memory-provider` returns `ProviderError` from the entry points that are
//! not wire ops — the opens, the legacy migration pass, the review-disposition
//! section and the SOUL-invalidation recovery row — and names no core type. This
//! `From` impl is the whole boundary: a core call site converts with `?` and no
//! caller matches on `ProviderError` itself.
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

impl From<ProviderError> for CoreError {
    fn from(error: ProviderError) -> Self {
        match error {
            ProviderError::Internal(reason) => Self::Internal { category: reason },
            // A read-only provider refuses the review-disposition section with
            // the resource that section names: the retained `Forbidden` shape.
            ProviderError::ReadOnly(reason) => Self::Forbidden { resource: reason },
            ProviderError::NotAuthorized(reason) => Self::ForbiddenReason {
                resource: "memory: migrate-legacy".to_string(),
                reason,
            },
            ProviderError::IdempotencyConflict => Self::ActorConflict {
                code: IDEMPOTENCY_CONFLICT_CODE.to_string(),
                message: error.message(),
            },
            ProviderError::NotFound(_) => Self::NotFound {
                resource: error.message(),
            },
            other => Self::Internal {
                category: other.message(),
            },
        }
    }
}
