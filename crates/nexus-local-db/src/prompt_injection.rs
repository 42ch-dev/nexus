//! Prompt injection queue CRUD operations for `creator.inject_prompt`.
//!
//! Manages per-creator/per-session prompt queue in `SQLite`.
//! Lifecycle: `queued` → `claimed` → `consumed` | `expired`.
//!
//! Design: architect note C0 — `creator.inject_prompt` Queue Design.

use sqlx::{Row, SqlitePool};

use crate::error::LocalDbError;

/// Prompt injection row — mirrors DB row.
#[derive(Debug, Clone)]
pub struct PromptInjectionRow {
    /// Unique injection ID.
    pub injection_id: String,
    /// Creator ID for ownership scoping.
    pub creator_id: String,
    /// Session ID for session-scoped consumption.
    pub session_id: String,
    /// The prompt text to inject.
    pub prompt: String,
    /// Higher priority = dequeued sooner.
    pub priority: i64,
    /// Lifecycle status: `queued`, `claimed`, `consumed`, `expired`.
    pub status: String,
    /// Unix epoch milliseconds when enqueued.
    pub created_at: i64,
    /// Unix epoch milliseconds when claimed.
    pub claimed_at: Option<i64>,
    /// Unix epoch milliseconds when consumed.
    pub consumed_at: Option<i64>,
    /// Unix epoch milliseconds when the injection expires.
    pub expires_at: Option<i64>,
    /// Source schedule that enqueued this injection.
    pub source_schedule_id: Option<String>,
    /// Source capability call that enqueued this injection.
    pub source_capability_call_id: Option<String>,
    /// Optional metadata as JSON blob.
    pub metadata_json: Option<Vec<u8>>,
}

/// Parameters for creating a new prompt injection.
pub struct NewPromptInjection<'a> {
    /// Unique injection ID (caller-generated).
    pub injection_id: &'a str,
    /// Creator ID for ownership scoping.
    pub creator_id: &'a str,
    /// Session ID for session-scoped consumption.
    pub session_id: &'a str,
    /// The prompt text to inject.
    pub prompt: &'a str,
    /// Higher priority = dequeued sooner.
    pub priority: i64,
    /// Unix epoch milliseconds when enqueued.
    pub created_at: i64,
    /// Optional expiry timestamp.
    pub expires_at: Option<i64>,
    /// Source schedule that enqueued this injection.
    pub source_schedule_id: Option<&'a str>,
    /// Source capability call that enqueued this injection.
    pub source_capability_call_id: Option<&'a str>,
    /// Optional metadata as JSON blob.
    pub metadata_json: Option<&'a [u8]>,
}

/// Enqueue a new prompt injection.
///
/// Inserts a `queued` row into `creator_prompt_injections`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn enqueue_prompt_injection(
    pool: &SqlitePool,
    new: NewPromptInjection<'_>,
) -> Result<PromptInjectionRow, LocalDbError> {
    sqlx::query!(
        "INSERT INTO creator_prompt_injections
            (injection_id, creator_id, session_id, prompt, priority,
             status, created_at, expires_at, source_schedule_id,
             source_capability_call_id, metadata_json)
         VALUES (?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?, ?)",
        new.injection_id,
        new.creator_id,
        new.session_id,
        new.prompt,
        new.priority,
        new.created_at,
        new.expires_at,
        new.source_schedule_id,
        new.source_capability_call_id,
        new.metadata_json
    )
    .execute(pool)
    .await?;

    Ok(PromptInjectionRow {
        injection_id: new.injection_id.to_string(),
        creator_id: new.creator_id.to_string(),
        session_id: new.session_id.to_string(),
        prompt: new.prompt.to_string(),
        priority: new.priority,
        status: "queued".to_string(),
        created_at: new.created_at,
        claimed_at: None,
        consumed_at: None,
        expires_at: new.expires_at,
        source_schedule_id: new.source_schedule_id.map(std::string::ToString::to_string),
        source_capability_call_id: new
            .source_capability_call_id
            .map(std::string::ToString::to_string),
        metadata_json: new.metadata_json.map(std::vec::Vec::from),
    })
}

/// Claim the next queued prompt injections for a creator/session.
///
/// Selects queued rows ordered by `priority DESC, created_at ASC`,
/// then updates them to `claimed` status. Returns the claimed rows.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn claim_prompt_injections(
    pool: &SqlitePool,
    creator_id: &str,
    session_id: &str,
    limit: i64,
    now: i64,
) -> Result<Vec<PromptInjectionRow>, LocalDbError> {
    // SAFETY: `creator_id`/`session_id` are application-generated strings.
    // Dynamic SQL is required to pass `LIMIT` as a parameter because
    // sqlx compile-time checking does not support parameterized LIMIT.
    let rows = sqlx::query(
        "SELECT injection_id, creator_id, session_id, prompt, priority,
                status, created_at, claimed_at, consumed_at, expires_at,
                source_schedule_id, source_capability_call_id, metadata_json
         FROM creator_prompt_injections
         WHERE creator_id = ? AND session_id = ? AND status = 'queued'
         ORDER BY priority DESC, created_at ASC
         LIMIT ?",
    )
    .bind(creator_id)
    .bind(session_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let ids: Vec<String> = rows
        .iter()
        .map(|r| r.get::<String, _>("injection_id"))
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Update status to claimed
    let placeholders: Vec<String> = (2..=ids.len() + 1).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "UPDATE creator_prompt_injections
         SET status = 'claimed', claimed_at = ?1
         WHERE injection_id IN ({})",
        placeholders.join(", ")
    );
    let mut query = sqlx::query(&sql).bind(now);
    for id in &ids {
        query = query.bind(id);
    }
    query.execute(pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| PromptInjectionRow {
            injection_id: row.get("injection_id"),
            creator_id: row.get("creator_id"),
            session_id: row.get("session_id"),
            prompt: row.get("prompt"),
            priority: row.get("priority"),
            status: "claimed".to_string(),
            created_at: row.get("created_at"),
            claimed_at: Some(now),
            consumed_at: row.get("consumed_at"),
            expires_at: row.get("expires_at"),
            source_schedule_id: row.get("source_schedule_id"),
            source_capability_call_id: row.get("source_capability_call_id"),
            metadata_json: row.get("metadata_json"),
        })
        .collect())
}

/// Mark claimed prompt injections as consumed.
///
/// Sets `status = 'consumed'` and `consumed_at = now` for the given injection IDs.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn mark_prompt_injections_consumed(
    pool: &SqlitePool,
    injection_ids: &[String],
    now: i64,
) -> Result<u64, LocalDbError> {
    if injection_ids.is_empty() {
        return Ok(0);
    }

    // SAFETY: `injection_ids` are application-generated ULIDs, not user input.
    // Dynamic SQL is required because sqlx compile-time checking cannot handle
    // variable-length IN clauses. Parameterized placeholders prevent injection.
    let placeholders: Vec<String> = (1..=injection_ids.len())
        .map(|i| format!("?{}", i + 1))
        .collect();
    let sql = format!(
        "UPDATE creator_prompt_injections
         SET status = 'consumed', consumed_at = ?
         WHERE injection_id IN ({}) AND status = 'claimed'",
        placeholders.join(", ")
    );

    let mut query = sqlx::query(&sql).bind(now);
    for id in injection_ids {
        query = query.bind(id);
    }
    let result = query.execute(pool).await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    #[tokio::test]
    async fn test_enqueue_and_claim() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();

        let row = enqueue_prompt_injection(
            &pool,
            NewPromptInjection {
                injection_id: "inj_001",
                creator_id: "ctr_test",
                session_id: "sess_001",
                prompt: "write chapter 1",
                priority: 0,
                created_at: now,
                expires_at: None,
                source_schedule_id: None,
                source_capability_call_id: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(row.status, "queued");
        assert_eq!(row.injection_id, "inj_001");

        let claimed = claim_prompt_injections(&pool, "ctr_test", "sess_001", 10, now)
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].status, "claimed");
        assert_eq!(claimed[0].prompt, "write chapter 1");
    }

    #[tokio::test]
    async fn test_claim_respects_priority() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();

        enqueue_prompt_injection(
            &pool,
            NewPromptInjection {
                injection_id: "inj_low",
                creator_id: "ctr_test",
                session_id: "sess_001",
                prompt: "low priority",
                priority: 0,
                created_at: now,
                expires_at: None,
                source_schedule_id: None,
                source_capability_call_id: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();

        enqueue_prompt_injection(
            &pool,
            NewPromptInjection {
                injection_id: "inj_high",
                creator_id: "ctr_test",
                session_id: "sess_001",
                prompt: "high priority",
                priority: 10,
                created_at: now,
                expires_at: None,
                source_schedule_id: None,
                source_capability_call_id: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();

        let claimed = claim_prompt_injections(&pool, "ctr_test", "sess_001", 1, now)
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].injection_id, "inj_high");
    }

    #[tokio::test]
    async fn test_mark_consumed() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();

        enqueue_prompt_injection(
            &pool,
            NewPromptInjection {
                injection_id: "inj_001",
                creator_id: "ctr_test",
                session_id: "sess_001",
                prompt: "test",
                priority: 0,
                created_at: now,
                expires_at: None,
                source_schedule_id: None,
                source_capability_call_id: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();

        let claimed = claim_prompt_injections(&pool, "ctr_test", "sess_001", 10, now)
            .await
            .unwrap();

        let consumed =
            mark_prompt_injections_consumed(&pool, &[claimed[0].injection_id.clone()], now)
                .await
                .unwrap();
        assert_eq!(consumed, 1);
    }

    #[tokio::test]
    async fn test_mark_consumed_empty_list() {
        let (pool, _dir) = fresh_pool().await;
        let result = mark_prompt_injections_consumed(&pool, &[], now_ms())
            .await
            .unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_claim_nothing_for_wrong_session() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();

        enqueue_prompt_injection(
            &pool,
            NewPromptInjection {
                injection_id: "inj_001",
                creator_id: "ctr_test",
                session_id: "sess_001",
                prompt: "test",
                priority: 0,
                created_at: now,
                expires_at: None,
                source_schedule_id: None,
                source_capability_call_id: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();

        let claimed = claim_prompt_injections(&pool, "ctr_test", "sess_other", 10, now)
            .await
            .unwrap();
        assert!(claimed.is_empty());
    }
}
