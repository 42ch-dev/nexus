//! Thin daemon adapter over the core bounded Character mind projection
//! (v1.190 P2-T2).
//!
//! The SOUL/Memory/`ToM` projection itself lives in [`nexus_core`]
//! (`CoreCharacterMind`); this module keeps only the pool-bound entry the
//! daemon `agent_host` composition (P4-T2's file) still calls, so its import
//! path survives until P4-T2 consumes the core reader directly. Retained
//! semantics are unchanged: only the executing Character's shared scope plus
//! the selected binding-local scope are projected, and any fail-closed read
//! aborts before host launch.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use nexus_moment_context_assembly::CharacterMindInput;
use sqlx::SqlitePool;
use std::path::Path;

/// Load bounded SOUL/Memory plus L1-then-L2 `ToM` for an admitted Character run.
///
/// Caller guarantees admission (owner/active Character + active binding).
pub async fn load_character_mind_projection_with_tom(
    pool: &SqlitePool,
    nexus_home: &Path,
    owner_creator_id: &str,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> Result<CharacterMindInput, NexusApiError> {
    nexus_core::CoreCharacterMind::new(pool.clone(), nexus_home.to_owned())
        .projection_with_tom(owner_creator_id, character_id, world_id, binding_id)
        .await
        .map_err(NexusApiError::from)
}
