//! `nexus.fork.create` capability (V1.60 P0 — DF-46).
//!
//! Creates an explicit local timeline fork — a new `branch_id` within an
//! existing world owned by the caller. This is **local timeline branching**
//! ("explicit branch creation when rewrite-past is intended"), distinct from
//! the PD-01 **platform community/social fork** which remains platform-only.
//!
//! # PD-01 boundary
//!
//! PD-01 rules that "World fork is platform-only" refers to community/social
//! forking (sharing a world across creators / publishing a fork to a
//! community). `nexus.fork.create` is the **local** operation: a single
//! creator branches their own world's timeline so a divergent rewrite can be
//! explored without disturbing the parent branch. It performs no sync, no
//! cross-creator sharing, and no platform publish.
//!
//! # Design
//!
//! Forks are lazy in V1.26+ storage: a fork is a new `branch_id` carried by
//! timeline events (there is no dedicated `fork_branches` table — see
//! `narrative_gateway.rs` doc comment). `fork.create` allocates the new
//! branch id and materializes it by appending a `fork_created` marker event at
//! `sequence_no` 0 on the new branch, recording the parent branch + fork point.
//! Mirrors the `nexus.reference.refresh` (V1.58 P1) orchestration handler
//! pattern.

use crate::capability::builtins::world::ensure_world_owned;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Input for `nexus.fork.create`.
#[derive(Debug, Deserialize)]
struct ForkCreateInput {
    world_id: String,
    /// Caller creator id (admission gate).
    creator_id: String,
    /// The branch the new fork diverges from.
    parent_branch_id: String,
    /// The event on the parent branch that is the fork point (branch head
    /// after which the new branch diverges).
    forked_from_event_id: String,
    /// Optional human-readable label for the new branch.
    #[serde(default)]
    label: Option<String>,
}

/// Create a local timeline fork (new branch within an owned world).
#[derive(Debug, Clone)]
pub struct ForkCreate {
    pool: Option<Arc<sqlx::SqlitePool>>,
}

impl ForkCreate {
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(Arc::new(pool)),
        }
    }
}

impl Default for ForkCreate {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a new fork branch id.
fn generate_fork_branch_id() -> String {
    format!("fbk_{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

#[async_trait]
impl Capability for ForkCreate {
    fn name(&self) -> &'static str {
        "nexus.fork.create"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"parent_branch_id":{"type":"string"},"forked_from_event_id":{"type":"string"},"label":{"type":"string"}},"required":["world_id","creator_id","parent_branch_id","forked_from_event_id"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"branch_id":{"type":"string"},"parent_branch_id":{"type":"string"},"forked_from_event_id":{"type":"string"},"created_at":{"type":"string","format":"date-time"}},"required":["branch_id","parent_branch_id","forked_from_event_id","created_at"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: ForkCreateInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("fork.create input: {e}")))?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        tracing::info!(
            world_id = %parsed.world_id,
            parent_branch = %parsed.parent_branch_id,
            "fork.create admitted"
        );

        // Admission gate: creator must own the world.
        ensure_world_owned(pool, &parsed.creator_id, &parsed.world_id).await?;

        // Validate the fork point event exists and belongs to the parent branch.
        // SAFETY: SELECT against known narrative_timeline_events schema.
        let event_ok: Option<String> = sqlx::query_scalar(
            "SELECT timeline_event_id FROM narrative_timeline_events \
             WHERE timeline_event_id = ? AND world_id = ? AND branch_id = ?",
        )
        .bind(&parsed.forked_from_event_id)
        .bind(&parsed.world_id)
        .bind(&parsed.parent_branch_id)
        .fetch_optional(&**pool)
        .await
        .map_err(|e| CapabilityError::Internal(format!("fork point check: {e}")))?;
        if event_ok.is_none() {
            return Err(CapabilityError::InputInvalid(format!(
                "fork point event '{}' not found on branch '{}' in world '{}'",
                parsed.forked_from_event_id, parsed.parent_branch_id, parsed.world_id
            )));
        }

        // Allocate the new branch id.
        let new_branch_id = generate_fork_branch_id();

        // Materialize the fork by appending a `fork_created` marker event on the
        // new branch at sequence_no 0. This establishes the branch in storage
        // (lazy forks are otherwise invisible until the first real event).
        let label = parsed.label.clone().unwrap_or_else(|| "fork".to_string());
        let marker_summary = format!(
            "forked from {}/{} ({label})",
            parsed.parent_branch_id, parsed.forked_from_event_id
        );
        let marker = nexus_local_db::narrative_write::append_event(
            pool,
            &parsed.world_id,
            &new_branch_id,
            "fork_created",
            Some(&label),
            Some(&marker_summary),
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("fork marker append: {e}")))?;

        tracing::info!(
            world_id = %parsed.world_id,
            new_branch = %new_branch_id,
            parent_branch = %parsed.parent_branch_id,
            marker_event = %marker.event_id,
            "fork.create: local timeline fork established"
        );

        Ok(json!({
            "branch_id": new_branch_id,
            "parent_branch_id": parsed.parent_branch_id,
            "forked_from_event_id": parsed.forked_from_event_id,
            "created_at": marker.created_at,
        }))
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_creator(pool: &sqlx::SqlitePool, creator_id: &str) {
        // SAFETY: test-only seed.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(creator_id)
        .bind("Test Creator")
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_world_with_event(
        pool: &sqlx::SqlitePool,
        owner: &str,
    ) -> (String, String, String) {
        let w = nexus_local_db::narrative_write::create_world(
            pool, owner, "Test", "test", "private", "manual",
        )
        .await
        .unwrap();
        let evt = nexus_local_db::narrative_write::append_event(
            pool,
            &w.world_id,
            &w.root_fork_branch_id,
            "story_advance",
            Some("Parent event"),
            None,
        )
        .await
        .unwrap();
        (w.world_id, w.root_fork_branch_id, evt.event_id)
    }

    #[tokio::test]
    async fn fork_create_success() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let (world_id, parent_branch, fork_point) = seed_world_with_event(&pool, "ctr_a").await;

        let cap = ForkCreate::with_pool(pool);
        let out = cap
            .run(json!({
                "world_id": world_id,
                "creator_id": "ctr_a",
                "parent_branch_id": parent_branch,
                "forked_from_event_id": fork_point,
                "label": "alt-ending",
            }))
            .await
            .unwrap();
        assert!(out["branch_id"].as_str().unwrap().starts_with("fbk_"));
        assert_eq!(out["parent_branch_id"], parent_branch);
        assert_eq!(out["forked_from_event_id"], fork_point);
    }

    #[tokio::test]
    async fn fork_create_rejects_cross_creator() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_creator(&pool, "ctr_b").await;
        let (world_id, parent_branch, fork_point) = seed_world_with_event(&pool, "ctr_a").await;

        let cap = ForkCreate::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": world_id,
                "creator_id": "ctr_b",
                "parent_branch_id": parent_branch,
                "forked_from_event_id": fork_point,
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn fork_create_rejects_bad_fork_point() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let (world_id, parent_branch, _fork_point) = seed_world_with_event(&pool, "ctr_a").await;

        let cap = ForkCreate::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": world_id,
                "creator_id": "ctr_a",
                "parent_branch_id": parent_branch,
                "forked_from_event_id": "evt_does_not_exist",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }
}
