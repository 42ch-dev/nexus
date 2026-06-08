//! Finding CRUD operations for the quality-loop (novel-quality-loop §2.1, V1.39 P1).
//!
//! Manages the `findings` table — quality issues surfaced by the
//! review/reflection-loop stage during auto-chain. Each finding is
//! scoped to a Work (and optionally a chapter), carries a severity
//! and status lifecycle, and provides a routing hint (`target_executor`)
//! indicating which preset should address it.

use sqlx::{Row, SqlitePool};

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

/// Create a new finding.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn create_finding(
    pool: &SqlitePool,
    f: &Finding,
) -> Result<(), LocalDbError> {
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

/// Count open findings for a Work, grouped by severity.
///
/// Returns a list of (severity, count) pairs for all open findings.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_open_findings_by_severity(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<Vec<SeverityCount>, LocalDbError> {
    // SAFETY: COUNT(*) return type cannot be inferred by sqlx compile-time
    // macro for SQLite; use runtime query with manual row mapping.
    let rows = sqlx::query(
        "SELECT severity, COUNT(*) as count FROM findings
         WHERE creator_id = ? AND work_id = ? AND status = 'open'
         GROUP BY severity",
    )
    .bind(creator_id)
    .bind(work_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SeverityCount {
            severity: r.get("severity"),
            count: r.get("count"),
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
