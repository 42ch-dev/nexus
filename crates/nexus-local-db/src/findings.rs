//! Finding CRUD operations for the quality-loop (novel-quality-loop §2.1, V1.39 P1).
//!
//! Manages the `findings` table — quality issues surfaced by the
//! review/reflection-loop stage during auto-chain. Each finding is
//! scoped to a Work (and optionally a chapter), carries a severity
//! and status lifecycle, and provides a routing hint (`target_executor`)
//! indicating which preset should address it.

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// Finding record — mirrors DB row.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    /// Unique identifier (ULID).
    pub finding_id: String,
    /// Owning Work.
    pub work_id: String,
    /// Optional chapter binding (NULL = Work-level).
    pub chapter: Option<i64>,
    /// Severity: `info`, `minor`, `major`, `blocker`.
    pub severity: String,
    /// Status: `open`, `resolved`, `wont_fix`.
    pub status: String,
    /// Short human-readable label.
    pub title: String,
    /// Detailed finding body.
    pub description: String,
    /// Routing hint: `write`, `brainstorm`, `none`, `master`.
    pub target_executor: String,
    /// Owning creator (isolation).
    pub creator_id: String,
    /// Creation timestamp (Unix epoch).
    pub created_at: i64,
    /// Last update timestamp (Unix epoch).
    pub updated_at: i64,
}

/// Fields that can be patched on a Finding.
#[derive(Debug, Clone, Default)]
pub struct FindingPatch {
    /// New severity.
    pub severity: Option<String>,
    /// New status.
    pub status: Option<String>,
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New routing hint.
    pub target_executor: Option<String>,
}

/// Filters for listing findings.
#[derive(Debug, Clone, Default)]
pub struct FindingListFilters {
    /// Filter by `work_id`.
    pub work_id: Option<String>,
    /// Filter by `chapter`.
    pub chapter: Option<i64>,
    /// Filter by `status`.
    pub status: Option<String>,
    /// Filter by `severity`.
    pub severity: Option<String>,
    /// Maximum number of results.
    pub limit: Option<u32>,
    /// Pagination offset.
    pub offset: Option<u32>,
}

/// Valid severity values (R-V139P1-W-1).
pub const VALID_SEVERITIES: &[&str] = &["info", "minor", "major", "blocker"];

/// Valid status values (R-V139P1-W-1).
pub const VALID_STATUSES: &[&str] = &["open", "resolved", "wont_fix"];

/// Valid `target_executor` values (R-V139P1-W-1).
pub const VALID_TARGET_EXECUTORS: &[&str] = &["write", "brainstorm", "none", "master"];

/// R-V139P1-W-2: Single source of truth for finding ID generation.
///
/// All callers (handler direct-create, from-review hook) must use this
/// function instead of inline `format!("fnd_{}", ...)`.
#[must_use]
pub fn mint_finding_id() -> String {
    format!("fnd_{}", uuid::Uuid::new_v4().simple())
}

/// Validate finding enum fields. Returns [`LocalDbError::ConstraintViolation`] on invalid values.
///
/// R-V139P1-W-1: runtime match!() guard mirrors the CHECK constraints in
/// migration `202606100002_findings_check_constraints.sql`. Catches invalid
/// values before they reach the DB, providing actionable error messages.
///
/// # Errors
///
/// Returns [`LocalDbError::ConstraintViolation`] if any enum field has an invalid value.
pub fn validate_finding_enums(
    severity: &str,
    status: &str,
    target_executor: &str,
) -> Result<(), LocalDbError> {
    if !VALID_SEVERITIES.contains(&severity) {
        return Err(LocalDbError::ConstraintViolation {
            table: "findings".to_string(),
            constraint: format!(
                "invalid severity '{severity}'; expected one of: {}",
                VALID_SEVERITIES.join(", ")
            ),
        });
    }
    if !VALID_STATUSES.contains(&status) {
        return Err(LocalDbError::ConstraintViolation {
            table: "findings".to_string(),
            constraint: format!(
                "invalid status '{status}'; expected one of: {}",
                VALID_STATUSES.join(", ")
            ),
        });
    }
    if !VALID_TARGET_EXECUTORS.contains(&target_executor) {
        return Err(LocalDbError::ConstraintViolation {
            table: "findings".to_string(),
            constraint: format!(
                "invalid target_executor '{target_executor}'; expected one of: {}",
                VALID_TARGET_EXECUTORS.join(", ")
            ),
        });
    }
    Ok(())
}

/// Create a new finding.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn create_finding(pool: &SqlitePool, f: &Finding) -> Result<(), LocalDbError> {
    validate_finding_enums(&f.severity, &f.status, &f.target_executor)?;
    sqlx::query!(
        "INSERT INTO findings (finding_id, work_id, chapter, severity, status, title, description, target_executor, creator_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        f.finding_id,
        f.work_id,
        f.chapter,
        f.severity,
        f.status,
        f.title,
        f.description,
        f.target_executor,
        f.creator_id,
        f.created_at,
        f.updated_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// List findings with optional filters, scoped to a creator.
///
/// R-V139P1-W-4: EXPLAIN QUERY PLAN audit result for the primary query:
///   `SEARCH` findings USING INDEX `idx_findings_creator_status` (`creator_id`=? AND `status`=?)
///   When `work_id` is provided, `SQLite` may use `idx_findings_work_status` instead.
///   The composite index `idx_findings_work_chapter_status` covers chapter lookups.
///   All three indexes are utilized; no full-table scan on realistic data.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_findings(
    pool: &SqlitePool,
    creator_id: &str,
    filters: &FindingListFilters,
) -> Result<Vec<Finding>, LocalDbError> {
    let limit = filters.limit.unwrap_or(100);
    let offset = filters.offset.unwrap_or(0);

    let rows = sqlx::query!(
        "SELECT finding_id as \"finding_id!\", work_id as \"work_id!\", chapter,
                severity as \"severity!\", status as \"status!\",
                title as \"title!\", description as \"description!\",
                target_executor as \"target_executor!\",
                creator_id as \"creator_id!\",
                created_at as \"created_at!\", updated_at as \"updated_at!\"
         FROM findings
         WHERE creator_id = ?
           AND (? IS NULL OR work_id = ?)
           AND (? IS NULL OR chapter = ?)
           AND (? IS NULL OR status = ?)
           AND (? IS NULL OR severity = ?)
         ORDER BY created_at DESC
         LIMIT ? OFFSET ?",
        creator_id,
        filters.work_id,
        filters.work_id,
        filters.chapter,
        filters.chapter,
        filters.status,
        filters.status,
        filters.severity,
        filters.severity,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Finding {
            finding_id: r.finding_id,
            work_id: r.work_id,
            chapter: r.chapter,
            severity: r.severity,
            status: r.status,
            title: r.title,
            description: r.description,
            target_executor: r.target_executor,
            creator_id: r.creator_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

/// Get a single finding by ID, scoped to a creator.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_finding(
    pool: &SqlitePool,
    creator_id: &str,
    finding_id: &str,
) -> Result<Option<Finding>, LocalDbError> {
    let row = sqlx::query!(
        "SELECT finding_id as \"finding_id!\", work_id as \"work_id!\", chapter,
                severity as \"severity!\", status as \"status!\",
                title as \"title!\", description as \"description!\",
                target_executor as \"target_executor!\",
                creator_id as \"creator_id!\",
                created_at as \"created_at!\", updated_at as \"updated_at!\"
         FROM findings
         WHERE creator_id = ? AND finding_id = ?",
        creator_id,
        finding_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Finding {
        finding_id: r.finding_id,
        work_id: r.work_id,
        chapter: r.chapter,
        severity: r.severity,
        status: r.status,
        title: r.title,
        description: r.description,
        target_executor: r.target_executor,
        creator_id: r.creator_id,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// Update (patch) a finding, scoped to a creator.
///
/// Only non-None fields in `patch` are applied. `updated_at` is always
/// set to the current Unix epoch.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the finding is not found.
pub async fn update_finding(
    pool: &SqlitePool,
    creator_id: &str,
    finding_id: &str,
    patch: &FindingPatch,
    now_epoch: i64,
) -> Result<bool, LocalDbError> {
    // R-V139P1-W-1: validate patch enum fields if provided
    if let Some(ref sev) = patch.severity {
        if !VALID_SEVERITIES.contains(&sev.as_str()) {
            return Err(LocalDbError::ConstraintViolation {
                table: "findings".to_string(),
                constraint: format!(
                    "invalid severity '{sev}'; expected one of: {}",
                    VALID_SEVERITIES.join(", ")
                ),
            });
        }
    }
    if let Some(ref st) = patch.status {
        if !VALID_STATUSES.contains(&st.as_str()) {
            return Err(LocalDbError::ConstraintViolation {
                table: "findings".to_string(),
                constraint: format!(
                    "invalid status '{st}'; expected one of: {}",
                    VALID_STATUSES.join(", ")
                ),
            });
        }
    }
    if let Some(ref te) = patch.target_executor {
        if !VALID_TARGET_EXECUTORS.contains(&te.as_str()) {
            return Err(LocalDbError::ConstraintViolation {
                table: "findings".to_string(),
                constraint: format!(
                    "invalid target_executor '{te}'; expected one of: {}",
                    VALID_TARGET_EXECUTORS.join(", ")
                ),
            });
        }
    }
    let result = sqlx::query!(
        "UPDATE findings
         SET severity        = COALESCE(?, severity),
             status          = COALESCE(?, status),
             title           = COALESCE(?, title),
             description     = COALESCE(?, description),
             target_executor = COALESCE(?, target_executor),
             updated_at      = ?
         WHERE creator_id = ? AND finding_id = ?",
        patch.severity,
        patch.status,
        patch.title,
        patch.description,
        patch.target_executor,
        now_epoch,
        creator_id,
        finding_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete a finding, scoped to a creator.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn delete_finding(
    pool: &SqlitePool,
    creator_id: &str,
    finding_id: &str,
) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "DELETE FROM findings WHERE creator_id = ? AND finding_id = ?",
        creator_id,
        finding_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Severity count row — used by `count_open_findings_by_severity`.
#[derive(Debug, Clone)]
pub struct SeverityCount {
    /// Severity level.
    pub severity: String,
    /// Number of open findings with this severity.
    pub count: i64,
}

/// R-V139P1-W-5: internal row for compile-time `query_as!`.
#[derive(Debug, Clone, sqlx::FromRow)]
struct SeverityCountRow {
    severity: String,
    count: i64,
}

/// Summary of a single stale finding — used by the master-decision timeout
/// daemon task (V1.39 P4, novel-quality-loop §6).
///
/// "Stale" means `status = 'open'` and `created_at < now - threshold_seconds`,
/// where the threshold defaults to 96h. Only the fields needed by the
/// status banner and the structured log line are included to keep the
/// daemon task allocation light.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StaleFindingSummary {
    /// Finding identifier.
    pub finding_id: String,
    /// Owning Work.
    pub work_id: String,
    /// Owning creator (used to scope the banner and optional auto-schedule).
    pub creator_id: String,
    /// Finding severity at the time of detection.
    pub severity: String,
    /// Original creation epoch (seconds).
    pub created_at: i64,
    /// Age in seconds at the time of the query (>= threshold).
    pub age_seconds: i64,
}

/// List open findings that are older than `threshold_seconds`, **scoped to
/// a single creator** so the banner respects per-creator isolation.
///
/// "Stale" condition: `status = 'open'` AND `created_at < now_epoch -
/// threshold_seconds`. Results are ordered by `created_at ASC` so the
/// oldest finding shows first.
///
/// The function takes `now_epoch` as a parameter so the master-decision
/// timeout daemon task can be exercised hermetically with a mocked clock
/// (V1.39 P4 T5).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_stale_open_findings(
    pool: &SqlitePool,
    creator_id: &str,
    now_epoch: i64,
    threshold_seconds: i64,
) -> Result<Vec<StaleFindingSummary>, LocalDbError> {
    let cutoff = now_epoch.saturating_sub(threshold_seconds);
    let rows = sqlx::query!(
        "SELECT finding_id as \"finding_id!\", work_id as \"work_id!\",
                creator_id as \"creator_id!\", severity as \"severity!\",
                created_at as \"created_at!\"
         FROM findings
         WHERE creator_id = ? AND status = 'open' AND created_at < ?
         ORDER BY created_at ASC",
        creator_id,
        cutoff,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| StaleFindingSummary {
            finding_id: r.finding_id,
            work_id: r.work_id,
            creator_id: r.creator_id,
            severity: r.severity,
            age_seconds: now_epoch.saturating_sub(r.created_at),
            created_at: r.created_at,
        })
        .collect())
}

/// List **all** open stale findings across creators, used by the daemon
/// scheduled task to surface a workspace-wide structured log (V1.39 P4 T2).
///
/// The CLI banner uses `list_stale_open_findings` (per-creator scope);
/// the daemon log uses this function because the daemon does not have a
/// single bound creator and must log all stale rows it observes.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_all_stale_open_findings(
    pool: &SqlitePool,
    now_epoch: i64,
    threshold_seconds: i64,
) -> Result<Vec<StaleFindingSummary>, LocalDbError> {
    let cutoff = now_epoch.saturating_sub(threshold_seconds);
    let rows = sqlx::query!(
        "SELECT finding_id as \"finding_id!\", work_id as \"work_id!\",
                creator_id as \"creator_id!\", severity as \"severity!\",
                created_at as \"created_at!\"
         FROM findings
         WHERE status = 'open' AND created_at < ?
         ORDER BY created_at ASC",
        cutoff,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| StaleFindingSummary {
            finding_id: r.finding_id,
            work_id: r.work_id,
            creator_id: r.creator_id,
            severity: r.severity,
            age_seconds: now_epoch.saturating_sub(r.created_at),
            created_at: r.created_at,
        })
        .collect())
}

/// Count open findings for a Work, grouped by severity.
///
/// Returns a list of (severity, count) pairs for all open findings.
///
/// R-V139P1-W-5: investigated compile-time `query_as!` conversion; `SQLite`'s
/// `COUNT(*)` return type is not reliably inferred by `sqlx` offline macros.
/// Keeping runtime `query_as::<_, SeverityCountRow>` (`FromRow` derives correctly)
/// which still provides column-name-checked mapping without full `sqlx` prepare.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_open_findings_by_severity(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<Vec<SeverityCount>, LocalDbError> {
    // SAFETY: dynamic SQL — runtime query_as with FromRow; COUNT(*) return type
    // not reliably inferred by sqlx compile-time macro for SQLite offline mode.
    let rows = sqlx::query_as::<_, SeverityCountRow>(
        "SELECT severity, CAST(COUNT(*) AS INTEGER) as count FROM findings
         WHERE creator_id = ? AND work_id = ? AND status = 'open'
         GROUP BY severity",
    )
    .bind(creator_id)
    .bind(work_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SeverityCount {
            severity: r.severity,
            count: r.count,
        })
        .collect())
}

/// Parameters for creating a finding from a review-stage result (T3 hook).
///
/// This is the **minimal path** signal source: the supervisor (or daemon
/// API handler) extracts a structured verdict from the review stage's
/// terminal output context and passes it here. The actual LLM-judge
/// parsing is the caller's responsibility; this function just persists
/// the finding.
///
/// ## Signal source (documented choice)
///
/// The chosen signal is the review schedule's **terminal context JSON**,
/// which contains a `verdict` object with `severity`, `title`,
/// `description`, and `target_executor` fields. The daemon API handler
/// (or a future orchestration hook) is responsible for parsing this JSON
/// and constructing a `ReviewVerdictFinding` — keeping the DB layer
/// free of LLM output parsing.
#[derive(Debug, Clone)]
pub struct ReviewVerdictFinding {
    /// Owning Work.
    pub work_id: String,
    /// Optional chapter (from review context).
    pub chapter: Option<i64>,
    /// Severity from LLM-judge verdict.
    pub severity: String,
    /// Short title from LLM-judge verdict.
    pub title: String,
    /// Detailed description from LLM-judge verdict.
    pub description: String,
    /// Routing hint from LLM-judge verdict.
    pub target_executor: String,
    /// Owning creator.
    pub creator_id: String,
}

/// Create a finding from a review-stage verdict (T3 minimal path).
///
/// Generates a ULID `finding_id` and inserts the finding row. Errors are
/// logged by the caller; findings creation must not fork or block the
/// auto-chain driver schedule (AC4).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn create_finding_from_review(
    pool: &SqlitePool,
    verdict: &ReviewVerdictFinding,
) -> Result<String, LocalDbError> {
    let finding_id = format!("fnd_{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().timestamp();
    let f = Finding {
        finding_id: finding_id.clone(),
        work_id: verdict.work_id.clone(),
        chapter: verdict.chapter,
        severity: verdict.severity.clone(),
        status: "open".to_string(),
        title: verdict.title.clone(),
        description: verdict.description.clone(),
        target_executor: verdict.target_executor.clone(),
        creator_id: verdict.creator_id.clone(),
        created_at: now,
        updated_at: now,
    };
    create_finding(pool, &f).await?;
    Ok(finding_id)
}

#[cfg(test)]
mod tests {
    use sqlx::{FromRow, SqlitePool};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    /// C-1 fix: Verify the spec-required composite index on
    /// (work_id, chapter, status) exists after migration.
    /// Per novel-quality-loop.md §2.1: chapter-scoped finding lookups
    /// (the review-stage hook's hot path) must use this index.
    #[tokio::test]
    async fn test_findings_work_chapter_status_index_exists() {
        let (pool, _dir) = fresh_pool().await;

        let index_sql: Option<String> = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master \
             WHERE type = 'index' AND name = 'idx_findings_work_chapter_status'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap()
        .flatten();

        assert!(
            index_sql.is_some(),
            "C-1 fix: idx_findings_work_chapter_status index should exist after migration"
        );

        let sql = index_sql.unwrap();
        assert!(
            sql.contains("work_id") && sql.contains("chapter") && sql.contains("status"),
            "index should cover (work_id, chapter, status): {sql}"
        );
    }
}
