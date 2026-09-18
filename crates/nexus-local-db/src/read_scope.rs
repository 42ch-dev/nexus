//! Admitted read-selection resolution (v1.191 P1 R5).
//!
//! Durable §4.1 makes the knowledge read selection **server-chosen**: it is
//! derived from stored ownership rows and the holder registry, never from a
//! request body. This module is the shared home for that derivation, so every
//! layer that reads a World KB on behalf of an admitted Creator resolves the
//! same containers and the same holder — the `nexus-core` ActorView arm and the
//! orchestration capabilities/paths both build their selection from here.
//!
//! The selection resolved here is the **ActorView** one (durable §4.1): the
//! owned World plus its owned Character/binding containers, admitting exactly
//! the Creator's own holder. A `CreatorManagement` review selection is never
//! minted here — management review is its own surface, and a model/consumer
//! read must never inherit it by implication.

use nexus_knowledge::world_kb::knowledge_entry::KnowledgeOwnerRef;
use nexus_knowledge::world_kb::store::KnowledgeReadScope;
use sqlx::{Row, SqlitePool};

use crate::LocalDbError;

/// Owned containers of one World for an admitted read selection, plus the
/// owned Character ids they were derived from.
///
/// The returned container list starts with the World container and then follows
/// the stored `(character_id, binding_id)` order as World → Character → binding,
/// deduplicated per Character, so the same stored ownership always resolves the
/// same sequence. Retained reads (durable §11.2): an archived Character's
/// container stays in the selection — liveness is the admitted-actor checks'
/// job, not this derivation's.
///
/// The Character ids are the same deduplicated set the containers were built
/// from, in that same order: the `CreatorManagement` review selection needs them
/// to resolve its known-governance holder set, so a management review covers
/// exactly the Characters whose containers it authorizes.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn owned_world_containers(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> Result<(Vec<KnowledgeOwnerRef>, Vec<String>), LocalDbError> {
    let rows = sqlx::query(
        "SELECT b.character_id, b.binding_id \
         FROM actor_world_bindings b \
         INNER JOIN characters c ON c.character_id = b.character_id \
         WHERE b.world_id = ? AND c.owner_creator_id = ? \
         ORDER BY b.character_id ASC, b.binding_id ASC",
    )
    .bind(world_id)
    .bind(creator_id)
    .fetch_all(pool)
    .await?;

    let mut containers = vec![KnowledgeOwnerRef::world(world_id)];
    let mut character_ids: Vec<String> = Vec::new();
    for row in rows {
        let character_id: String = row.try_get("character_id")?;
        let binding_id: String = row.try_get("binding_id")?;
        if !character_ids.contains(&character_id) {
            character_ids.push(character_id.clone());
            containers.push(KnowledgeOwnerRef::character(character_id));
        }
        containers.push(KnowledgeOwnerRef::actor_world_binding(binding_id));
    }
    Ok((containers, character_ids))
}

/// The `ActorView` read selection of an admitted Creator over one **owned**
/// World (durable §4.1).
///
/// Fail-closed by construction: a foreign/missing World refuses, and a Creator
/// with no holder registry row refuses, so a caller that cannot resolve a
/// selection has nothing to read with — there is no narrower-or-wider fallback
/// to fall back to. The holder is resolved, never provisioned (§2.2).
///
/// # Errors
///
/// Returns [`LocalDbError::ActorNotFound`] for a foreign/missing World,
/// [`LocalDbError::HolderStateInvalid`] for a missing/corrupt holder registry
/// row, and [`LocalDbError::Sqlx`] on database failure.
pub async fn creator_actor_view_scope(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> Result<KnowledgeReadScope, LocalDbError> {
    // Stored ownership, no status requirement (retained reads, durable §11.2):
    // a World still owned but paused/archived keeps its retained history
    // readable. The World container is authorized only because the stored row
    // says so — naming a foreign World never authorizes it.
    let stored: Option<String> =
        sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await?;
    if stored.as_deref() != Some(creator_id) {
        return Err(LocalDbError::ActorNotFound {
            resource: "world",
            id: world_id.to_string(),
        });
    }

    let (containers, _) = owned_world_containers(pool, creator_id, world_id).await?;
    let holder_entry_id = crate::require_creator_holder(pool, creator_id).await?;
    KnowledgeReadScope::actor_view(holder_entry_id, containers).map_err(|err| {
        // Unreachable in practice: `require_creator_holder` resolved a non-empty
        // holder id. It fails closed rather than forming a holder-less
        // selection that would admit shared rows only.
        LocalDbError::HolderStateInvalid {
            reason: err.to_string(),
        }
    })
}
