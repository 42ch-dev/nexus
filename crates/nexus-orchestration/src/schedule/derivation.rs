//! `CoreContextManager` — immutable versioned `core_context` derivation (spec §6).
//!
//! Manages the monotonic version sequence per Schedule, applying [`DerivationStep`]
//! operations and persisting each version as an immutable row in `core_context_versions`.
//!
//! Design: `.agents/plans/knowledge/creator-schedule-and-core-context-v1.md` §6.

use std::sync::Arc;

use nexus_contracts::local::schedule::{
    CoreContextAuthor, CoreContextPayload, CoreContextRecord, CoreContextVersion, DerivationStep,
    EditOp, ScheduleId,
};
use nexus_local_db::SqlitePool;
use serde::de::Error as _;
use tokio::sync::Mutex;

/// Error type for core context operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreContextError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("schedule {0} not found")]
    NotFound(String),
    #[error("preset hook: {0}")]
    PresetHookValidation(String),
    #[error("user edit validation: {0}")]
    UserEditValidation(String),
    #[error("version {1} not found for schedule {0}")]
    VersionNotFound(String, u32),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Manages immutable, versioned `core_context` for a schedule.
///
/// Each `apply()` call appends a new row to `core_context_versions` and
/// bumps `creator_schedules.current_core_context_version`.
///
/// **Write guard (H2)**: The `apply()` and `apply_seed()` methods hold a
/// `tokio::sync::Mutex` to prevent concurrent writes from corrupting the
/// version chain (two concurrent calls could both read version N and both
/// write version N+1, overwriting one another).
pub struct CoreContextManager {
    pool: Arc<SqlitePool>,
    /// Serializes the version check + insert in `apply()` and `apply_seed()`.
    write_guard: Mutex<()>,
}

impl CoreContextManager {
    /// Create a new manager backed by the given shared SQLite pool.
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            write_guard: Mutex::new(()),
        }
    }

    /// Apply the seed step to create version 0 of `core_context`.
    ///
    /// Per spec §6.2, the seed is version 0 (not 1).
    /// This must be called before any [`apply`] call for the schedule.
    ///
    /// Returns the new [`CoreContextRecord`] with version 0.
    pub async fn apply_seed(
        &self,
        schedule_id: &ScheduleId,
        raw: &str,
        author: CoreContextAuthor,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // H2: Serialize writes to prevent version chain corruption.
        let _guard = self.write_guard.lock().await;

        let now = chrono::Utc::now().timestamp();
        let schedule_id_str = &schedule_id.0;
        let new_version = CoreContextVersion(0);

        let new_payload = CoreContextPayload::Text {
            body: raw.to_string(),
        };

        // Serialize payload and derivation for storage
        let payload_kind = "text";
        let content_bytes = serde_json::to_vec(&new_payload)?;
        let step = DerivationStep::Seed {
            raw: raw.to_string(),
        };
        let derivation_json = serde_json::to_string(&step)?;

        let (created_by_kind, created_by_user_id) = match &author {
            CoreContextAuthor::User { id } => ("user", Some(id.clone())),
            CoreContextAuthor::System => ("system", None),
        };

        // Insert version 0 row
        sqlx::query(
            r#"INSERT INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
        )
        .bind(schedule_id_str)
        .bind(new_version.0 as i64)
        .bind(payload_kind)
        .bind(&content_bytes)
        .bind("seed")
        .bind(&derivation_json)
        .bind(now)
        .bind(created_by_kind)
        .bind(&created_by_user_id)
        .execute(&*self.pool)
        .await?;

        // Set the schedule's current_core_context_version to 0
        sqlx::query(
            "UPDATE creator_schedules
             SET current_core_context_version = ?1, updated_at = ?2
             WHERE schedule_id = ?3",
        )
        .bind(new_version.0 as i64)
        .bind(now)
        .bind(schedule_id_str)
        .execute(&*self.pool)
        .await?;

        Ok(CoreContextRecord {
            schedule_id: schedule_id.0.clone(),
            version: new_version,
            content: new_payload,
            derivation: step,
            created_at: now.to_string(),
            created_by: author,
        })
    }

    /// Apply a derivation step to produce the next version of `core_context`.
    ///
    /// - Reads the current version.
    /// - Applies the `DerivationStep` to compute the new payload.
    /// - Inserts a new `core_context_versions` row.
    /// - Bumps `creator_schedules.current_core_context_version`.
    ///
    /// Returns the new [`CoreContextRecord`].
    pub async fn apply(
        &self,
        schedule_id: &ScheduleId,
        step: DerivationStep,
        author: CoreContextAuthor,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // H2: Serialize writes to prevent version chain corruption.
        let _guard = self.write_guard.lock().await;

        let now = chrono::Utc::now().timestamp();
        let schedule_id_str = &schedule_id.0;

        // Read current version and payload
        let current_version = self.current_version(schedule_id).await?;
        let new_version = CoreContextVersion(current_version.0 + 1);

        // Compute new payload from previous content
        let previous_payload = {
            let record = self.read(schedule_id, current_version).await?;
            Some(record.content)
        };

        let new_payload = apply_step(&previous_payload, &step, &author)?;

        // Serialize payload and derivation for storage
        let payload_kind = match &new_payload {
            CoreContextPayload::Text { .. } => "text",
            CoreContextPayload::Struct { .. } => "struct",
        };
        let content_bytes = serde_json::to_vec(&new_payload)?;
        let derivation_json = serde_json::to_string(&step)?;

        let (created_by_kind, created_by_user_id) = match &author {
            CoreContextAuthor::User { id } => ("user", Some(id.clone())),
            CoreContextAuthor::System => ("system", None),
        };

        let derivation_kind = derivation_kind_str(&step);

        // Insert the new version row
        sqlx::query(
            r#"INSERT INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
        )
        .bind(schedule_id_str)
        .bind(new_version.0 as i64)
        .bind(payload_kind)
        .bind(&content_bytes)
        .bind(derivation_kind)
        .bind(&derivation_json)
        .bind(now)
        .bind(created_by_kind)
        .bind(&created_by_user_id)
        .execute(&*self.pool)
        .await?;

        // Bump the schedule's current_core_context_version
        sqlx::query(
            "UPDATE creator_schedules
             SET current_core_context_version = ?1, updated_at = ?2
             WHERE schedule_id = ?3",
        )
        .bind(new_version.0 as i64)
        .bind(now)
        .bind(schedule_id_str)
        .execute(&*self.pool)
        .await?;

        Ok(CoreContextRecord {
            schedule_id: schedule_id.0.clone(),
            version: new_version,
            content: new_payload,
            derivation: step,
            created_at: now.to_string(),
            created_by: author,
        })
    }

    /// Apply a preset hook derivation step.
    ///
    /// **Strict subset**: only `EditOp::Append` and `EditOp::StructMerge` are
    /// allowed. `EditOp::Replace` and `EditOp::StructRemove` are rejected.
    ///
    /// This enforces the spec §6.2 rule: preset hooks are "strictly additive"
    /// in V1.4.
    pub async fn apply_preset_hook(
        &self,
        schedule_id: &ScheduleId,
        state_id: &str,
        hook_name: &str,
        op: EditOp,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // Validate: preset hooks only allow Append or StructMerge
        match &op {
            EditOp::Append { .. } | EditOp::StructMerge { .. } => {}
            EditOp::Replace { .. } | EditOp::StructRemove { .. } => {
                return Err(CoreContextError::PresetHookValidation(
                    "preset hook cannot use Replace or StructRemove; only Append and StructMerge are allowed".to_string(),
                ));
            }
        }

        // For PresetHook, we need to combine the hook metadata with the op.
        // The DerivationStep::PresetHook doesn't carry the op directly,
        // so we use apply_user_edit to actually apply the op, but wrap it.
        // Actually, looking at the spec more carefully: PresetHook is a derivation
        // kind, and the op is applied as part of the payload transformation.
        // Let's handle this by creating a combined step.
        //
        // The simplest approach: store the op in the derivation_detail via
        // a custom wrapper, but for V1.4 let's just apply the op directly.

        // We need to apply the edit op but record it as a PresetHook derivation.
        // Let's do a direct implementation:
        // H2: Serialize writes to prevent version chain corruption.
        let _guard = self.write_guard.lock().await;
        let now = chrono::Utc::now().timestamp();
        let schedule_id_str = &schedule_id.0;

        let current_version = self.current_version(schedule_id).await?;
        let new_version = CoreContextVersion(current_version.0 + 1);

        let previous_payload = {
            let record = self.read(schedule_id, current_version).await?;
            Some(record.content)
        };

        let new_payload = apply_edit_op(&previous_payload, &op)?;

        let payload_kind = match &new_payload {
            CoreContextPayload::Text { .. } => "text",
            CoreContextPayload::Struct { .. } => "struct",
        };
        let content_bytes = serde_json::to_vec(&new_payload)?;

        // For PresetHook derivation_detail, store a JSON with op details
        let detail_json = serde_json::json!({
            "state_id": state_id,
            "hook_name": hook_name,
            "op": op,
        });
        let derivation_json = serde_json::to_string(&detail_json)?;

        sqlx::query(
            r#"INSERT INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'system', NULL)"#,
        )
        .bind(schedule_id_str)
        .bind(new_version.0 as i64)
        .bind(payload_kind)
        .bind(&content_bytes)
        .bind("preset_hook")
        .bind(&derivation_json)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        // Bump the schedule's current_core_context_version
        sqlx::query(
            "UPDATE creator_schedules
             SET current_core_context_version = ?1, updated_at = ?2
             WHERE schedule_id = ?3",
        )
        .bind(new_version.0 as i64)
        .bind(now)
        .bind(schedule_id_str)
        .execute(&*self.pool)
        .await?;

        let step = DerivationStep::PresetHook {
            state_id: state_id.to_string(),
            hook_name: hook_name.to_string(),
        };

        Ok(CoreContextRecord {
            schedule_id: schedule_id.0.clone(),
            version: new_version,
            content: new_payload,
            derivation: step,
            created_at: now.to_string(),
            created_by: CoreContextAuthor::System,
        })
    }

    /// Apply a user edit derivation step.
    ///
    /// **H3**: `EditOp::Replace` is rejected to prevent overwriting
    /// system-managed fields (seed data, LLM summaries). Only `Append`,
    /// `StructMerge`, and `StructRemove` are allowed for user edits.
    pub async fn apply_user_edit(
        &self,
        schedule_id: &ScheduleId,
        op: EditOp,
        source_user: Option<String>,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // H3: Reject Replace to protect system-managed fields.
        if matches!(op, EditOp::Replace { .. }) {
            return Err(CoreContextError::UserEditValidation(
                "EditOp::Replace is not allowed for user edits; use Append or StructMerge instead"
                    .to_string(),
            ));
        }

        let author_id = source_user.clone().unwrap_or_default();
        let step = DerivationStep::UserEdit { op, source_user };
        let author = CoreContextAuthor::User { id: author_id };
        self.apply(schedule_id, step, author).await
    }

    /// Get the current (latest) version number for a schedule.
    pub async fn current_version(
        &self,
        schedule_id: &ScheduleId,
    ) -> Result<CoreContextVersion, CoreContextError> {
        let row = sqlx::query_as::<_, (i64,)>(
            "SELECT current_core_context_version
             FROM creator_schedules WHERE schedule_id = ?1",
        )
        .bind(&schedule_id.0)
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some((v,)) => Ok(CoreContextVersion(v as u32)),
            None => Err(CoreContextError::NotFound(schedule_id.0.clone())),
        }
    }

    /// Read a specific version of `core_context` for a schedule.
    pub async fn read(
        &self,
        schedule_id: &ScheduleId,
        version: CoreContextVersion,
    ) -> Result<CoreContextRecord, CoreContextError> {
        let row = sqlx::query_as::<_, CoreContextVersionRow>(
            "SELECT schedule_id, version, payload_kind, content,
                    derivation_kind, derivation_detail,
                    created_at, created_by_kind, created_by_user_id
             FROM core_context_versions
             WHERE schedule_id = ?1 AND version = ?2",
        )
        .bind(&schedule_id.0)
        .bind(version.0 as i64)
        .fetch_optional(&*self.pool)
        .await?
        .ok_or_else(|| CoreContextError::VersionNotFound(schedule_id.0.clone(), version.0))?;

        row.into_record()
    }

    /// Read the current (latest) snapshot of `core_context` for a schedule.
    pub async fn current_snapshot(
        &self,
        schedule_id: &ScheduleId,
    ) -> Result<CoreContextRecord, CoreContextError> {
        let version = self.current_version(schedule_id).await?;
        self.read(schedule_id, version).await
    }
}

/// Internal row mapping for reading `core_context_versions`.
#[derive(sqlx::FromRow)]
struct CoreContextVersionRow {
    schedule_id: String,
    version: i64,
    #[allow(dead_code)]
    payload_kind: String,
    content: Vec<u8>,
    derivation_kind: String,
    derivation_detail: Option<Vec<u8>>,
    created_at: i64,
    created_by_kind: String,
    created_by_user_id: Option<String>,
}

impl CoreContextVersionRow {
    fn into_record(self) -> Result<CoreContextRecord, CoreContextError> {
        let content: CoreContextPayload =
            serde_json::from_slice(&self.content).map_err(CoreContextError::Serde)?;

        let derivation = reconstruct_derivation(&self.derivation_kind, &self.derivation_detail)?;

        let created_by = match self.created_by_kind.as_str() {
            "user" => CoreContextAuthor::User {
                id: self.created_by_user_id.unwrap_or_default(),
            },
            _ => CoreContextAuthor::System,
        };

        Ok(CoreContextRecord {
            schedule_id: self.schedule_id,
            version: CoreContextVersion(self.version as u32),
            content,
            derivation,
            created_at: self.created_at.to_string(),
            created_by,
        })
    }
}

/// Reconstruct a [`DerivationStep`] from stored `derivation_kind` + `derivation_detail`.
fn reconstruct_derivation(
    kind: &str,
    detail: &Option<Vec<u8>>,
) -> Result<DerivationStep, CoreContextError> {
    match kind {
        "seed" => {
            if let Some(bytes) = detail {
                serde_json::from_slice(bytes).map_err(CoreContextError::Serde)
            } else {
                Ok(DerivationStep::Seed { raw: String::new() })
            }
        }
        "user_edit" => {
            if let Some(bytes) = detail {
                serde_json::from_slice(bytes).map_err(CoreContextError::Serde)
            } else {
                Ok(DerivationStep::UserEdit {
                    op: EditOp::Append {
                        body: String::new(),
                    },
                    source_user: None,
                })
            }
        }
        "preset_hook" => {
            if let Some(bytes) = detail {
                // Try to parse as PresetHook variant
                let json: serde_json::Value = serde_json::from_slice(bytes)?;
                Ok(DerivationStep::PresetHook {
                    state_id: json["state_id"].as_str().unwrap_or("").to_string(),
                    hook_name: json["hook_name"].as_str().unwrap_or("").to_string(),
                })
            } else {
                Ok(DerivationStep::PresetHook {
                    state_id: String::new(),
                    hook_name: String::new(),
                })
            }
        }
        "preset_seed_expansion" => {
            if let Some(bytes) = detail {
                serde_json::from_slice(bytes).map_err(CoreContextError::Serde)
            } else {
                Ok(DerivationStep::PresetSeedExpansion {
                    capability: String::new(),
                })
            }
        }
        "llm_summarize" => {
            if let Some(bytes) = detail {
                serde_json::from_slice(bytes).map_err(CoreContextError::Serde)
            } else {
                Ok(DerivationStep::llm_summarize(String::new(), [0u8; 32]))
            }
        }
        other => Err(CoreContextError::Serde(serde_json::Error::custom(format!(
            "unknown derivation_kind: {other}"
        )))),
    }
}

/// Apply a [`DerivationStep`] to compute the new payload from the previous one.
fn apply_step(
    previous: &Option<CoreContextPayload>,
    step: &DerivationStep,
    _author: &CoreContextAuthor,
) -> Result<CoreContextPayload, CoreContextError> {
    match step {
        DerivationStep::Seed { raw } => Ok(CoreContextPayload::Text { body: raw.clone() }),
        DerivationStep::UserEdit { op, .. } => apply_edit_op(previous, op),
        DerivationStep::PresetHook { .. } => {
            // PresetHook via apply() uses the op stored in derivation_detail
            // This path is used when apply() is called directly with a PresetHook step.
            // For the correct behavior, use apply_preset_hook() instead.
            Ok(previous.clone().unwrap_or(CoreContextPayload::Text {
                body: String::new(),
            }))
        }
        DerivationStep::LlmSummarize { .. } => {
            // V1.5+; not emitted by V1.4 code
            Ok(previous.clone().unwrap_or(CoreContextPayload::Text {
                body: String::new(),
            }))
        }
        DerivationStep::PresetSeedExpansion { .. } => {
            // V1.4 plumbing only; no actual expansion yet
            Ok(previous.clone().unwrap_or(CoreContextPayload::Text {
                body: String::new(),
            }))
        }
    }
}

/// Apply an [`EditOp`] to transform the payload.
fn apply_edit_op(
    previous: &Option<CoreContextPayload>,
    op: &EditOp,
) -> Result<CoreContextPayload, CoreContextError> {
    match op {
        EditOp::Replace { body } => Ok(CoreContextPayload::Text { body: body.clone() }),
        EditOp::Append { body } => {
            let prev_text = match previous {
                Some(CoreContextPayload::Text { body: prev }) => prev.as_str(),
                _ => "",
            };
            Ok(CoreContextPayload::Text {
                body: format!("{prev_text}{body}"),
            })
        }
        EditOp::StructMerge { patch } => {
            let prev_value = match previous {
                Some(CoreContextPayload::Struct { body }) => body.clone(),
                _ => serde_json::json!({}),
            };
            let merged = json_merge(&prev_value, patch);
            Ok(CoreContextPayload::Struct { body: merged })
        }
        EditOp::StructRemove { path } => {
            let prev_value = match previous {
                Some(CoreContextPayload::Struct { body }) => body.clone(),
                _ => serde_json::json!({}),
            };
            let mut map = match prev_value {
                serde_json::Value::Object(m) => m,
                other => {
                    return Err(CoreContextError::Serde(serde_json::Error::custom(format!(
                        "StructRemove requires struct payload, got: {other}"
                    ))))
                }
            };
            map.remove(path);
            Ok(CoreContextPayload::Struct {
                body: serde_json::Value::Object(map),
            })
        }
    }
}

/// Simple recursive JSON merge (patch keys overwrite base keys).
fn json_merge(base: &serde_json::Value, patch: &serde_json::Value) -> serde_json::Value {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            let mut merged = base_map.clone();
            for (key, value) in patch_map {
                if merged.contains_key(key) {
                    merged.insert(key.clone(), json_merge(&merged[key], value));
                } else {
                    merged.insert(key.clone(), value.clone());
                }
            }
            serde_json::Value::Object(merged)
        }
        (_, patch) => patch.clone(),
    }
}

/// Map a [`DerivationStep`] to its storage string tag.
fn derivation_kind_str(step: &DerivationStep) -> &'static str {
    match step {
        DerivationStep::Seed { .. } => "seed",
        DerivationStep::UserEdit { .. } => "user_edit",
        DerivationStep::PresetHook { .. } => "preset_hook",
        DerivationStep::LlmSummarize { .. } => "llm_summarize",
        DerivationStep::PresetSeedExpansion { .. } => "preset_seed_expansion",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::local::schedule::{
        CoreContextAuthor, CoreContextPayload, CoreContextVersion, EditOp,
        ScheduleId,
    };

    /// Helper: create a fresh test DB with migrations and return the pool.
    async fn fresh_pool() -> (Arc<SqlitePool>, tempfile::NamedTempFile) {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        (Arc::new(pool), db)
    }

    /// Helper: insert a minimal schedule row for testing.
    async fn insert_test_schedule(pool: &SqlitePool, schedule_id: &str) {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at)
               VALUES (?1, 'test-creator', 'test-preset', 1, 'pending',
                'serial', 0, ?2, ?2)"#,
        )
        .bind(schedule_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn core_context_derivation_progresses_versions() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // v0 from seed:
        let record0 = mgr
            .apply_seed(
                &sid,
                "topic=bees",
                CoreContextAuthor::User {
                    id: "u1".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(record0.version, CoreContextVersion(0));
        assert_eq!(
            mgr.current_version(&sid).await.unwrap(),
            CoreContextVersion(0)
        );

        // v1 from user edit:
        let record1 = mgr
            .apply_user_edit(
                &sid,
                EditOp::Append {
                    body: " vibe=literary".to_string(),
                },
                Some("u1".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(record1.version, CoreContextVersion(1));
        assert_eq!(
            mgr.current_version(&sid).await.unwrap(),
            CoreContextVersion(1)
        );

        // Read v1 content — should contain both parts
        let content = mgr.read(&sid, CoreContextVersion(1)).await.unwrap();
        match &content.content {
            CoreContextPayload::Text { body } => {
                assert!(body.contains("topic=bees"));
                assert!(body.contains("vibe=literary"));
            }
            _ => panic!("expected text payload"),
        }
    }

    #[tokio::test]
    async fn preset_hook_cannot_replace_only_append_or_merge() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(
            &sid,
            "v0",
            CoreContextAuthor::System,
        )
        .await
        .unwrap();

        // PresetHook with EditOp::Replace should be rejected:
        let err = mgr
            .apply_preset_hook(
                &sid,
                "st1",
                "h1",
                EditOp::Replace {
                    body: "nope".to_string(),
                },
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("preset hook"));
    }

    #[tokio::test]
    async fn preset_hook_append_succeeds() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(
            &sid,
            "initial",
            CoreContextAuthor::System,
        )
        .await
        .unwrap();

        // PresetHook with Append should succeed
        let record = mgr
            .apply_preset_hook(
                &sid,
                "st1",
                "h1",
                EditOp::Append {
                    body: " appended".to_string(),
                },
            )
            .await
            .unwrap();
        assert_eq!(record.version, CoreContextVersion(1));

        // Content should be concatenated
        let snapshot = mgr.current_snapshot(&sid).await.unwrap();
        match &snapshot.content {
            CoreContextPayload::Text { body } => {
                assert_eq!(body, "initial appended");
            }
            _ => panic!("expected text payload"),
        }
    }

    #[tokio::test]
    async fn current_snapshot_returns_latest() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(
            &sid,
            "first",
            CoreContextAuthor::System,
        )
        .await
        .unwrap();

        // Append v1
        mgr.apply_user_edit(
            &sid,
            EditOp::Append {
                body: " second".to_string(),
            },
            None,
        )
        .await
        .unwrap();

        // Append v2 (using Append instead of Replace, which is now rejected for user edits)
        mgr.apply_user_edit(
            &sid,
            EditOp::Append {
                body: " third".to_string(),
            },
            None,
        )
        .await
        .unwrap();

        let snapshot = mgr.current_snapshot(&sid).await.unwrap();
        assert_eq!(snapshot.version, CoreContextVersion(2));
        match &snapshot.content {
            CoreContextPayload::Text { body } => assert_eq!(body, "first second third"),
            _ => panic!("expected text payload"),
        }
    }

    // ---------- H3: Replace rejected in user edit ----------

    #[tokio::test]
    async fn user_edit_rejects_replace() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(
            &sid,
            "initial seed",
            CoreContextAuthor::System,
        )
        .await
        .unwrap();

        // User edit with Replace should be rejected
        let err = mgr
            .apply_user_edit(
                &sid,
                EditOp::Replace {
                    body: "overwritten".to_string(),
                },
                Some("u1".to_string()),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Replace"));
        assert!(err.to_string().contains("not allowed"));

        // Version should still be 0
        assert_eq!(
            mgr.current_version(&sid).await.unwrap(),
            CoreContextVersion(0),
            "version should remain at 0 after rejected Replace"
        );
    }

    #[tokio::test]
    async fn user_edit_allows_append_and_struct_merge() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0 with struct
        mgr.apply_seed(
            &sid,
            "{}",
            CoreContextAuthor::System,
        )
        .await
        .unwrap();

        // User edit with StructMerge should succeed
        let record = mgr
            .apply_user_edit(
                &sid,
                EditOp::StructMerge {
                    patch: serde_json::json!({"key": "value"}),
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(record.version, CoreContextVersion(1));
    }

    #[test]
    fn json_merge_shallow() {
        let base = serde_json::json!({"a": 1, "b": 2});
        let patch = serde_json::json!({"b": 3, "c": 4});
        let merged = json_merge(&base, &patch);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"], 3);
        assert_eq!(merged["c"], 4);
    }
}
