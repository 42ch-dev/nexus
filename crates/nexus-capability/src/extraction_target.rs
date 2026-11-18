//! Trusted extraction target (v1.191 P1 T13).
//!
//! Moved verbatim from `nexus-orchestration/src/quality_loop.rs`; the
//! review-time hook in `quality_loop` and the extraction builtins consume the
//! same single implementation — no second resolver exists.

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

impl ExtractionTarget {
    /// The trusted-context JSON field the extraction capability consumes.
    #[must_use]
    pub fn context_value(&self) -> serde_json::Value {
        serde_json::json!({
            "world_id": self.world_id,
            "holder_entry_id": self.governance.holder_entry_id,
            "disclosure": self.governance.disclosure,
        })
    }

    /// Parse a trusted target out of its context value.
    ///
    /// Fails closed on an empty container or a malformed native governance
    /// pair, so a caller cannot hand the extractor an invented policy.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityError::Forbidden`] for a missing/empty `world_id` or
    /// a pair the domain rules refuse.
    pub fn from_context(value: &serde_json::Value) -> Result<Self, CapabilityError> {
        let world_id = value.get("world_id").and_then(|v| v.as_str()).unwrap_or("");
        if world_id.is_empty() {
            return Err(CapabilityError::Forbidden(
                "extraction target world_id must be non-empty".to_string(),
            ));
        }
        let holder_entry_id = value.get("holder_entry_id").and_then(|v| v.as_str());
        let disclosure = value.get("disclosure").and_then(|v| v.as_str());
        nexus_knowledge::world_kb::validate_native_governance(holder_entry_id, disclosure)
            .map_err(|e| {
                CapabilityError::Forbidden(format!("extraction target governance: {e}"))
            })?;
        Ok(Self {
            world_id: world_id.to_string(),
            governance: KnowledgeGovernance {
                holder_entry_id: holder_entry_id.map(str::to_owned),
                disclosure: disclosure.map(str::to_owned),
            },
        })
    }
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
