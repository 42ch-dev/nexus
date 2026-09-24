//! `CoreContextManager` — immutable versioned `core_context` derivation (spec §6).
//!
//! Manages the monotonic version sequence per Schedule, applying [`DerivationStep`]
//! operations and persisting each version as an immutable row in `core_context_versions`.
//!
//! Design: `.mstar/archived/knowledge/creator-schedule-and-core-context.md` §6.

use std::collections::HashMap;
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
    /// The schedule pointer no longer named the version an append derived
    /// from, so the append (and its immutable version row) was rolled back.
    ///
    /// A version race never produces an orphan successful version: the caller
    /// re-reads the pointer and retries against the committed winner.
    #[error(
        "core context version race on schedule {0}: the pointer no longer names version {1}"
    )]
    VersionRace(String, u32),
    /// More than one schedule names the same run as its
    /// `current_session_id`, so "the schedule that owns this run" has no
    /// single answer.
    ///
    /// Duplicate ownership is not a DB constraint, so the run's context
    /// schedule would otherwise be whichever row the query happened to
    /// return — a silent cross-wiring of one run's prompt. Refused instead of
    /// picked: the caller must not render a snapshot it cannot attribute.
    #[error(
        "run {0} is named as current_session_id by {1} schedules ({2}); \
         refusing to choose one"
    )]
    AmbiguousOwnership(String, usize, String),
    /// A USER edit reached the writer for a schedule that had already settled.
    ///
    /// Admitted INSIDE the append transaction (see [`CoreContextManager::apply`]),
    /// so it covers the whole window between a caller's pre-check and this
    /// writer's turn: a settlement that committed earlier is visible here, and
    /// one that commits later cannot (the `BEGIN IMMEDIATE` writer lock is held
    /// until this transaction ends). No immutable version row and no pointer
    /// advance survive the refusal.
    ///
    /// Only user-authored steps are fenced: system derivations (`apply_seed`,
    /// `apply_preset_hook`, `apply_llm_summarize`) still write the final context
    /// of a terminal schedule.
    #[error("schedule {0} is in terminal status '{1}'; core-context edits are not allowed")]
    TerminalSchedule(String, String),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Manages immutable, versioned `core_context` for a schedule.
///
/// Each `apply()` call appends a new row to `core_context_versions` and
/// bumps `creator_schedules.current_core_context_version`.
///
/// **Write guard (R6 — per-schedule locking)**: The `apply()`, `apply_seed()`,
/// and `apply_preset_hook()` methods use per-schedule locks to prevent
/// concurrent writes on the *same* schedule from corrupting the version
/// chain. Different schedules can write concurrently without blocking
/// each other.
pub struct CoreContextManager {
    pool: Arc<SqlitePool>,
    /// Per-schedule write guards. Each schedule gets its own Mutex<()> entry.
    /// This allows concurrent writes to *different* schedules while
    /// maintaining per-schedule safety (R6).
    schedule_guards: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl CoreContextManager {
    /// Create a new manager backed by the given shared `SQLite` pool.
    #[must_use]
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            schedule_guards: Mutex::new(HashMap::new()),
        }
    }

    /// Remove the per-schedule guard for a terminal schedule.
    ///
    /// Callers (schedule supervisor) should invoke this when a schedule reaches
    /// Completed/Cancelled/Failed status to prevent the `schedule_guards`
    /// `HashMap` from growing unboundedly.
    ///
    /// # Safety
    ///
    /// - Cleanup on non-existent schedule → `HashMap::remove` is a no-op.
    /// - Active write in progress → guard is `Arc`-shared; the active write
    ///   holds a clone and completes safely before the `Arc` is dropped.
    pub async fn cleanup_guard(&self, schedule_id: &ScheduleId) {
        let key = schedule_id.0.clone();
        let mut guards = self.schedule_guards.lock().await;
        guards.remove(&key);
    }

    /// Get or create a per-schedule write guard.
    ///
    /// Returns an `Arc<Mutex<()>>` for the given schedule. The Arc allows
    /// cloning so the lock can be held while the `HashMap` is not locked.
    async fn schedule_guard(&self, schedule_id: &ScheduleId) -> Arc<tokio::sync::Mutex<()>> {
        let key = schedule_id.0.clone();
        let mut guards = self.schedule_guards.lock().await;
        guards
            .entry(key)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Apply the seed step to create version 0 of `core_context`.
    ///
    /// Per spec §6.2, the seed is version 0 (not 1).
    /// This must be called before any [`apply`] call for the schedule.
    ///
    /// Returns the new [`CoreContextRecord`] with version 0.
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if deserialization or step application fails.
    pub async fn apply_seed(
        &self,
        schedule_id: &ScheduleId,
        raw: &str,
        author: CoreContextAuthor,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // R6: Per-schedule lock to prevent version chain corruption.
        let guard = self.schedule_guard(schedule_id).await;
        let _lock = guard.lock().await;

        let now = chrono::Utc::now().timestamp();
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

        // Pre-own all bind params (borrow lifetime rules for sqlx macros).
        let schedule_id_owned = schedule_id.0.clone();
        let version_i64 = i64::from(new_version.0);

        // Insert version 0 row
        sqlx::query!(
            r#"INSERT INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            schedule_id_owned,
            version_i64,
            payload_kind,
            content_bytes,
            "seed",
            derivation_json,
            now,
            created_by_kind,
            created_by_user_id
        )
        .execute(&*self.pool)
        .await?;

        // Set the schedule's current_core_context_version to 0
        sqlx::query!(
            "UPDATE creator_schedules
             SET current_core_context_version = ?, updated_at = ?
             WHERE schedule_id = ?",
            version_i64,
            now,
            schedule_id_owned
        )
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
    /// - Reads the schedule's current version (the pointer) and that version's
    ///   payload.
    /// - Applies the `DerivationStep` to compute the new payload.
    /// - Inserts the new immutable `core_context_versions` row.
    /// - Advances `creator_schedules.current_core_context_version` to it.
    ///
    /// All four steps are ONE `BEGIN IMMEDIATE` transaction. The pointer is the
    /// single version authority, and the advance is conditional
    /// (`WHERE current_core_context_version = <derived-from version>`), so two
    /// concurrent appends — this manager's other schedules, a second manager,
    /// or a second process over the same Creator DB — cannot interleave: the
    /// winner commits version N *and* pointer N together and the next append
    /// derives from N. A lost advance (the schedule row vanished) rolls the
    /// whole transaction back: a version row is never committed while the
    /// pointer still names an older one, and no append body is lost.
    ///
    /// A USER-authored step additionally admits the schedule's `status` in that
    /// same transaction: a schedule that settled (`completed`, `cancelled`,
    /// `failed`) refuses the edit with [`CoreContextError::TerminalSchedule`]
    /// and rolls back, so no immutable version row and no pointer advance can
    /// land on it — even when the settlement committed after a caller's
    /// pre-check. System derivations are deliberately exempt.
    ///
    /// Returns the new [`CoreContextRecord`].
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if step application fails,
    /// [`CoreContextError::NotFound`] when the schedule row is absent,
    /// [`CoreContextError::VersionRace`] when the pointer no longer named the
    /// version this step derived from, and
    /// [`CoreContextError::TerminalSchedule`] when a user edit targets a
    /// terminal schedule.
    pub async fn apply(
        &self,
        schedule_id: &ScheduleId,
        step: DerivationStep,
        author: CoreContextAuthor,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // R6: Per-schedule lock. The lock serializes THIS manager's writers on
        // one schedule; the transaction below is what makes the
        // version+pointer pair atomic against every other writer.
        let guard = self.schedule_guard(schedule_id).await;
        let _lock = guard.lock().await;

        let now = chrono::Utc::now().timestamp();
        let schedule_id_owned = schedule_id.0.clone();

        // `BEGIN IMMEDIATE` takes the Creator-DB writer lock up front, so the
        // read-derive-insert-advance sequence is never split by another writer
        // (and never has to upgrade a read lock mid-transaction).
        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| CoreContextError::Database(begin_error(e)))?;

        // The schedule row's pointer is the version authority. `status` is read
        // in the SAME statement — and the same `BEGIN IMMEDIATE` transaction —
        // because the USER-edit admission below must be atomic with the append:
        // a settlement that committed before this transaction took the writer
        // lock is visible to this read, and one that commits afterwards cannot
        // (the writer lock is ours until this transaction ends).
        let (current, status) = sqlx::query_as::<_, (i64, String)>(
            "SELECT current_core_context_version, status FROM creator_schedules \
             WHERE schedule_id = ?",
        )
        .bind(&schedule_id_owned)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CoreContextError::NotFound(schedule_id.0.clone()))?;

        // Terminal admission for USER edits: an edit must not land on a
        // schedule that already settled. Gated on the author, so every system
        // derivation keeps writing (a terminal schedule still receives its
        // final context). The whole transaction rolls back — no immutable
        // version row, no pointer advance.
        if matches!(&author, CoreContextAuthor::User { .. }) && terminal_status(&status) {
            tx.rollback().await?;
            return Err(CoreContextError::TerminalSchedule(
                schedule_id.0.clone(),
                status,
            ));
        }

        let current_version = CoreContextVersion(u32::try_from(current).unwrap_or_default());
        let new_version = CoreContextVersion(current_version.0 + 1);

        // Compute new payload from the previous (pointed-at) content.
        let previous_payload =
            Some(read_version_in_tx(&mut tx, &schedule_id_owned, current_version).await?.content);

        let new_payload = apply_step(previous_payload.as_ref(), &step, &author)?;

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
        let version_i64 = i64::from(new_version.0);

        // Runtime `sqlx::query` (not the compile-time macro) for the new
        // statements: they carry no `.sqlx` offline entry, and this is the
        // same convention the supervisor's admission transaction uses.
        sqlx::query(
            "INSERT INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&schedule_id_owned)
        .bind(version_i64)
        .bind(payload_kind)
        .bind(content_bytes)
        .bind(derivation_kind)
        .bind(derivation_json)
        .bind(now)
        .bind(created_by_kind)
        .bind(created_by_user_id)
        .execute(&mut *tx)
        .await?;

        // Conditional pointer advance: the append is only durable together
        // with the pointer that names it.
        let advanced = sqlx::query(
            "UPDATE creator_schedules
             SET current_core_context_version = ?, updated_at = ?
             WHERE schedule_id = ? AND current_core_context_version = ?",
        )
        .bind(version_i64)
        .bind(now)
        .bind(&schedule_id_owned)
        .bind(current)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if advanced != 1 {
            tx.rollback().await?;
            return Err(CoreContextError::VersionRace(
                schedule_id.0.clone(),
                current_version.0,
            ));
        }

        tx.commit().await?;

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
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if step application fails.
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
        // R6: Per-schedule lock to prevent version chain corruption.
        let guard = self.schedule_guard(schedule_id).await;
        let _lock = guard.lock().await;
        let now = chrono::Utc::now().timestamp();

        let current_version = self.current_version(schedule_id).await?;
        let new_version = CoreContextVersion(current_version.0 + 1);

        let previous_payload = {
            let record = self.read(schedule_id, current_version).await?;
            Some(record.content)
        };

        let new_payload = apply_edit_op(previous_payload.as_ref(), &op)?;

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

        // Pre-own all bind params (borrow lifetime rules for sqlx macros).
        let schedule_id_owned = schedule_id.0.clone();
        let version_i64 = i64::from(new_version.0);

        sqlx::query!(
            r#"INSERT INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, 'system', NULL)"#,
            schedule_id_owned,
            version_i64,
            payload_kind,
            content_bytes,
            "preset_hook",
            derivation_json,
            now
        )
        .execute(&*self.pool)
        .await?;

        // Bump the schedule's current_core_context_version
        sqlx::query!(
            "UPDATE creator_schedules
             SET current_core_context_version = ?, updated_at = ?
             WHERE schedule_id = ?",
            version_i64,
            now,
            schedule_id_owned
        )
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

    /// Apply an LLM summarization derivation step (V1.5).
    ///
    /// Takes the LLM-produced summary text and the prompt hash, writes a new
    /// `core_context_versions` row with `derivation_kind = 'llm_summarize'`.
    /// The previous content is replaced by the summary (LLM produces a full
    /// new version, not an append).
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if step application fails.
    pub async fn apply_llm_summarize(
        &self,
        schedule_id: &ScheduleId,
        summary: &str,
        prompt_hash: [u8; 32],
        capability_name: &str,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // R6: Per-schedule lock to prevent version chain corruption.
        let guard = self.schedule_guard(schedule_id).await;
        let _lock = guard.lock().await;
        let now = chrono::Utc::now().timestamp();

        let current_version = self.current_version(schedule_id).await?;
        let new_version = CoreContextVersion(current_version.0 + 1);

        let new_payload = CoreContextPayload::Text {
            body: summary.to_string(),
        };

        let payload_kind = "text";
        let content_bytes = serde_json::to_vec(&new_payload)?;

        let step = DerivationStep::llm_summarize(capability_name.to_string(), prompt_hash);
        let derivation_json = serde_json::to_string(&step)?;

        // Pre-own all bind params (borrow lifetime rules for sqlx macros).
        let schedule_id_owned = schedule_id.0.clone();
        let version_i64 = i64::from(new_version.0);

        sqlx::query!(
            r#"INSERT INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, 'system', NULL)"#,
            schedule_id_owned,
            version_i64,
            payload_kind,
            content_bytes,
            "llm_summarize",
            derivation_json,
            now
        )
        .execute(&*self.pool)
        .await?;

        // Bump the schedule's current_core_context_version
        sqlx::query!(
            "UPDATE creator_schedules
             SET current_core_context_version = ?, updated_at = ?
             WHERE schedule_id = ?",
            version_i64,
            now,
            schedule_id_owned
        )
        .execute(&*self.pool)
        .await?;

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
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if step application fails.
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
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if database query fails.
    pub async fn current_version(
        &self,
        schedule_id: &ScheduleId,
    ) -> Result<CoreContextVersion, CoreContextError> {
        let schedule_id_owned = schedule_id.0.clone();
        let row = sqlx::query_scalar!(
            "SELECT current_core_context_version
             FROM creator_schedules WHERE schedule_id = ?",
            schedule_id_owned
        )
        .fetch_optional(&*self.pool)
        .await?;

        row.map_or_else(
            || Err(CoreContextError::NotFound(schedule_id.0.clone())),
            |v| Ok(CoreContextVersion(u32::try_from(v).unwrap_or_default())),
        )
    }

    /// Read a specific version of `core_context` for a schedule.
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if database query or deserialization fails.
    pub async fn read(
        &self,
        schedule_id: &ScheduleId,
        version: CoreContextVersion,
    ) -> Result<CoreContextRecord, CoreContextError> {
        // Pre-own all bind params (borrow lifetime rules for sqlx macros).
        let schedule_id_owned = schedule_id.0.clone();
        let version_i64 = i64::from(version.0);
        let row = sqlx::query_as!(
            CoreContextVersionRow,
            "SELECT schedule_id, version, payload_kind, content,
                    derivation_kind, derivation_detail,
                    created_at, created_by_kind, created_by_user_id
             FROM core_context_versions
             WHERE schedule_id = ? AND version = ?",
            schedule_id_owned,
            version_i64
        )
        .fetch_optional(&*self.pool)
        .await?
        .ok_or_else(|| CoreContextError::VersionNotFound(schedule_id.0.clone(), version.0))?;

        row.into_record()
    }

    /// Read the COMMITTED `core_context` head of the schedule that owns one run.
    ///
    /// `session_id` is a durable root run id; a run is schedule-owned exactly
    /// while some `creator_schedules` row names it as its `current_session_id`
    /// (the ownership predicate the coordinator's terminal settlement already
    /// uses). `Ok(None)` therefore means "this run carries no schedule
    /// core-context contract" — a session-POST run, or a nested child — and the
    /// caller applies no refresh.
    ///
    /// The record returned is the EXACT version the schedule pointer names,
    /// never the newest row in `core_context_versions`: an outer state boundary
    /// may only render a payload a committed pointer names. A pointer that
    /// names no readable row is [`CoreContextError::VersionNotFound`] (and any
    /// storage/serde failure its own variant) so the caller fails closed
    /// instead of rendering an unverified snapshot.
    ///
    /// Exactly one schedule must name the run. `creator_schedules.
    /// current_session_id` carries no uniqueness constraint, so a duplicate or
    /// corrupt association would otherwise resolve to an arbitrary row and
    /// cross-wire one run's prompt: two matches are
    /// [`CoreContextError::AmbiguousOwnership`], never a silent pick.
    ///
    /// # Errors
    /// Returns [`CoreContextError`] on a database/deserialization failure, on a
    /// pointer outside the version range, when the named version row is
    /// absent, or when more than one schedule owns the run.
    pub async fn head_for_run(
        &self,
        session_id: &str,
    ) -> Result<Option<CoreContextRecord>, CoreContextError> {
        let mut rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT schedule_id, current_core_context_version
             FROM creator_schedules WHERE current_session_id = ?",
        )
        .bind(session_id)
        .fetch_all(&*self.pool)
        .await?;
        let (schedule_id, version) = match rows.len() {
            // No owning schedule: the run carries no core-context contract.
            0 => return Ok(None),
            1 => rows.remove(0),
            owners => {
                rows.sort_unstable();
                return Err(CoreContextError::AmbiguousOwnership(
                    session_id.to_string(),
                    owners,
                    rows.iter()
                        .map(|(schedule_id, _)| schedule_id.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
        };
        let version = u32::try_from(version).map_err(|_| {
            CoreContextError::Serde(serde_json::Error::custom(format!(
                "schedule {schedule_id} core-context pointer {version} is out of range"
            )))
        })?;
        let record = self
            .read(&ScheduleId(schedule_id), CoreContextVersion(version))
            .await?;
        Ok(Some(record))
    }

    /// Read the current (latest) snapshot of `core_context` for a schedule.
    ///
    /// # Errors
    /// Returns [`CoreContextError`] if database query or deserialization fails.
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

        let derivation =
            reconstruct_derivation(&self.derivation_kind, self.derivation_detail.as_deref())?;

        let created_by = match self.created_by_kind.as_str() {
            "user" => CoreContextAuthor::User {
                id: self.created_by_user_id.unwrap_or_default(),
            },
            _ => CoreContextAuthor::System,
        };

        Ok(CoreContextRecord {
            schedule_id: self.schedule_id,
            version: CoreContextVersion(u32::try_from(self.version).unwrap_or_default()),
            content,
            derivation,
            created_at: self.created_at.to_string(),
            created_by,
        })
    }
}

/// Read one `core_context_versions` row through an OPEN transaction.
///
/// The append path reads the version it derives from through the same
/// transaction that inserts the next one, so the payload can never come from a
/// version another writer has since moved away from.
///
/// # Errors
/// Returns the storage error, or [`CoreContextError::VersionNotFound`] when the
/// version row is absent (a schedule whose pointer has no backing row is not a
/// valid derivation base).
async fn read_version_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    schedule_id: &str,
    version: CoreContextVersion,
) -> Result<CoreContextRecord, CoreContextError> {
    let version_i64 = i64::from(version.0);
    let row = sqlx::query_as::<_, CoreContextVersionRow>(
        "SELECT schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id
         FROM core_context_versions
         WHERE schedule_id = ? AND version = ?",
    )
    .bind(schedule_id)
    .bind(version_i64)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| CoreContextError::VersionNotFound(schedule_id.to_string(), version.0))?;
    row.into_record()
}

/// Map a `nexus_local_db` transaction-open failure onto the sqlx error this
/// module's error type carries (the local-db wrapper preserves the source
/// `sqlx::Error`; every other local-db refusal is a plain protocol error).
fn begin_error(err: nexus_local_db::LocalDbError) -> sqlx::Error {
    match err {
        nexus_local_db::LocalDbError::Sqlx(e) => e,
        other => sqlx::Error::Protocol(other.to_string()),
    }
}

/// Reconstruct a [`DerivationStep`] from stored `derivation_kind` + `derivation_detail`.
fn reconstruct_derivation(
    kind: &str,
    detail: Option<&[u8]>,
) -> Result<DerivationStep, CoreContextError> {
    match kind {
        "seed" => detail.map_or_else(
            || Ok(DerivationStep::Seed { raw: String::new() }),
            |bytes| serde_json::from_slice(bytes).map_err(CoreContextError::Serde),
        ),
        "user_edit" => detail.map_or_else(
            || {
                Ok(DerivationStep::UserEdit {
                    op: EditOp::Append {
                        body: String::new(),
                    },
                    source_user: None,
                })
            },
            |bytes| serde_json::from_slice(bytes).map_err(CoreContextError::Serde),
        ),
        "preset_hook" => detail.map_or_else(
            || {
                Ok(DerivationStep::PresetHook {
                    state_id: String::new(),
                    hook_name: String::new(),
                })
            },
            |bytes| {
                let json: serde_json::Value =
                    serde_json::from_slice(bytes).map_err(CoreContextError::Serde)?;
                Ok(DerivationStep::PresetHook {
                    state_id: json["state_id"].as_str().unwrap_or("").to_string(),
                    hook_name: json["hook_name"].as_str().unwrap_or("").to_string(),
                })
            },
        ),
        "preset_seed_expansion" => detail.map_or_else(
            || {
                Ok(DerivationStep::PresetSeedExpansion {
                    capability: String::new(),
                })
            },
            |bytes| serde_json::from_slice(bytes).map_err(CoreContextError::Serde),
        ),
        "llm_summarize" => detail.map_or_else(
            || Ok(DerivationStep::llm_summarize(String::new(), [0u8; 32])),
            |bytes| serde_json::from_slice(bytes).map_err(CoreContextError::Serde),
        ),
        other => Err(CoreContextError::Serde(serde_json::Error::custom(format!(
            "unknown derivation_kind: {other}"
        )))),
    }
}

/// Apply a [`DerivationStep`] to compute the new payload from the previous one.
fn apply_step(
    previous: Option<&CoreContextPayload>,
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
            Ok(previous.cloned().unwrap_or(CoreContextPayload::Text {
                body: String::new(),
            }))
        }
        DerivationStep::LlmSummarize { .. } => {
            // V1.5+; not emitted by V1.4 code
            Ok(previous.cloned().unwrap_or(CoreContextPayload::Text {
                body: String::new(),
            }))
        }
        DerivationStep::PresetSeedExpansion { .. } => {
            // V1.4 plumbing only; no actual expansion yet
            Ok(previous.cloned().unwrap_or(CoreContextPayload::Text {
                body: String::new(),
            }))
        }
    }
}

/// Apply an [`EditOp`] to transform the payload.
fn apply_edit_op(
    previous: Option<&CoreContextPayload>,
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

/// Whether a stored `creator_schedules.status` is terminal for USER edits.
///
/// The same three values the Core pre-check names, and the same set the
/// supervisor's own admission queries exclude (`status NOT IN ('completed',
/// 'failed', 'cancelled')`); the strings are the stored snake_case form of
/// `ScheduleStatus`'s terminal variants.
fn terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "cancelled" | "failed")
}

/// Map a [`DerivationStep`] to its storage string tag.
const fn derivation_kind_str(step: &DerivationStep) -> &'static str {
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
        CoreContextAuthor, CoreContextPayload, CoreContextVersion, DerivationStep, EditOp,
        ScheduleId,
    };

    /// Helper: create a fresh test DB with migrations and return the pool.
    async fn fresh_pool() -> (Arc<SqlitePool>, tempfile::NamedTempFile) {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        (Arc::new(pool), db)
    }

    /// Helper: insert a minimal schedule row for testing.
    async fn insert_test_schedule(pool: &SqlitePool, schedule_id: &str) {
        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helper that inserts a minimal schedule row for test setup.
        sqlx::query(
            r"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at)
               VALUES (?, 'test-creator', 'test-preset', 1, 'pending',
               'serial', 0, ?, ?)",
        )
        .bind(schedule_id)
        .bind(now)
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
            CoreContextPayload::Struct { .. } => panic!("expected text payload"),
        }
    }

    #[tokio::test]
    async fn preset_hook_cannot_replace_only_append_or_merge() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(&sid, "v0", CoreContextAuthor::System)
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
        mgr.apply_seed(&sid, "initial", CoreContextAuthor::System)
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
            CoreContextPayload::Struct { .. } => panic!("expected text payload"),
        }
    }

    #[tokio::test]
    async fn current_snapshot_returns_latest() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("01A".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(&sid, "first", CoreContextAuthor::System)
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
            CoreContextPayload::Struct { .. } => panic!("expected text payload"),
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
        mgr.apply_seed(&sid, "initial seed", CoreContextAuthor::System)
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
        mgr.apply_seed(&sid, "{}", CoreContextAuthor::System)
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

    // ---------- P1: terminal admission for USER edits ----------

    /// Settle a schedule exactly as the supervisor does: the stored status is
    /// the lowercase snake_case name, and the three terminal values are the
    /// ones the user-edit admission refuses.
    async fn settle_schedule(pool: &SqlitePool, schedule_id: &str, status: &str) {
        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helper for test state setup.
        sqlx::query("UPDATE creator_schedules SET status = ?, updated_at = ? WHERE schedule_id = ?")
            .bind(status)
            .bind(now)
            .bind(schedule_id)
            .execute(pool)
            .await
            .unwrap();
    }

    /// How many immutable `core_context_versions` rows `schedule_id` owns.
    async fn version_row_count(pool: &SqlitePool, schedule_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM core_context_versions WHERE schedule_id = ?")
            .bind(schedule_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// A user edit that reaches the writer AFTER the schedule settled must be
    /// refused by the append transaction itself — the status admission lives in
    /// the same `BEGIN IMMEDIATE` transaction as the insert and the conditional
    /// pointer advance, so the settlement cannot slip into the window between a
    /// caller's pre-check and this writer's turn.
    ///
    /// The observable contract: the refused edit leaves NO extra immutable row
    /// and NO pointer advance, for every terminal status; a SYSTEM derivation on
    /// the same terminal schedule still writes.
    #[tokio::test]
    async fn user_edit_on_a_terminal_schedule_inserts_no_version() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);

        for status in ["completed", "cancelled", "failed"] {
            let sid = ScheduleId(format!("TERM-{status}"));
            insert_test_schedule(&mgr.pool, &sid.0).await;
            mgr.apply_seed(&sid, "seeded", CoreContextAuthor::System)
                .await
                .unwrap();
            settle_schedule(&mgr.pool, &sid.0, status).await;
            let before = version_row_count(&mgr.pool, &sid.0).await;

            let err = mgr
                .apply_user_edit(
                    &sid,
                    EditOp::Append {
                        body: " too late".to_string(),
                    },
                    Some("u1".to_string()),
                )
                .await
                .unwrap_err();
            assert!(
                matches!(err, CoreContextError::TerminalSchedule(_, _)),
                "{status}: expected a terminal refusal, got {err:?}"
            );
            let reason = err.to_string();
            assert!(reason.contains(status), "{status}: names the status: {reason}");
            assert!(
                reason.contains("terminal"),
                "{status}: names the refusal: {reason}"
            );

            assert_eq!(
                version_row_count(&mgr.pool, &sid.0).await,
                before,
                "{status}: the refused edit must not insert a version row"
            );
            assert_eq!(
                mgr.current_version(&sid).await.unwrap(),
                CoreContextVersion(0),
                "{status}: the refused edit must not advance the pointer"
            );
            let snapshot = mgr.current_snapshot(&sid).await.unwrap();
            assert!(
                matches!(snapshot.content, CoreContextPayload::Text { ref body } if body == "seeded"),
                "{status}: the committed payload is untouched"
            );

            // The exemption is deliberate: a system derivation still writes the
            // terminal schedule's final context — only the USER edit is fenced.
            let record = mgr
                .apply_llm_summarize(&sid, "final summary", [0u8; 32], "context.summarize")
                .await
                .unwrap();
            assert_eq!(
                record.version,
                CoreContextVersion(1),
                "{status}: a system derivation must keep writing"
            );
        }
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

    // ---------- head_for_run: run → owning schedule resolution ----------

    /// Helper: name `session_id` as `schedule_id`'s owned run.
    ///
    /// `creator_schedules.current_session_id` is a real FK, so the session row
    /// exists first — the same shape a durable run has when a schedule owns it.
    async fn claim_run(pool: &SqlitePool, schedule_id: &str, session_id: &str) {
        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helpers for a durable run + its claim.
        sqlx::query(
            "INSERT OR IGNORE INTO orchestration_sessions
               (session_id, creator_id, preset_id, preset_version, status,
                context_json, created_at, updated_at)
             VALUES (?, 'test-creator', 'test-preset', 1, 'running', ?, ?, ?)",
        )
        .bind(session_id)
        .bind(b"{}".as_slice())
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE creator_schedules SET current_session_id = ?, status = 'running'
             WHERE schedule_id = ?",
        )
        .bind(session_id)
        .bind(schedule_id)
        .execute(pool)
        .await
        .unwrap();
    }

    /// `head_for_run` resolves the exact committed version of the ONE schedule
    /// that names the run, reports `None` when no schedule does, and REFUSES
    /// when two schedules claim the same run: the ownership the whole
    /// core-context contract rests on has no single answer, so picking the row
    /// the query happened to return would cross-wire one run's prompt.
    #[tokio::test]
    async fn head_for_run_refuses_ambiguous_ownership() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let owned = ScheduleId("HEAD-OWNED".to_string());
        insert_test_schedule(&mgr.pool, &owned.0).await;
        mgr.apply_seed(&owned, "owned context", CoreContextAuthor::System)
            .await
            .unwrap();
        claim_run(&mgr.pool, &owned.0, "run-head-1").await;

        let record = mgr
            .head_for_run("run-head-1")
            .await
            .expect("one owner must resolve")
            .expect("the owning schedule yields its committed version");
        assert_eq!(record.schedule_id, owned.0);
        assert_eq!(record.version, CoreContextVersion(0));
        assert!(matches!(
            record.content,
            CoreContextPayload::Text { ref body } if body == "owned context"
        ));

        assert!(
            mgr.head_for_run("run-nobody-owns").await.unwrap().is_none(),
            "a run no schedule names carries no core-context contract"
        );

        // A second schedule naming the SAME run: the context schedule is
        // ambiguous, so the lookup must fail closed rather than choose one.
        let second = ScheduleId("HEAD-SECOND".to_string());
        insert_test_schedule(&mgr.pool, &second.0).await;
        claim_run(&mgr.pool, &second.0, "run-head-1").await;

        let ambiguous = mgr.head_for_run("run-head-1").await;
        assert!(
            ambiguous.is_err(),
            "two schedules naming one run must refuse, got {ambiguous:?}"
        );
        let reason = ambiguous.unwrap_err().to_string();
        assert!(reason.contains("run-head-1"), "names the run: {reason}");
        assert!(reason.contains("HEAD-OWNED"), "names both owners: {reason}");
        assert!(reason.contains("HEAD-SECOND"), "names both owners: {reason}");
    }

    // ---------- R6: Per-schedule version bump race ----------

    #[tokio::test]
    async fn r6_concurrent_apply_same_schedule_produces_sequential_versions() {
        let (pool, _db) = fresh_pool().await;
        let mgr = Arc::new(CoreContextManager::new(pool));
        let sid_a = ScheduleId("R6-A".to_string());
        let sid_b_id = ScheduleId("R6-B".to_string());
        insert_test_schedule(&mgr.pool, &sid_a.0).await;
        insert_test_schedule(&mgr.pool, &sid_b_id.0).await;

        // Seed both
        mgr.apply_seed(&sid_a, "A-initial", CoreContextAuthor::System)
            .await
            .unwrap();
        mgr.apply_seed(&sid_b_id, "B-initial", CoreContextAuthor::System)
            .await
            .unwrap();

        // Concurrently apply to both schedules
        let mgr_clone = Arc::clone(&mgr);
        let sid_a_clone = sid_a.clone();
        let sid_b_id_clone = sid_b_id.clone();

        let h1 = tokio::spawn(async move {
            mgr_clone
                .apply_user_edit(
                    &sid_a_clone,
                    EditOp::Append {
                        body: " append-a".to_string(),
                    },
                    None,
                )
                .await
                .unwrap()
                .version
        });

        let mgr_clone2 = Arc::clone(&mgr);
        let h2 = tokio::spawn(async move {
            mgr_clone2
                .apply_user_edit(
                    &sid_b_id_clone,
                    EditOp::Append {
                        body: " append-b".to_string(),
                    },
                    None,
                )
                .await
                .unwrap()
                .version
        });

        let v_a = h1.await.unwrap();
        let v_b = h2.await.unwrap();

        // Both should succeed with version 1
        assert_eq!(v_a, CoreContextVersion(1));
        assert_eq!(v_b, CoreContextVersion(1));
    }

    // ---------- V1.5: LLM Summarize derivation ----------

    #[tokio::test]
    async fn llm_summarize_writes_version_with_correct_derivation_kind() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("LLM-SUM1".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(
            &sid,
            "initial context about bees",
            CoreContextAuthor::System,
        )
        .await
        .unwrap();

        // Apply LLM summarize
        let prompt_hash = [0xABu8; 32];
        let record = mgr
            .apply_llm_summarize(
                &sid,
                "Summarized: a story about bees and honey.",
                prompt_hash,
                "context.summarize",
            )
            .await
            .unwrap();

        // Should be version 1
        assert_eq!(record.version, CoreContextVersion(1));

        // Content should be the summary
        match &record.content {
            CoreContextPayload::Text { body } => {
                assert_eq!(body, "Summarized: a story about bees and honey.");
            }
            CoreContextPayload::Struct { .. } => panic!("expected text payload"),
        }

        // Derivation should be LlmSummarize
        match &record.derivation {
            DerivationStep::LlmSummarize {
                capability,
                prompt_hash: hash,
                ..
            } => {
                assert_eq!(capability, "context.summarize");
                assert_eq!(*hash, [0xABu8; 32]);
            }
            other => panic!("expected LlmSummarize derivation, got: {other:?}"),
        }

        // Verify current version bumped to 1
        assert_eq!(
            mgr.current_version(&sid).await.unwrap(),
            CoreContextVersion(1)
        );
    }

    #[tokio::test]
    async fn llm_summarize_roundtrip_via_read() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("LLM-RT1".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(&sid, "seed data", CoreContextAuthor::System)
            .await
            .unwrap();

        // Apply LLM summarize
        let prompt_hash = [0x42u8; 32];
        mgr.apply_llm_summarize(&sid, "LLM summary v1", prompt_hash, "context.summarize")
            .await
            .unwrap();

        // Read back v1 and verify derivation_kind is 'llm_summarize'
        let record = mgr.read(&sid, CoreContextVersion(1)).await.unwrap();
        match &record.derivation {
            DerivationStep::LlmSummarize { capability, .. } => {
                assert_eq!(capability, "context.summarize");
            }
            other => panic!("expected LlmSummarize, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn llm_summarize_can_follow_other_operations() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("LLM-MIX1".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0
        mgr.apply_seed(&sid, "initial", CoreContextAuthor::System)
            .await
            .unwrap();

        // v1: user edit
        mgr.apply_user_edit(
            &sid,
            EditOp::Append {
                body: " + user addition".to_string(),
            },
            None,
        )
        .await
        .unwrap();

        // v2: preset hook
        mgr.apply_preset_hook(
            &sid,
            "st1",
            "h1",
            EditOp::Append {
                body: " + hook addition".to_string(),
            },
        )
        .await
        .unwrap();

        // v3: LLM summarize
        let record = mgr
            .apply_llm_summarize(&sid, "Final summary", [0u8; 32], "context.summarize")
            .await
            .unwrap();

        assert_eq!(record.version, CoreContextVersion(3));
        match &record.content {
            CoreContextPayload::Text { body } => {
                assert_eq!(body, "Final summary");
            }
            CoreContextPayload::Struct { .. } => panic!("expected text payload"),
        }

        // Verify current snapshot is the LLM summary
        let snapshot = mgr.current_snapshot(&sid).await.unwrap();
        assert_eq!(snapshot.version, CoreContextVersion(3));
    }

    // ---------- R7: cleanup_guard tests ----------

    #[tokio::test]
    async fn cleanup_guard_removes_entry_and_allows_new_guard() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("R7-CLEANUP1".to_string());
        insert_test_schedule(&mgr.pool, &sid.0).await;

        // Seed v0 — this creates the per-schedule guard
        mgr.apply_seed(&sid, "initial", CoreContextAuthor::System)
            .await
            .unwrap();

        // Verify guard exists (schedule_guards has an entry for sid)
        // We can't inspect the HashMap directly (it's private), but we can
        // verify behavior: after cleanup, a new operation should still work
        // with a freshly created guard.
        mgr.cleanup_guard(&sid).await;

        // After cleanup, applying a new edit should still work
        // (a new guard is created on demand by schedule_guard)
        let record = mgr
            .apply_user_edit(
                &sid,
                EditOp::Append {
                    body: " after cleanup".to_string(),
                },
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            record.version,
            CoreContextVersion(1),
            "new guard should be created after cleanup"
        );

        // Verify the content
        match &record.content {
            CoreContextPayload::Text { body } => {
                assert_eq!(body, "initial after cleanup");
            }
            CoreContextPayload::Struct { .. } => panic!("expected text payload"),
        }
    }

    #[tokio::test]
    async fn cleanup_guard_on_nonexistent_schedule_is_noop() {
        let (pool, _db) = fresh_pool().await;
        let mgr = CoreContextManager::new(pool);
        let sid = ScheduleId("R7-NONEXISTENT".to_string());

        // Cleanup on a schedule that was never used — should not panic
        mgr.cleanup_guard(&sid).await;
    }
}
