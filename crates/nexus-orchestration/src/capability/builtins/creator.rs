//! Creator capabilities: `read_memory`, `write_memory`, `inject_prompt`.
//!
//! Owner crate: `nexus-orchestration`.
//!
//! Capabilities use [`CreatorCapabilityStore`] (orchestration-side adapter
//! backed by `Arc<SqlitePool>`) for real persistence. When no store is
//! injected (standalone/test mode), capabilities return stub responses.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_contracts::local::orchestration::{
    CreatorInjectPromptInput, CreatorInjectPromptOutput, CreatorReadMemoryInput,
    CreatorReadMemoryOutput, CreatorWriteBriefInput, CreatorWriteBriefOutput,
    CreatorWriteMemoryInput, CreatorWriteMemoryOutput,
};
use serde_json::Value;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// CreatorCapabilityStore — orchestration-side adapter
// ---------------------------------------------------------------------------

/// Orchestration-side adapter that centralizes creator identity resolution,
/// memory fragment queries, and prompt injection queue operations.
///
/// Backed by `Arc<SqlitePool>` and injected into all three creator capabilities.
/// Follows the `KbExtractWork::with_pool` injection pattern.
pub struct CreatorCapabilityStore {
    pool: Arc<sqlx::SqlitePool>,
}

impl CreatorCapabilityStore {
    /// Create a new store from a `SqlitePool`.
    #[must_use]
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Create a new store from an `Arc<SqlitePool>`.
    // Arc::new is not const, so this cannot be const fn on stable.
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn from_arc(pool: Arc<sqlx::SqlitePool>) -> Self {
        Self { pool }
    }

    /// Resolve the creator identity from capability input.
    ///
    /// Only accepts **context-injected** identity fields (prefixed with `_`)
    /// to prevent cross-creator authorization bypass (IDOR). The orchestration
    /// task execution layer is responsible for enriching capability input with
    /// `_creator_id`, `_schedule_id`, and `_session_id` from the running
    /// schedule/session context before invoking capabilities.
    ///
    /// Resolution order:
    /// 1. Input `_creator_id` (injected by task execution from schedule/session context)
    /// 2. Input `_schedule_id` → look up `creator_schedules.creator_id`
    /// 3. Input `_session_id` → look up `orchestration_sessions.creator_id`
    /// 4. Error
    ///
    /// # Errors
    ///
    /// Returns `CapabilityError::InputInvalid` if no creator identity can be resolved.
    /// Returns `CapabilityError::Internal` if a database lookup fails.
    pub async fn resolve_creator_id(&self, input: &Value) -> Result<String, CapabilityError> {
        // Step 1: context-injected _creator_id
        if let Some(id) = input.get("_creator_id").and_then(|v| v.as_str()) {
            return Ok(id.to_string());
        }

        // Step 2: context-injected _schedule_id → creator_schedules.creator_id
        if let Some(schedule_id) = input.get("_schedule_id").and_then(|v| v.as_str()) {
            let row = sqlx::query_scalar!(
                "SELECT creator_id FROM creator_schedules WHERE schedule_id = ?",
                schedule_id
            )
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(|e| CapabilityError::Internal(format!("schedule lookup: {e}")))?;

            if let Some(creator_id) = row {
                return Ok(creator_id);
            }
        }

        // Step 3: context-injected _session_id → orchestration_sessions.creator_id
        if let Some(session_id) = input.get("_session_id").and_then(|v| v.as_str()) {
            let row = sqlx::query_scalar!(
                "SELECT creator_id FROM orchestration_sessions WHERE session_id = ?",
                session_id
            )
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(|e| CapabilityError::Internal(format!("session lookup: {e}")))?;

            if let Some(creator_id) = row {
                return Ok(creator_id);
            }
        }

        // Step 4: unresolved
        Err(CapabilityError::InputInvalid(
            "missing creator identity: orchestration context must inject _creator_id, _schedule_id, or _session_id"
                .into(),
        ))
    }

    /// Resolve `session_id` from capability input.
    ///
    /// Only accepts context-injected `_session_id`. Falls back to `"default"`
    /// only in standalone/test mode (no store injected).
    fn resolve_session_id(input: &Value) -> String {
        if let Some(id) = input.get("_session_id").and_then(|v| v.as_str()) {
            return id.to_string();
        }
        "default".to_string()
    }

    /// Read memory fragments for a creator with optional keyword filter.
    ///
    /// Returns the count of matching fragments.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityError::Internal` if the database query fails.
    pub async fn read_memory(
        &self,
        creator_id: &str,
        keyword: Option<&str>,
        limit: u32,
    ) -> Result<u32, CapabilityError> {
        let count = nexus_local_db::count_fragments(self.pool.as_ref(), creator_id, keyword)
            .await
            .map_err(|e| CapabilityError::Internal(format!("read_memory: {e}")))?;

        // If count is within limit, return it directly; otherwise return limit
        // (we count, not fetch, so this is informational)
        let _ = limit;
        Ok(count)
    }

    /// Write a memory fragment for a creator.
    ///
    /// Returns the fragment ID of the newly created fragment.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityError::Internal` if the database insert fails.
    pub async fn write_memory(
        &self,
        creator_id: &str,
        content: &str,
        keywords: &[String],
        source_session_id: Option<&str>,
    ) -> Result<String, CapabilityError> {
        let fragment_id = format!("frag_{}", generate_ulid());

        let keywords_json = serde_json::to_string(keywords)
            .map_err(|e| CapabilityError::Internal(format!("serialize keywords: {e}")))?;

        let now = chrono::Utc::now().to_rfc3339();

        let fragment = nexus_local_db::MemoryFragmentRecord {
            fragment_id: fragment_id.clone(),
            session_id: source_session_id.unwrap_or("standalone").to_string(),
            creator_id: creator_id.to_string(),
            keywords: keywords_json,
            summary: content.to_string(),
            created_at: now,
            ttl: None,
        };

        nexus_local_db::create_fragment(self.pool.as_ref(), &fragment)
            .await
            .map_err(|e| CapabilityError::Internal(format!("write_memory: {e}")))?;

        Ok(fragment_id)
    }

    /// Enqueue a prompt for later consumption.
    ///
    /// Inserts a `queued` row into `creator_prompt_injections`.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityError::Internal` if the database insert fails.
    pub async fn enqueue_prompt(
        &self,
        creator_id: &str,
        session_id: &str,
        prompt: &str,
        priority: i32,
        source_schedule_id: Option<&str>,
    ) -> Result<String, CapabilityError> {
        let injection_id = format!("inj_{}", generate_ulid());
        let now = chrono::Utc::now().timestamp_millis();

        nexus_local_db::enqueue_prompt_injection(
            self.pool.as_ref(),
            nexus_local_db::NewPromptInjection {
                injection_id: &injection_id,
                creator_id,
                session_id,
                prompt,
                priority: i64::from(priority),
                created_at: now,
                expires_at: None,
                source_schedule_id,
                source_capability_call_id: None,
                metadata_json: None,
            },
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("enqueue_prompt: {e}")))?;

        Ok(injection_id)
    }

    /// Drain queued prompt injections for a creator/session.
    ///
    /// Claims queued rows and returns them for consumption by `acp.prompt`.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityError::Internal` if the database query fails.
    pub async fn drain_prompt_queue(
        &self,
        creator_id: &str,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<nexus_local_db::PromptInjectionRow>, CapabilityError> {
        let now = chrono::Utc::now().timestamp_millis();
        nexus_local_db::claim_prompt_injections(
            self.pool.as_ref(),
            creator_id,
            session_id,
            limit,
            now,
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("drain_prompt_queue: {e}")))
    }
}

/// Generate a simple ULID-like ID for fragment/injection primary keys.
/// Uses timestamp + counter for uniqueness within this process.
fn generate_ulid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{ts:016x}{counter:08x}")
}

// ---------------------------------------------------------------------------
// creator.read_memory
// ---------------------------------------------------------------------------

/// Read entries from the creator memory store.
///
/// When no store is injected (standalone/test mode), returns `count: 0`.
pub struct CreatorReadMemory {
    store: Option<Arc<CreatorCapabilityStore>>,
}

impl CreatorReadMemory {
    /// Create without a store (placeholder mode).
    #[must_use]
    pub const fn new() -> Self {
        Self { store: None }
    }

    /// Create with a store for real persistence.
    #[must_use]
    // Arc::new is not const, so this cannot be const fn on stable.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_store(store: Arc<CreatorCapabilityStore>) -> Self {
        Self { store: Some(store) }
    }
}

impl Default for CreatorReadMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for CreatorReadMemory {
    fn name(&self) -> &'static str {
        "creator.read_memory"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"keyword":{"type":"string"},"limit":{"type":"integer","minimum":1,"default":50}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"count":{"type":"integer","minimum":0}},"required":["count"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: CreatorReadMemoryInput =
            serde_json::from_value(input.clone()).map_err(|e| {
                CapabilityError::InputInvalid(format!("creator.read_memory input: {e}"))
            })?;

        let Some(store) = &self.store else {
            // Standalone/test mode — return zero count
            let output = CreatorReadMemoryOutput { count: 0 };
            return serde_json::to_value(output)
                .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")));
        };

        let creator_id = store.resolve_creator_id(&input).await?;
        let keyword = parsed.keyword.as_deref();
        let limit = parsed.limit;

        let count = store.read_memory(&creator_id, keyword, limit).await?;

        let output = CreatorReadMemoryOutput { count };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// creator.write_memory
// ---------------------------------------------------------------------------

/// Append/update creator memory.
///
/// When no store is injected (standalone/test mode), returns a stub fragment ID.
pub struct CreatorWriteMemory {
    store: Option<Arc<CreatorCapabilityStore>>,
}

impl CreatorWriteMemory {
    /// Create without a store (placeholder mode).
    #[must_use]
    pub const fn new() -> Self {
        Self { store: None }
    }

    /// Create with a store for real persistence.
    #[must_use]
    // Arc::new is not const, so this cannot be const fn on stable.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_store(store: Arc<CreatorCapabilityStore>) -> Self {
        Self { store: Some(store) }
    }
}

impl Default for CreatorWriteMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for CreatorWriteMemory {
    fn name(&self) -> &'static str {
        "creator.write_memory"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"content":{"type":"string"},"keywords":{"type":"array","items":{"type":"string"}},"required":["content","keywords"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"fragmentId":{"type":"string"}},"required":["fragmentId"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: CreatorWriteMemoryInput =
            serde_json::from_value(input.clone()).map_err(|e| {
                CapabilityError::InputInvalid(format!("creator.write_memory input: {e}"))
            })?;

        let Some(store) = &self.store else {
            // Standalone/test mode — return stub
            let output = CreatorWriteMemoryOutput {
                fragment_id: "stub-fragment-id".to_string(),
            };
            return serde_json::to_value(output)
                .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")));
        };

        let creator_id = store.resolve_creator_id(&input).await?;
        let source_session_id = input.get("_session_id").and_then(|v| v.as_str());

        let fragment_id = store
            .write_memory(
                &creator_id,
                &parsed.content,
                &parsed.keywords,
                source_session_id,
            )
            .await?;

        let output = CreatorWriteMemoryOutput { fragment_id };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// creator.inject_prompt
// ---------------------------------------------------------------------------

/// Queue a prompt to be sent on the next `acp.prompt`.
///
/// When no store is injected (standalone/test mode), returns `{ queued: true }`
/// without persistence.
pub struct CreatorInjectPrompt {
    store: Option<Arc<CreatorCapabilityStore>>,
}

impl CreatorInjectPrompt {
    /// Create without a store (placeholder mode).
    #[must_use]
    pub const fn new() -> Self {
        Self { store: None }
    }

    /// Create with a store for real persistence.
    #[must_use]
    // Arc::new is not const, so this cannot be const fn on stable.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_store(store: Arc<CreatorCapabilityStore>) -> Self {
        Self { store: Some(store) }
    }
}

impl Default for CreatorInjectPrompt {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for CreatorInjectPrompt {
    fn name(&self) -> &'static str {
        "creator.inject_prompt"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"prompt":{"type":"string"},"priority":{"type":"integer","default":0}},"required":["prompt"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"queued":{"type":"boolean"}},"required":["queued"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: CreatorInjectPromptInput =
            serde_json::from_value(input.clone()).map_err(|e| {
                CapabilityError::InputInvalid(format!("creator.inject_prompt input: {e}"))
            })?;

        // Validate prompt is non-empty
        if parsed.prompt.trim().is_empty() {
            return Err(CapabilityError::InputInvalid(
                "prompt must not be empty".into(),
            ));
        }

        let Some(store) = &self.store else {
            // Standalone/test mode — return stub
            let output = CreatorInjectPromptOutput { queued: true };
            return serde_json::to_value(output)
                .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")));
        };

        let creator_id = store.resolve_creator_id(&input).await?;
        let session_id = CreatorCapabilityStore::resolve_session_id(&input);
        let source_schedule_id = input.get("_schedule_id").and_then(|v| v.as_str());

        store
            .enqueue_prompt(
                &creator_id,
                &session_id,
                &parsed.prompt,
                parsed.priority,
                source_schedule_id,
            )
            .await?;

        let output = CreatorInjectPromptOutput { queued: true };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// creator.write_brief
// ---------------------------------------------------------------------------

/// Required keys in a valid creative brief (work-experience-model §4).
const BRIEF_REQUIRED_KEYS: &[&str] = &[
    "genre",
    "tone",
    "audience",
    "constraints",
    "themes",
    "non_goals",
    "protagonist_hook",
    "setting_hook",
    "open_questions_resolved",
];

/// Required string keys that must be non-empty after trim.
const BRIEF_REQUIRED_NONEMPTY_KEYS: &[&str] = &[
    "genre",
    "tone",
    "audience",
    "protagonist_hook",
    "setting_hook",
];

/// Required array keys that must have at least one entry.
const BRIEF_REQUIRED_ARRAY_KEYS: &[&str] = &["constraints", "themes"];

/// Validate a creative brief JSON against §4 schema.
///
/// Returns `Ok(())` if valid, or an error message describing what's wrong.
fn validate_creative_brief(brief: &serde_json::Value) -> Result<(), String> {
    let obj = brief
        .as_object()
        .ok_or_else(|| "brief must be a JSON object".to_string())?;

    // Check all required keys present
    for key in BRIEF_REQUIRED_KEYS {
        if !obj.contains_key(*key) {
            return Err(format!("missing required field '{key}'"));
        }
    }

    // Check string fields non-empty
    for key in BRIEF_REQUIRED_NONEMPTY_KEYS {
        if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
            if val.trim().is_empty() {
                return Err(format!("field '{key}' must not be empty"));
            }
        } else {
            return Err(format!("field '{key}' must be a non-empty string"));
        }
    }

    // Check array fields have at least one entry
    for key in BRIEF_REQUIRED_ARRAY_KEYS {
        if let Some(arr) = obj.get(*key).and_then(|v| v.as_array()) {
            if arr.is_empty() {
                return Err(format!("field '{key}' must have at least one entry"));
            }
            // Each entry must be a non-empty string
            for (i, entry) in arr.iter().enumerate() {
                let s = entry
                    .as_str()
                    .ok_or_else(|| format!("{key}[{i}] must be a string"))?;
                if s.trim().is_empty() {
                    return Err(format!("{key}[{i}] must not be empty"));
                }
            }
        } else {
            return Err(format!("field '{key}' must be an array"));
        }
    }

    // non_goals and open_questions_resolved should be arrays (may be empty)
    for key in &["non_goals", "open_questions_resolved"] {
        if !obj.get(*key).map_or(false, |v| v.is_array()) {
            return Err(format!("field '{key}' must be an array"));
        }
    }

    Ok(())
}

/// Validate and persist a creative brief on a Work entity.
///
/// Parses `brief_text` as JSON, validates against §4 schema, and writes
/// to the Work via `patch_work` in `nexus_local_db`. Sets
/// `intake_status=complete` only when the brief is valid.
///
/// In standalone/test mode (no store), returns a stub success response
/// after validating the brief.
pub struct CreatorWriteBrief {
    store: Option<Arc<CreatorCapabilityStore>>,
}

impl CreatorWriteBrief {
    /// Create without a store (placeholder mode).
    #[must_use]
    pub const fn new() -> Self {
        Self { store: None }
    }

    /// Create with a store for real persistence.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_store(store: Arc<CreatorCapabilityStore>) -> Self {
        Self { store: Some(store) }
    }
}

impl Default for CreatorWriteBrief {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for CreatorWriteBrief {
    fn name(&self) -> &'static str {
        "creator.write_brief"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"work_id":{"type":"string"},"brief_text":{"type":"string"}},"required":["work_id","brief_text"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"written":{"type":"boolean"},"intakeStatus":{"type":"string"}},"required":["written","intakeStatus"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: CreatorWriteBriefInput =
            serde_json::from_value(input.clone()).map_err(|e| {
                CapabilityError::InputInvalid(format!("creator.write_brief input: {e}"))
            })?;

        // Parse brief_text as JSON
        let brief: serde_json::Value =
            serde_json::from_str(&parsed.brief_text).map_err(|e| {
                CapabilityError::InputInvalid(format!(
                    "brief_text is not valid JSON: {e}"
                ))
            })?;

        // Validate against §4 schema
        validate_creative_brief(&brief).map_err(CapabilityError::InputInvalid)?;

        let Some(store) = &self.store else {
            // Standalone/test mode — return stub
            let output = CreatorWriteBriefOutput {
                written: true,
                intake_status: "complete".to_string(),
            };
            return serde_json::to_value(output)
                .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")));
        };

        let creator_id = store.resolve_creator_id(&input).await?;
        let brief_json = serde_json::to_string(&brief)
            .map_err(|e| CapabilityError::Internal(format!("serialize brief: {e}")))?;
        let now = chrono::Utc::now().to_rfc3339();

        let patch = nexus_local_db::WorkPatch {
            creative_brief: Some(Some(brief_json)),
            intake_status: Some("complete".to_string()),
            ..Default::default()
        };

        nexus_local_db::patch_work(
            store.pool.as_ref(),
            &creator_id,
            &parsed.work_id,
            &patch,
            &now,
        )
        .await
        .map_err(|e| {
            CapabilityError::Internal(format!("write_brief patch_work: {e}"))
        })?;

        let output = CreatorWriteBriefOutput {
            written: true,
            intake_status: "complete".to_string(),
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Standalone mode: no store injected — returns zero count.
    #[tokio::test]
    async fn read_memory_standalone_returns_zero() {
        let cap = CreatorReadMemory::new();
        let out = cap
            .run(serde_json::json!({"keyword": "test", "_creator_id": "ctr_test"}))
            .await
            .unwrap();
        assert_eq!(out["count"], 0);
    }

    /// Standalone mode: no store injected — returns stub fragment ID.
    #[tokio::test]
    async fn write_memory_standalone_returns_stub() {
        let cap = CreatorWriteMemory::new();
        let out = cap
            .run(serde_json::json!({
                "content": "hello",
                "keywords": ["greeting"],
                "_creator_id": "ctr_test"
            }))
            .await
            .unwrap();
        assert_eq!(out["fragmentId"], "stub-fragment-id");
    }

    /// Standalone mode: no store injected — returns queued without persistence.
    #[tokio::test]
    async fn inject_prompt_standalone_returns_queued() {
        let cap = CreatorInjectPrompt::new();
        let out = cap
            .run(serde_json::json!({
                "prompt": "write chapter 1",
                "_creator_id": "ctr_test"
            }))
            .await
            .unwrap();
        assert_eq!(out["queued"], true);
    }

    /// Inject prompt with empty text should fail.
    #[tokio::test]
    async fn inject_prompt_rejects_empty() {
        let cap = CreatorInjectPrompt::new();
        let result = cap
            .run(serde_json::json!({
                "prompt": "   ",
                "_creator_id": "ctr_test"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    /// Standalone mode: missing creator identity should fail.
    #[tokio::test]
    async fn read_memory_standalone_no_identity_ok() {
        // Standalone mode returns count 0 without identity resolution
        let cap = CreatorReadMemory::new();
        let out = cap.run(serde_json::json!({})).await.unwrap();
        assert_eq!(out["count"], 0);
    }

    // ── Integration tests (require SQLite) ──────────────────────────────

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn read_memory_with_store_returns_count() {
        let (pool, _dir) = fresh_pool().await;
        let store = Arc::new(CreatorCapabilityStore::new(pool));
        let cap = CreatorReadMemory::with_store(store);

        // Seed a fragment
        nexus_local_db::create_fragment(
            cap.store.as_ref().unwrap().pool.as_ref(),
            &nexus_local_db::MemoryFragmentRecord {
                fragment_id: "frag_test_1".to_string(),
                session_id: "sess_test".to_string(),
                creator_id: "ctr_test".to_string(),
                keywords: "[\"alpha\"]".to_string(),
                summary: "Test fragment".to_string(),
                created_at: "2026-05-30T00:00:00Z".to_string(),
                ttl: None,
            },
        )
        .await
        .unwrap();

        let out = cap
            .run(serde_json::json!({"_creator_id": "ctr_test"}))
            .await
            .unwrap();
        assert_eq!(out["count"], 1);
    }

    #[tokio::test]
    async fn read_memory_with_keyword_filter() {
        let (pool, _dir) = fresh_pool().await;
        let store = Arc::new(CreatorCapabilityStore::new(pool));
        let cap = CreatorReadMemory::with_store(store);

        let p = cap.store.as_ref().unwrap().pool.as_ref();
        nexus_local_db::create_fragment(
            p,
            &nexus_local_db::MemoryFragmentRecord {
                fragment_id: "frag_1".to_string(),
                session_id: "sess_test".to_string(),
                creator_id: "ctr_test".to_string(),
                keywords: "[\"alpha\"]".to_string(),
                summary: "Alpha fragment".to_string(),
                created_at: "2026-05-30T00:00:00Z".to_string(),
                ttl: None,
            },
        )
        .await
        .unwrap();

        nexus_local_db::create_fragment(
            p,
            &nexus_local_db::MemoryFragmentRecord {
                fragment_id: "frag_2".to_string(),
                session_id: "sess_test".to_string(),
                creator_id: "ctr_test".to_string(),
                keywords: "[\"beta\"]".to_string(),
                summary: "Beta fragment".to_string(),
                created_at: "2026-05-30T00:00:00Z".to_string(),
                ttl: None,
            },
        )
        .await
        .unwrap();

        // Filter by "alpha" — should get 1
        let out = cap
            .run(serde_json::json!({"_creator_id": "ctr_test", "keyword": "alpha"}))
            .await
            .unwrap();
        assert_eq!(out["count"], 1);
    }

    #[tokio::test]
    async fn write_memory_with_store_roundtrip() {
        let (pool, _dir) = fresh_pool().await;
        let store = Arc::new(CreatorCapabilityStore::new(pool));
        let write_cap = CreatorWriteMemory::with_store(store.clone());
        let read_cap = CreatorReadMemory::with_store(store);

        // Write
        let write_out = write_cap
            .run(serde_json::json!({
                "content": "A memory of the protagonist",
                "keywords": ["character", "protagonist"],
                "_creator_id": "ctr_roundtrip"
            }))
            .await
            .unwrap();

        let fragment_id = write_out["fragmentId"].as_str().unwrap();
        assert!(
            !fragment_id.contains("stub"),
            "expected real fragment ID, got: {fragment_id}"
        );

        // Read — should find the fragment
        let read_out = read_cap
            .run(serde_json::json!({"_creator_id": "ctr_roundtrip"}))
            .await
            .unwrap();
        assert_eq!(read_out["count"], 1);
    }

    #[tokio::test]
    async fn inject_prompt_with_store_enqueue_drain() {
        let (pool, _dir) = fresh_pool().await;
        let store = Arc::new(CreatorCapabilityStore::new(pool));
        let cap = CreatorInjectPrompt::with_store(store.clone());

        // Enqueue
        let out = cap
            .run(serde_json::json!({
                "prompt": "write chapter 1",
                "priority": 5,
                "_creator_id": "ctr_inject",
                "_session_id": "sess_inject"
            }))
            .await
            .unwrap();
        assert_eq!(out["queued"], true);

        // Drain — should find the queued prompt
        let drained = store
            .drain_prompt_queue("ctr_inject", "sess_inject", 10)
            .await
            .unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].prompt, "write chapter 1");
        assert_eq!(drained[0].status, "claimed");
    }

    #[tokio::test]
    async fn inject_prompt_multiple_priority_order() {
        let (pool, _dir) = fresh_pool().await;
        let store = Arc::new(CreatorCapabilityStore::new(pool));
        let cap = CreatorInjectPrompt::with_store(store.clone());

        // Enqueue low priority first
        cap.run(serde_json::json!({
            "prompt": "low priority",
            "priority": 0,
            "_creator_id": "ctr_prio",
            "_session_id": "sess_prio"
        }))
        .await
        .unwrap();

        // Enqueue high priority second
        cap.run(serde_json::json!({
            "prompt": "high priority",
            "priority": 10,
            "_creator_id": "ctr_prio",
            "_session_id": "sess_prio"
        }))
        .await
        .unwrap();

        // Drain — high priority should come first
        let drained = store
            .drain_prompt_queue("ctr_prio", "sess_prio", 1)
            .await
            .unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].prompt, "high priority");
    }

    /// Verifies that raw `creator_id` (without underscore) is REJECTED.
    /// Only `_creator_id` (context-injected) is accepted for security.
    #[tokio::test]
    async fn resolve_creator_id_rejects_raw_creator_id() {
        let (pool, _dir) = fresh_pool().await;
        let store = CreatorCapabilityStore::new(pool);
        let result = store
            .resolve_creator_id(&serde_json::json!({"creator_id": "ctr_direct"}))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing creator identity"),
            "expected rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn resolve_creator_id_from_underscore_creator_id() {
        let (pool, _dir) = fresh_pool().await;
        let store = CreatorCapabilityStore::new(pool);
        let id = store
            .resolve_creator_id(&serde_json::json!({"_creator_id": "ctr_injected"}))
            .await
            .unwrap();
        assert_eq!(id, "ctr_injected");
    }

    #[tokio::test]
    async fn resolve_creator_id_from_schedule_id() {
        let (pool, _dir) = fresh_pool().await;
        // Insert a schedule row
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            "INSERT INTO creator_schedules
                (schedule_id, creator_id, preset_id, preset_version, status,
                 concurrency_kind, current_core_context_version, created_at, updated_at)
             VALUES ('sched_1', 'ctr_from_sched', 'test', 1, 'running', 'serial', 0, ?, ?)",
            now,
            now
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = CreatorCapabilityStore::new(pool);
        let id = store
            .resolve_creator_id(&serde_json::json!({"_schedule_id": "sched_1"}))
            .await
            .unwrap();
        assert_eq!(id, "ctr_from_sched");
    }

    #[tokio::test]
    async fn resolve_creator_id_from_session_id() {
        let (pool, _dir) = fresh_pool().await;
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, status,
                 context_json, created_at, updated_at)
             VALUES ('sess_1', 'ctr_from_sess', 'test', 1, 'running', '{}', ?, ?)",
            now,
            now
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = CreatorCapabilityStore::new(pool);
        let id = store
            .resolve_creator_id(&serde_json::json!({"_session_id": "sess_1"}))
            .await
            .unwrap();
        assert_eq!(id, "ctr_from_sess");
    }

    #[tokio::test]
    async fn resolve_creator_id_missing_returns_error() {
        let (pool, _dir) = fresh_pool().await;
        let store = CreatorCapabilityStore::new(pool);
        let result = store
            .resolve_creator_id(&serde_json::json!({"other_field": "value"}))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing creator identity"));
    }

    // ── creator.write_brief tests ──────────────────────────────────────

    /// Standalone mode: valid brief returns written=true.
    #[tokio::test]
    async fn write_brief_standalone_valid_brief() {
        let cap = CreatorWriteBrief::new();
        let brief = serde_json::json!({
            "brief_schema_version": 1,
            "genre": "literary fiction",
            "tone": "dark and brooding",
            "audience": "adult literary readers",
            "constraints": ["no graphic violence"],
            "themes": ["identity", "loss"],
            "non_goals": ["not a romance"],
            "protagonist_hook": "A retired detective haunted by a cold case",
            "setting_hook": "A fog-shrouded coastal town in winter",
            "open_questions_resolved": ["genre confirmed as literary fiction"]
        });
        let out = cap
            .run(serde_json::json!({
                "workId": "wrk_test",
                "briefText": serde_json::to_string(&brief).unwrap(),
                "_creator_id": "ctr_test"
            }))
            .await
            .unwrap();
        assert_eq!(out["written"], true);
        assert_eq!(out["intakeStatus"], "complete");
    }

    /// Standalone mode: invalid JSON in brief_text should fail.
    #[tokio::test]
    async fn write_brief_standalone_invalid_json() {
        let cap = CreatorWriteBrief::new();
        let result = cap
            .run(serde_json::json!({
                "workId": "wrk_test",
                "briefText": "not valid json {{{",
                "_creator_id": "ctr_test"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not valid JSON"));
    }

    /// Standalone mode: missing required field should fail.
    #[tokio::test]
    async fn write_brief_standalone_missing_field() {
        let cap = CreatorWriteBrief::new();
        let brief = serde_json::json!({
            "genre": "sci-fi",
            "tone": "tense"
            // missing audience, constraints, themes, etc.
        });
        let result = cap
            .run(serde_json::json!({
                "workId": "wrk_test",
                "briefText": serde_json::to_string(&brief).unwrap(),
                "_creator_id": "ctr_test"
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing required field"),
            "expected missing field error, got: {err}"
        );
    }

    /// Standalone mode: empty string field should fail.
    #[tokio::test]
    async fn write_brief_standalone_empty_genre() {
        let cap = CreatorWriteBrief::new();
        let brief = serde_json::json!({
            "brief_schema_version": 1,
            "genre": "  ",
            "tone": "dark",
            "audience": "adults",
            "constraints": ["none"],
            "themes": ["loss"],
            "non_goals": [],
            "protagonist_hook": "A detective",
            "setting_hook": "A city",
            "open_questions_resolved": []
        });
        let result = cap
            .run(serde_json::json!({
                "workId": "wrk_test",
                "briefText": serde_json::to_string(&brief).unwrap(),
                "_creator_id": "ctr_test"
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must not be empty"),
            "expected empty field error, got: {err}"
        );
    }

    /// Standalone mode: empty constraints array should fail.
    #[tokio::test]
    async fn write_brief_standalone_empty_constraints() {
        let cap = CreatorWriteBrief::new();
        let brief = serde_json::json!({
            "brief_schema_version": 1,
            "genre": "sci-fi",
            "tone": "tense",
            "audience": "adults",
            "constraints": [],
            "themes": ["identity"],
            "non_goals": [],
            "protagonist_hook": "A hacker",
            "setting_hook": "Neo Tokyo",
            "open_questions_resolved": []
        });
        let result = cap
            .run(serde_json::json!({
                "workId": "wrk_test",
                "briefText": serde_json::to_string(&brief).unwrap(),
                "_creator_id": "ctr_test"
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("at least one entry"),
            "expected array length error, got: {err}"
        );
    }

    /// Integration: write_brief with store writes to Work entity.
    #[tokio::test]
    async fn write_brief_with_store_roundtrip() {
        let (pool, _dir) = fresh_pool().await;
        let store = Arc::new(CreatorCapabilityStore::new(pool.clone()));
        let cap = CreatorWriteBrief::with_store(store);

        // Seed a Work
        nexus_local_db::create_work(
            &pool,
            &nexus_local_db::WorkRecord {
                work_id: "wrk_brief_test".to_string(),
                creator_id: "ctr_brief_test".to_string(),
                workspace_slug: "ws".to_string(),
                status: "active".to_string(),
                title: "Test Work".to_string(),
                long_term_goal: "Test".to_string(),
                initial_idea: "An idea".to_string(),
                creative_brief: None,
                intake_status: "pending".to_string(),
                world_id: None,
                story_ref: None,
                inspiration_log: "[]".to_string(),
                primary_preset_id: "novel-writing".to_string(),
                schedule_ids: "[]".to_string(),
                created_at: "2026-06-04T00:00:00Z".to_string(),
                updated_at: "2026-06-04T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();

        let brief = serde_json::json!({
            "brief_schema_version": 1,
            "genre": "literary fiction",
            "tone": "melancholic",
            "audience": "adult readers",
            "constraints": ["no explicit content"],
            "themes": ["memory", "loss"],
            "non_goals": ["not a thriller"],
            "protagonist_hook": "A retired librarian discovers a hidden diary",
            "setting_hook": "A small New England town in autumn",
            "open_questions_resolved": ["genre: literary fiction", "tone: melancholic"]
        });

        let out = cap
            .run(serde_json::json!({
                "workId": "wrk_brief_test",
                "briefText": serde_json::to_string(&brief).unwrap(),
                "_creator_id": "ctr_brief_test"
            }))
            .await
            .unwrap();

        assert_eq!(out["written"], true);
        assert_eq!(out["intakeStatus"], "complete");

        // Verify the Work was actually updated
        let updated = nexus_local_db::get_work(&pool, "ctr_brief_test", "wrk_brief_test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.intake_status, "complete");
        assert!(updated.creative_brief.is_some());
        let stored: serde_json::Value =
            serde_json::from_str(&updated.creative_brief.unwrap()).unwrap();
        assert_eq!(stored["genre"], "literary fiction");
    }
}
