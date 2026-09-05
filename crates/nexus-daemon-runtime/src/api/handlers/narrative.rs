//! Narrative read surface handlers (V1.25 Theme C, C1.1 → V1.26 persistence).
//!
//! Read-only daemon routes backed by `NarrativeGateway` with
//! `SQLite` persistence via `SqliteNarrativeGateway`.
//!
//! # Endpoints
//!
//! - `GET /v1/daemon/narrative/worlds` — list all worlds
//! - `GET /v1/daemon/narrative/worlds/{world_id}` — get a single world state
//!
//! These are **narrative state** routes, distinct from the work-scope
//! `/v1/daemon/kb/*` file-index routes.

#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::read_active_creator_id;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::daemon_api::{CreateWorldRequest, CreateWorldResponse};
use nexus_narrative::{NarrativeGateway, WorldState};
use serde::Serialize;

// ─── Response types ────────────────────────────────────────────────────────

/// `GET /v1/daemon/narrative/worlds` response.
#[derive(Debug, Serialize)]
pub struct ListWorldsResponse {
    pub worlds: Vec<WorldState>,
}

/// `GET /v1/daemon/narrative/worlds/{world_id}` response.
#[derive(Debug, Serialize)]
pub struct GetWorldResponse {
    pub world: WorldState,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /v1/daemon/narrative/worlds` — list all worlds.
///
/// Returns worlds from the persistent `SQLite` gateway. Empty list when
/// no worlds have been seeded into the database.
pub async fn list_worlds(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListWorldsResponse>, NexusApiError> {
    let gateway = state
        .narrative_gateway()
        .ok_or(NexusApiError::Uninitialized)?;
    let worlds = gateway
        .list_worlds()
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "NARRATIVE_ERROR".into(),
            message: e.to_string(),
        })?;
    Ok(Json(ListWorldsResponse { worlds }))
}

/// `GET /v1/daemon/narrative/worlds/{world_id}` — get a single world state.
///
/// Returns 404 for an unknown world ID. Returns the projected world
/// state for a known world from the persistent gateway.
pub async fn get_world(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<GetWorldResponse>, NexusApiError> {
    let gateway = state
        .narrative_gateway()
        .ok_or(NexusApiError::Uninitialized)?;
    let world = gateway
        .get_world_state(&world_id)
        .await
        .map_err(|e| match e {
            nexus_narrative::NarrativeError::ValidationError(msg) if msg.contains("not found") => {
                NexusApiError::NotFound(format!("World {world_id} not found"))
            }
            _ => NexusApiError::Internal {
                code: "NARRATIVE_ERROR".into(),
                message: e.to_string(),
            },
        })?;
    Ok(Json(GetWorldResponse { world }))
}

/// `DELETE /v1/daemon/worlds/{world_id}` — hard-delete a World (V1.129 P2).
///
/// Per architect lock (Seat 2, 2026-07-21 — R-V1126P0-T2-001): **hard delete**,
/// not soft. Confirm dialog in the web UI is the safety net.
///
/// # Cascade (per lock)
///
/// - `narrative_worlds` row is deleted; `SQLite` FKs cascade:
///   - `narrative_timeline_events` → `ON DELETE CASCADE`
///   - `kb_key_blocks` → `ON DELETE CASCADE` (and `kb_source_anchors` via
///     its FK on `kb_key_blocks`)
///   - `kb_relationships` → `ON DELETE CASCADE`
/// - `kb_extract_jobs.world_id` has no FK; manual cascade runs first.
/// - `works.world_id` has no FK; the handler sets it to NULL **on Works owned
///   by the active creator** so those Works survive the World's removal
///   (architect lock: preserve Works).
///
/// # Errors
///
/// - `401 AuthRequired` if no active creator is configured.
/// - `404 NotFound` if the world id is unknown or not owned by the caller.
/// - `409 Conflict` (`world_has_actor_bindings`) if any Character binding remains.
/// - `500 Internal` on database error.
///
/// `POST /v1/daemon/worlds` - create a new World (V1.130 P2).
///
/// The daemon resolves the active creator from `nexus_home/config.toml`;
/// clients never send ownership. The title is validated (1-200 chars after
/// trim), an ASCII kebab slug is derived, and the world is persisted via
/// `nexus_local_db::narrative_write::create_world`.
///
/// # Errors
///
/// - `400 BadRequest` if title is empty or exceeds 200 chars after trim.
/// - `401 AuthRequired` if no active creator is configured.
/// - `500 Internal` on database error.
pub async fn create_world(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateWorldRequest>,
) -> Result<(StatusCode, Json<CreateWorldResponse>), NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Validate title: 1-200 chars after trim.
    let title = req.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(NexusApiError::BadRequest {
            code: "invalid_title".to_string(),
            message: "title must be 1-200 characters after trimming whitespace".to_string(),
        });
    }

    // Derive ASCII kebab slug from title; fall back to "world" when
    // normalization yields empty (e.g. CJK-only titles).
    let slug = derive_world_slug(title);

    let result = nexus_local_db::narrative_write::create_world(
        pool,
        &creator_id,
        title,
        &slug,
        "private",
        "manual",
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    tracing::info!(
        target: "worlds.create",
        world_id = %result.world_id,
        creator_id = %creator_id,
        "World created"
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateWorldResponse {
            world_id: result.world_id,
            status: "active".parse().expect("valid status constant"),
        }),
    ))
}

/// Derive an ASCII kebab-case slug from a title.
///
/// Reuses `nexus_local_db::inspiration_items::title_to_slug` which handles
/// lowercasing, hyphen collapsing, truncation, and CJK fallback. When the
/// result is empty, returns `"world"` as the default slug.
fn derive_world_slug(title: &str) -> String {
    let slug = nexus_local_db::inspiration_items::title_to_slug(title);
    if slug.is_empty() {
        "world".to_string()
    } else {
        slug
    }
}

#[allow(clippy::too_many_lines)] // route handler
pub async fn delete_world(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<StatusCode, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    // Existence + ownership check using the shared narrative_write admission
    // gate. This is the only precondition before mutation; worlds have no
    // runtime lock analogue.
    let owned = nexus_local_db::narrative_write::is_world_owned(pool, &creator_id, &world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::NotFound(format!(
            "World {world_id} not found"
        )));
    }

    // Manual cascade for tables without an FK on narrative_worlds.world_id:
    //
    // 1. kb_extract_jobs — job queue rows that reference this World. Deleting
    //    the World would otherwise orphan them in `queued`/`running` state.
    // 2. works.world_id — set to NULL so Works survive the World's removal
    //    (architect lock). The creator_id scope is defensive; in practice the
    //    local-first model guarantees single-creator state.
    //
    // All three steps run inside ONE transaction so a later failure (e.g. the
    // parent World DELETE) rolls back the manual cleanup. Without this, a
    // successful kb_extract_jobs DELETE or works UPDATE followed by a failed
    // World DELETE would leave orphaned state committed — a caller retry would
    // then find the World still present but with its job queue already drained
    // and Works already detached. (V1.129 P5 — Greptile P1; also closes the
    // deferred QC3-W-001 / QC2-M-001 from P2's QC tri.)
    //
    // SAFETY: DELETE / UPDATE match kb_extract_jobs DDL in 20260527 and works
    // DDL in 20260604. Runtime sqlx::query keeps the world-removal step
    // cohesive in one handler rather than scattering fragments across
    // .sqlx/ entries. On any early `return Err(...)` below the `tx` is
    // dropped, which sqlx turns into an automatic ROLLBACK.
    let mut tx =
        nexus_local_db::begin_immediate(pool)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("delete_world: begin tx failed: {e}"),
            })?;

    let binding_count = nexus_local_db::count_bindings_for_world_tx(&mut tx, &world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("delete_world: actor binding check failed: {e}"),
        })?;
    if binding_count > 0 {
        return Err(NexusApiError::from(
            nexus_local_db::LocalDbError::ActorContractConflict {
                code: nexus_local_db::ActorContractConflict::WorldHasActorBindings,
            },
        ));
    }

    if let Err(e) = sqlx::query("DELETE FROM kb_extract_jobs WHERE world_id = ?")
        .bind(&world_id)
        .execute(&mut *tx)
        .await
    {
        return Err(NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("delete_world: kb_extract_jobs cleanup failed: {e}"),
        });
    }

    if let Err(e) = sqlx::query(
        "UPDATE works SET world_id = NULL, updated_at = ? \
         WHERE world_id = ? AND creator_id = ?",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&world_id)
    .bind(&creator_id)
    .execute(&mut *tx)
    .await
    {
        return Err(NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("delete_world: works.world_id clear failed: {e}"),
        });
    }

    // Delete the World row. FK cascades handle:
    //   narrative_timeline_events, kb_key_blocks (+ kb_source_anchors),
    //   kb_relationships.
    let deleted = match sqlx::query(
        "DELETE FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?",
    )
    .bind(&world_id)
    .bind(&creator_id)
    .execute(&mut *tx)
    .await
    {
        Ok(res) => res.rows_affected(),
        Err(e) => {
            // Last resort after the declarative binding pre-count.
            if let sqlx::Error::Database(db) = &e {
                if db.is_foreign_key_violation() {
                    return Err(NexusApiError::from(
                        nexus_local_db::LocalDbError::ActorContractConflict {
                            code: nexus_local_db::ActorContractConflict::WorldHasActorBindings,
                        },
                    ));
                }
            }
            return Err(NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("delete_world failed: {e}"),
            });
        }
    };

    if deleted == 0 {
        // Concurrent delete raced; treat as 404 — row is gone. The tx rolls
        // back on Drop, discarding the no-op manual cleanup above (each step
        // matched zero rows since the other caller already cascaded them).
        return Err(NexusApiError::NotFound(format!(
            "World {world_id} not found"
        )));
    }

    tx.commit().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: format!("delete_world: commit failed: {e}"),
    })?;

    tracing::info!(
        target: "worlds.delete",
        world_id = %world_id,
        creator_id = %creator_id,
        "World hard-deleted (KB + timelines cascaded; Works preserved with world_id=NULL)"
    );

    Ok(StatusCode::NO_CONTENT)
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_worlds_returns_empty_for_fresh_gateway() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let gateway = state
            .narrative_gateway()
            .expect("test fixture creates creator DB");
        let worlds = gateway.list_worlds().await.unwrap();
        assert!(worlds.is_empty());
        drop(state);
        drop(tmp);
    }

    #[tokio::test]
    async fn get_world_state_returns_error_for_missing() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let gateway = state
            .narrative_gateway()
            .expect("test fixture creates creator DB");
        let result = gateway.get_world_state("nonexistent").await;
        assert!(result.is_err());
        drop(state);
        drop(tmp);
    }

    #[tokio::test]
    async fn get_world_state_returns_world_when_seeded() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Seed a world directly into the DB
        nexus_local_db::narrative_gateway::seed::world(
            state.pool().expect("test fixture creates creator DB"),
            "wld_test",
            "ctr_test",
            "Test",
            "test",
            "private",
            "manual",
        )
        .await;

        let gateway = state
            .narrative_gateway()
            .expect("test fixture creates creator DB");
        let s = gateway.get_world_state("wld_test").await.unwrap();
        assert_eq!(s.world_id, "wld_test");
        assert_eq!(s.title, "Test");
        drop(state);
        drop(tmp);
    }
}
