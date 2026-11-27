//! Trusted extraction target (v1.191 P1 T13).
//!
//! [`resolve_extraction_target`] is the only resolver of an
//! [`ExtractionTarget`]: it rechecks stored ownership and applies the
//! service's audience policy. No JSON parser turns caller input into a target;
//! callers may name the World, never the target. The review-time hook resolves
//! before the model runs, and the `nexus.llm.extract` capability re-resolves
//! the World id it is handed, so preset/client/model-supplied JSON cannot
//! become a target on its own.

use crate::CapabilityError;
use nexus_knowledge::world_kb::KnowledgeGovernance;
use sqlx::SqlitePool;

/// `holder-product.md` §11).
///
/// Resolved from stored state by [`resolve_extraction_target`] before the
/// model runs, then threaded to the extraction capability as trusted context.
/// Model output never reaches either field, so the LLM cannot mark an author
/// secret shared and cannot name someone else's holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionTarget {
    /// The World container the extracted entries belong to.
    pub world_id: String,
    /// The governance pair the service decided for this target.
    pub governance: KnowledgeGovernance,
}

/// Resolve the trusted extraction target of one production run from stored
/// state (durable §§3, 8).
///
/// Two trusted steps, both outside the model's reach:
///
/// 1. **Stored-state recheck.** The target World must still exist and still be
///    owned by the stored controlling Creator. A stale admission (foreign or
///    missing World) refuses instead of extracting into a container the run is
///    no longer authorized for.
/// 2. **Audience resolution.** The service chooses the audience of an
///    extraction target — `holder-product.md` §11 fixes it to the in-scope
///    shared default, because no extraction surface carries an audience
///    selector — and resolves the native pair through the same domain rule an
///    authored write uses, so a private extraction target would resolve its
///    holder the same way (never from a client or model string).
///
/// # Errors
///
/// Returns [`CapabilityError::Forbidden`] for an empty identity or an
/// unowned/missing World, and [`CapabilityError::Internal`] on database
/// failure or a resolution the domain rule refuses.
pub async fn resolve_extraction_target(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> Result<ExtractionTarget, CapabilityError> {
    if creator_id.is_empty() || world_id.is_empty() {
        return Err(CapabilityError::Forbidden(
            "extraction target requires a stored controlling creator and World".into(),
        ));
    }
    match nexus_local_db::narrative_write::is_world_owned(pool, creator_id, world_id).await {
        Ok(true) => {}
        Ok(false) => {
            return Err(CapabilityError::Forbidden(format!(
                "extraction target World {world_id} is missing or not owned by creator \
                 {creator_id}; refusing to extract"
            )));
        }
        Err(e) => {
            return Err(CapabilityError::Internal(format!(
                "extraction target ownership recheck: {e}"
            )));
        }
    }

    let audience = nexus_knowledge::world_kb::KnowledgeAudience::Shared;
    let governance = nexus_knowledge::world_kb::resolve_authored_governance(
        nexus_knowledge::world_kb::KnowledgeAuthoringOp::Create,
        Some(&audience),
        None,
    )
    .map_err(|e| CapabilityError::Internal(format!("extraction target audience: {e}")))?
    .ok_or_else(|| {
        CapabilityError::Internal(
            "the in-scope extraction audience resolved no governance pair".into(),
        )
    })?;

    Ok(ExtractionTarget {
        world_id: world_id.to_string(),
        governance,
    })
}
