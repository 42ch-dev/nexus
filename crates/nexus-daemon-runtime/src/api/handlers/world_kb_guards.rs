//! Shared World KB ownership guards (V1.155 P2 T3, R-V1152P0-002).
//!
//! Deduplicated from `world_kb.rs` / `world_kb_pack.rs` — both handlers
//! enforce the same tier2-plus ownership gate for World-scoped KB routes:
//! active-creator resolution + `narrative_worlds.owner_creator_id` check
//! (404 missing world / 403 cross-author / 403 unowned world). A third
//! pack-adjacent handler must reuse these instead of copying them again.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::workspace::WorkspaceState;

/// Read the active creator id or return `AuthRequired`.
pub(crate) fn require_creator(state: &WorkspaceState) -> Result<String, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;
    Ok(creator_id)
}

/// Verify the active creator owns the World (`narrative_worlds.owner_creator_id`).
/// Returns 404 when the world is missing, 403 on cross-author access.
pub(crate) async fn require_world_owner(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    creator_id: &str,
) -> Result<(), NexusApiError> {
    // SAFETY: SELECT against the known narrative_worlds table schema.
    let owner: Option<Option<String>> =
        sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(NexusApiError::from)?;
    match owner {
        None => Err(NexusApiError::NotFound(format!("world {world_id}"))),
        Some(Some(owner_id)) if owner_id == creator_id => Ok(()),
        Some(Some(_)) => Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason:
                "active creator does not own this world; cross-author World KB edits are forbidden"
                    .to_string(),
        }),
        Some(None) => Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason: "world has no owner_creator_id; cannot authorize World KB edit".to_string(),
        }),
    }
}
