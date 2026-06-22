//! `nexus.timeline.event.append` capability (V1.60 P0 — DF-46).
//!
//! Appends a new event to a world's narrative timeline. The append is
//! **immutable**: it never rewrites an existing `event_id` and never mutates a
//! `canon`-status row. New events are always inserted with
//! `status = 'provisional'`; canonization is a separate, later flow (not in
//! V1.60 scope), honoring `acp-capability-set.md` §6 ("No canon history silent
//! rewrite").
//!
//! # Design
//!
//! Thin admission-gated wrapper over
//! [`nexus_local_db::narrative_write::append_event`], which already allocates
//! `sequence_no`, detects UNIQUE conflicts, and validates the world FK. This
//! handler adds the creator-ownership admission gate. Mirrors the
//! `nexus.reference.refresh` (V1.58 P1) orchestration handler pattern.

use crate::capability::builtins::world::ensure_world_owned;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Input for `nexus.timeline.event.append`.
#[derive(Debug, Deserialize)]
struct TimelineEventAppendInput {
    world_id: String,
    /// Caller creator id (admission gate).
    creator_id: String,
    branch_id: String,
    event_type: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    /// Optional explicit event id. When provided, must not collide with an
    /// existing event (handler rejects collisions). When omitted, the runtime
    /// allocates a fresh `evt_*` id.
    #[serde(default)]
    event_id: Option<String>,
}

/// Append a new provisional event to a world's timeline.
#[derive(Debug, Clone)]
pub struct TimelineEventAppend {
    pool: Option<Arc<sqlx::SqlitePool>>,
}

impl TimelineEventAppend {
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

impl Default for TimelineEventAppend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for TimelineEventAppend {
    fn name(&self) -> &'static str {
        "nexus.timeline.event.append"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"branch_id":{"type":"string"},"event_type":{"type":"string"},"title":{"type":"string"},"summary":{"type":"string"},"event_id":{"type":"string"}},"required":["world_id","creator_id","branch_id","event_type"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"event_id":{"type":"string"},"sequence_no":{"type":"integer","minimum":0},"status":{"type":"string","enum":["provisional"]},"created_at":{"type":"string","format":"date-time"}},"required":["event_id","sequence_no","status","created_at"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: TimelineEventAppendInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("timeline.event.append input: {e}"))
        })?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        tracing::info!(
            world_id = %parsed.world_id,
            branch_id = %parsed.branch_id,
            event_type = %parsed.event_type,
            "timeline.event.append admitted"
        );

        // Admission gate: creator must own the world.
        ensure_world_owned(pool, &parsed.creator_id, &parsed.world_id).await?;

        // Canon immutability: if the caller supplied an explicit event_id, reject
        // if it already exists (no silent rewrite). The append DAO itself
        // allocates a fresh id when none is supplied.
        if let Some(ref explicit_id) = parsed.event_id {
            // SAFETY: EXISTS check against known narrative_timeline_events schema.
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM narrative_timeline_events WHERE timeline_event_id = ?)",
            )
            .bind(explicit_id)
            .fetch_one(&**pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("event_id collision check: {e}")))?;
            if exists != 0 {
                return Err(CapabilityError::InputInvalid(format!(
                    "event_id collision: '{explicit_id}' already exists; canon history is immutable"
                )));
            }
        }

        let result = nexus_local_db::narrative_write::append_event(
            pool,
            &parsed.world_id,
            &parsed.branch_id,
            &parsed.event_type,
            parsed.title.as_deref(),
            parsed.summary.as_deref(),
        )
        .await
        .map_err(|e| match e {
            nexus_local_db::narrative_write::NarrativeWriteError::SequenceConflict { .. } => {
                CapabilityError::InputInvalid(format!("sequence conflict: {e}"))
            }
            other => CapabilityError::Internal(format!("append_event: {other}")),
        })?;

        // If the caller supplied an explicit id, rename the allocated event to it
        // (the collision was already rejected above, so this is safe).
        let final_id = if let Some(ref explicit_id) = parsed.event_id {
            if explicit_id != &result.event_id {
                // SAFETY: id rename against known narrative_timeline_events schema.
                sqlx::query(
                    "UPDATE narrative_timeline_events SET timeline_event_id = ? \
                     WHERE timeline_event_id = ?",
                )
                .bind(explicit_id)
                .bind(&result.event_id)
                .execute(&**pool)
                .await
                .map_err(|e| CapabilityError::Internal(format!("event_id rename: {e}")))?;
            }
            explicit_id.clone()
        } else {
            result.event_id
        };

        Ok(json!({
            "event_id": final_id,
            "sequence_no": result.sequence_no,
            "status": "provisional",
            "created_at": result.created_at,
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

    async fn seed_world(pool: &sqlx::SqlitePool, owner: &str) -> String {
        let w = nexus_local_db::narrative_write::create_world(
            pool, owner, "Test", "test", "private", "manual",
        )
        .await
        .unwrap();
        w.world_id
    }

    #[tokio::test]
    async fn timeline_event_append_success() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let world_id = seed_world(&pool, "ctr_a").await;
        let branch = sqlx::query_scalar::<_, String>(
            "SELECT root_fork_branch_id FROM narrative_worlds WHERE world_id = ?",
        )
        .bind(&world_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let cap = TimelineEventAppend::with_pool(pool);
        let out = cap
            .run(json!({
                "world_id": world_id,
                "creator_id": "ctr_a",
                "branch_id": branch,
                "event_type": "story_advance",
                "title": "Chapter 1",
            }))
            .await
            .unwrap();
        assert_eq!(out["status"], "provisional");
        assert_eq!(out["sequence_no"], 0);
    }

    #[tokio::test]
    async fn timeline_event_append_rejects_cross_creator() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_creator(&pool, "ctr_b").await;
        let world_id = seed_world(&pool, "ctr_a").await;

        let cap = TimelineEventAppend::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": world_id,
                "creator_id": "ctr_b",
                "branch_id": "fbk_root",
                "event_type": "story_advance",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn timeline_event_append_rejects_collision() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let world_id = seed_world(&pool, "ctr_a").await;
        let branch = sqlx::query_scalar::<_, String>(
            "SELECT root_fork_branch_id FROM narrative_worlds WHERE world_id = ?",
        )
        .bind(&world_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let cap = TimelineEventAppend::with_pool(pool.clone());
        // First append with an explicit id succeeds.
        cap.run(json!({
            "world_id": world_id,
            "creator_id": "ctr_a",
            "branch_id": branch,
            "event_type": "story_advance",
            "event_id": "evt_fixed_1",
        }))
        .await
        .unwrap();

        // Second append reusing the same explicit id must be rejected.
        let err = cap
            .run(json!({
                "world_id": world_id,
                "creator_id": "ctr_a",
                "branch_id": branch,
                "event_type": "story_advance",
                "event_id": "evt_fixed_1",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }
}
