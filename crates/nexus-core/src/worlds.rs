//! World lifecycle operations (list/get/create/delete), owned by
//! `CoreService` (v1.190 P0-T1).
//!
//! Extracted from the daemon `narrative` HTTP handlers: those keep only
//! auth/DTO/status translation, while title validation, slug derivation and
//! the hard-delete cascade (V1.129 P2 architect lock) run here. Reads reuse
//! the shared `SqliteNarrativeGateway` repository and writes reuse the shared
//! `narrative_write` repository — no second SQL implementation, the same one
//! the CLI uses.

use nexus_contracts::daemon_api::{CreateWorldRequest, CreateWorldResponse};
use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;
use nexus_local_db::narrative_write;
use nexus_narrative::{NarrativeGateway, WorldState};
use sqlx::SqlitePool;

use crate::error::db_err;
use crate::error::{local_db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use crate::CoreAccess;

/// `CoreError::Forbidden.resource` marker for a World hard-delete blocked by
/// remaining Character bindings. The HTTP adapter translates it back to the
/// retained `409 world_has_actor_bindings` body; the string matches the
/// `as_str()` of `nexus_local_db::ActorContractConflict::WorldHasActorBindings`.
pub const DELETE_WORLD_BLOCKED_BY_BINDINGS: &str = "world_has_actor_bindings";

impl CoreService {
    /// List the workspace's worlds, oldest first (shared gateway read).
    ///
    /// Read-scope invariant (workspace-single-owner): the workspace state DB
    /// holds one owner's workspace, so the lifecycle reads intentionally
    /// return workspace-wide rows with no `owner_creator_id` filter —
    /// parity with the pre-migration daemon read. Owner isolation is a
    /// workspace boundary property, not a per-read filter; mutations stay
    /// ownership-guarded.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, and [`CoreError::Internal`] when the
    /// gateway read fails.
    pub async fn list_worlds(&self, principal: &Principal) -> CoreResult<Vec<WorldState>> {
        self.verify_principal(principal)?;
        let gateway = SqliteNarrativeGateway::new(self.inner.pool.clone());
        gateway
            .list_worlds()
            .await
            .map_err(|e| narrative_internal("worlds.list", &e))
    }

    /// Project one world's state; unknown ids are [`CoreError::NotFound`].
    ///
    /// Like [`Self::list_worlds`], this read is workspace-scoped: it returns
    /// the requested row without an `owner_creator_id` filter (workspace
    /// single owner; parity with the pre-migration daemon). Deletion is the
    /// owner-guarded path.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::NotFound`] for an unknown
    /// world, and [`CoreError::Internal`] on other gateway failures.
    pub async fn get_world(
        &self,
        principal: &Principal,
        world_id: String,
    ) -> CoreResult<WorldState> {
        self.verify_principal(principal)?;
        let gateway = SqliteNarrativeGateway::new(self.inner.pool.clone());
        gateway
            .get_world_state(&world_id)
            .await
            .map_err(|e| match &e {
                nexus_narrative::NarrativeError::ValidationError(msg)
                    if msg.contains("not found") =>
                {
                    CoreError::NotFound {
                        resource: format!("World {world_id} not found"),
                    }
                }
                _ => narrative_internal("worlds.get", &e),
            })
    }

    /// Create a World owned by the principal's creator (V1.130 P2 contract).
    ///
    /// Validates the title (1-200 chars after trim), derives the ASCII
    /// kebab-case slug, and persists through the shared `narrative_write`
    /// repository (`private` visibility, `manual` time policy — unchanged).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::Forbidden`] under read-only
    /// access, [`CoreError::InvalidInput`] when the trimmed title is empty or
    /// over 200 characters, and [`CoreError::Internal`] on storage failure.
    ///
    /// # Panics
    /// Panics if the generated `active` status constant fails to parse — a
    /// wire-shape drift, not a runtime input error.
    pub async fn create_world(
        &self,
        principal: &Principal,
        request: CreateWorldRequest,
    ) -> CoreResult<CreateWorldResponse> {
        self.verify_principal(principal)?;
        require_write_access(self, "world_create")?;

        let title = request.title.trim();
        if title.is_empty() || title.chars().count() > 200 {
            return Err(CoreError::InvalidInput {
                field: "title".to_string(),
                reason: "title must be 1-200 characters after trimming whitespace".to_string(),
            });
        }

        let slug = derive_world_slug(title);
        let result = narrative_write::create_world(
            &self.inner.pool,
            principal.creator_id(),
            title,
            &slug,
            "private",
            "manual",
        )
        .await
        .map_err(|e| CoreError::Internal {
            category: format!("worlds.create: {e}"),
        })?;

        tracing::info!(
            target: "worlds.create",
            world_id = %result.world_id,
            creator_id = %principal.creator_id(),
            "World created"
        );

        Ok(CreateWorldResponse {
            world_id: result.world_id,
            status: "active".parse().expect("valid status constant"),
        })
    }

    /// Hard-delete a World (V1.129 P2 architect lock: hard, not soft).
    ///
    /// Ownership is the only precondition; a remaining Character binding
    /// blocks deletion with the [`DELETE_WORLD_BLOCKED_BY_BINDINGS`] marker.
    /// The manual cascade (`kb_extract_jobs` cleanup, `works.world_id = NULL`
    /// for the owner's Works) and the World `DELETE` (FK cascades handle
    /// timelines, KB blocks + anchors, relationships) run inside one private
    /// transaction, so a failure rolls the whole cleanup back.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::Forbidden`] under read-only
    /// access or when bindings remain (marker resource),
    /// [`CoreError::NotFound`] when the world is unknown or lost to a
    /// concurrent delete, and the mapped storage error otherwise.
    pub async fn delete_world(&self, principal: &Principal, world_id: String) -> CoreResult<()> {
        self.verify_principal(principal)?;
        require_write_access(self, "world_delete")?;
        delete_world_hard(&self.inner.pool, principal.creator_id(), &world_id).await
    }
}

fn require_write_access(service: &CoreService, what: &str) -> CoreResult<()> {
    if service.inner.access == CoreAccess::ReadOnly {
        return Err(CoreError::Forbidden {
            resource: format!("{what}: read-only core access"),
        });
    }
    Ok(())
}

/// Derive an ASCII kebab-case slug from a title (shared `title_to_slug`
/// normalization; `"world"` when it yields empty, e.g. CJK-only titles).
fn derive_world_slug(title: &str) -> String {
    let slug = nexus_local_db::inspiration_items::title_to_slug(title);
    if slug.is_empty() {
        "world".to_string()
    } else {
        slug
    }
}

pub(crate) fn narrative_internal(what: &str, e: &nexus_narrative::NarrativeError) -> CoreError {
    CoreError::Internal {
        category: format!("{what}: {e}"),
    }
}

/// Hard-delete transaction (ported verbatim from the daemon handler).
async fn delete_world_hard(pool: &SqlitePool, creator_id: &str, world_id: &str) -> CoreResult<()> {
    // Existence + ownership check using the shared narrative_write admission
    // gate. This is the only precondition before mutation; worlds have no
    // runtime lock analogue.
    let owned = narrative_write::is_world_owned(pool, creator_id, world_id)
        .await
        .map_err(|e| CoreError::Internal {
            category: format!("worlds.delete: {e}"),
        })?;
    if !owned {
        return Err(CoreError::NotFound {
            resource: format!("World {world_id} not found"),
        });
    }

    let mut tx = nexus_local_db::begin_immediate(pool)
        .await
        .map_err(local_db_err)?;

    let binding_count = nexus_local_db::count_bindings_for_world_tx(&mut tx, world_id)
        .await
        .map_err(local_db_err)?;
    if binding_count > 0 {
        return Err(CoreError::Forbidden {
            resource: DELETE_WORLD_BLOCKED_BY_BINDINGS.to_string(),
        });
    }

    // Manual cascade for tables without an FK on narrative_worlds.world_id:
    // kb_extract_jobs rows, then works.world_id = NULL for the owner's Works
    // (architect lock: preserve Works). FK cascades on the World DELETE handle
    // narrative_timeline_events, kb_key_blocks (+ kb_source_anchors) and
    // kb_relationships. Dropping `tx` on any early error rolls everything back.
    // SAFETY: DELETE / UPDATE match kb_extract_jobs DDL in 20260527 and works
    // DDL in 20260604 (kept verbatim from the migrated daemon handler).
    if let Err(e) = sqlx::query("DELETE FROM kb_extract_jobs WHERE world_id = ?")
        .bind(world_id)
        .execute(&mut *tx)
        .await
    {
        return Err(CoreError::Internal {
            category: format!("worlds.delete: kb_extract_jobs cleanup failed: {e}"),
        });
    }

    if let Err(e) = sqlx::query(
        "UPDATE works SET world_id = NULL, updated_at = ? \
         WHERE world_id = ? AND creator_id = ?",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(world_id)
    .bind(creator_id)
    .execute(&mut *tx)
    .await
    {
        return Err(CoreError::Internal {
            category: format!("worlds.delete: works.world_id clear failed: {e}"),
        });
    }

    let deleted = match sqlx::query(
        "DELETE FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?",
    )
    .bind(world_id)
    .bind(creator_id)
    .execute(&mut *tx)
    .await
    {
        Ok(res) => res.rows_affected(),
        Err(e) => {
            // Last resort after the declarative binding pre-count.
            if let sqlx::Error::Database(db) = &e {
                if db.is_foreign_key_violation() {
                    return Err(CoreError::Forbidden {
                        resource: DELETE_WORLD_BLOCKED_BY_BINDINGS.to_string(),
                    });
                }
            }
            return Err(db_err(&e));
        }
    };

    if deleted == 0 {
        // Concurrent delete raced; treat as not found — row is gone. The tx
        // rolls back on Drop, discarding the no-op manual cleanup above.
        return Err(CoreError::NotFound {
            resource: format!("World {world_id} not found"),
        });
    }

    tx.commit().await.map_err(|e| db_err(&e))?;

    tracing::info!(
        target: "worlds.delete",
        world_id = %world_id,
        creator_id = %creator_id,
        "World hard-deleted (KB + timelines cascaded; Works preserved with world_id=NULL)"
    );

    Ok(())
}
