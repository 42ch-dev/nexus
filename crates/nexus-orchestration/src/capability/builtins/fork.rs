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
        // label bounds mirror the HTTP schema (create-fork-request.schema.json,
        // 1–200 chars when present) — orchestration/preset surface parity.
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"parent_branch_id":{"type":"string"},"forked_from_event_id":{"type":"string"},"label":{"type":"string","minLength":1,"maxLength":200}},"required":["world_id","creator_id","parent_branch_id","forked_from_event_id"],"additionalProperties":false}"#
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
        // Carrier B (plan 2026-08-12-v1.162-p1-fork-backend-foundation): the
        // marker is written `status=canon` with structured lineage in
        // `extensions_nexus_json` (`fork_lineage`). Canon status reflects that
        // a fork creation is a committed structural fact and makes the marker
        // findable in the canon-default timeline read; lineage surfaces via
        // `TimelineEventInfo.extensions` on the existing timeline-events route.
        let lineage_json = json!({
            "fork_lineage": {
                "parent_branch_id": &parsed.parent_branch_id,
                "forked_from_event_id": &parsed.forked_from_event_id,
                "label": &label,
            }
        })
        .to_string();
        let marker = nexus_local_db::narrative_write::append_event_canon_with_extensions(
            pool,
            &parsed.world_id,
            &new_branch_id,
            "fork_created",
            Some(&label),
            Some(&marker_summary),
            &lineage_json,
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
            None, // modules_json — test seed writes no modules
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

    #[tokio::test]
    async fn forked_branch_marker_carries_lineage() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let (world_id, parent_branch, fork_point) = seed_world_with_event(&pool, "ctr_a").await;

        let cap = ForkCreate::with_pool(pool.clone());
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
        let new_branch = out["branch_id"].as_str().unwrap();

        // SAFETY: test-only SELECT against known narrative_timeline_events schema.
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT status, extensions_nexus_json, event_type \
             FROM narrative_timeline_events \
             WHERE world_id = ? AND branch_id = ? AND event_type = 'fork_created'",
        )
        .bind(&world_id)
        .bind(new_branch)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 1, "exactly one fork_created marker expected");
        let (status, extensions_json, event_type) = &rows[0];
        assert_eq!(event_type, "fork_created");
        assert_eq!(status, "canon");
        let extensions: Value = serde_json::from_str(extensions_json).unwrap();
        assert_eq!(
            extensions["fork_lineage"]["parent_branch_id"],
            parent_branch
        );
        assert_eq!(
            extensions["fork_lineage"]["forked_from_event_id"],
            fork_point
        );
        assert_eq!(extensions["fork_lineage"]["label"], "alt-ending");
    }

    #[tokio::test]
    async fn root_branch_has_no_fork_marker() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let (world_id, parent_branch, _fork_point) = seed_world_with_event(&pool, "ctr_a").await;

        // SAFETY: test-only SELECT against known narrative_timeline_events schema.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM narrative_timeline_events \
             WHERE world_id = ? AND branch_id = ? AND event_type = 'fork_created'",
        )
        .bind(&world_id)
        .bind(&parent_branch)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 0, "root branch must have no fork_created marker");
    }

    #[tokio::test]
    async fn fork_marker_is_canon() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let (world_id, parent_branch, fork_point) = seed_world_with_event(&pool, "ctr_a").await;

        let cap = ForkCreate::with_pool(pool.clone());
        let out = cap
            .run(json!({
                "world_id": world_id,
                "creator_id": "ctr_a",
                "parent_branch_id": parent_branch,
                "forked_from_event_id": fork_point,
            }))
            .await
            .unwrap();
        let new_branch = out["branch_id"].as_str().unwrap();

        // The marker must be findable in a canon-filtered read — exactly the
        // SQL the timeline-events route runs (`status` defaults to `canon`,
        // `event_type` exact-match; see `list_timeline_events_page`). This
        // verifies the Narrative-layer read does not choke on the
        // `fork_created` event type (plan risk: canon-merge/overview
        // interaction).
        let rows = nexus_local_db::narrative_gateway::list_timeline_events_page(
            &pool,
            &world_id,
            Some(new_branch),
            Some("canon"),
            Some("fork_created"),
            None,
            10,
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "fork marker must be written status=canon");
        assert_eq!(rows[0].status, "canon");
        assert_eq!(rows[0].event_type, "fork_created");
        let extensions: Value =
            serde_json::from_str(rows[0].extensions_nexus_json.as_deref().unwrap()).unwrap();
        assert_eq!(
            extensions["fork_lineage"]["parent_branch_id"],
            parent_branch
        );
        assert_eq!(
            extensions["fork_lineage"]["forked_from_event_id"],
            fork_point
        );
        // Default-label contract: the caller omitted `label`, so the marker
        // must carry the canonical `"fork"` default.
        assert_eq!(extensions["fork_lineage"]["label"], "fork");
    }
}
