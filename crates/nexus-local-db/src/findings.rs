//! Finding CRUD operations for the quality-loop (novel-quality-loop §2.1, V1.39 P1).
//!
//! Manages the `findings` table — quality issues surfaced by the
//! review stage (`novel-chapter-review` preset, V1.47) during auto-chain
//! or on-demand `creator run`. Each finding is scoped to a Work (and
//! optionally a chapter), carries a `kind`, severity, status lifecycle,
//! optional `rule_suggestion`, and a routing hint (`target_executor`)
//! indicating which preset should address it.

use sqlx::{Sqlite, SqlitePool, Transaction};

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
    /// V1.47: finding category (`continuity`, `craft`, `plot_hole`,
    /// `world_inconsistency`, …). NOT NULL with default `'craft'`.
    pub kind: String,
    /// V1.47: optional prose suggestion for Layer 2 rules; persisted on the
    /// finding row only (no `AGENTS.md` write in P0).
    pub rule_suggestion: Option<String>,
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
    /// V1.47: new `kind` (optional).
    pub kind: Option<String>,
    /// V1.47: new `rule_suggestion` (optional; pass `Some(None)` via separate
    /// sentinel is not supported — `None` means "do not patch").
    pub rule_suggestion: Option<String>,
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

/// V1.47 §2.1 / §8.2: suggested minimum `kind` vocabulary for findings.
///
/// Open vocabulary — callers MAY use other kind strings; this list is the
/// suggested minimum enumerated in the spec. Validation does **not** reject
/// unknown kinds (the column is `TEXT`, not `CHECK`-constrained).
pub const SUGGESTED_FINDING_KINDS: &[&str] =
    &["continuity", "craft", "plot_hole", "world_inconsistency"];

/// V1.47 P0 fix (qc2 W-2): closed set of valid `kind` values for the
/// review-hook path (`create_finding_from_review`).
///
/// The general CRUD path (`create_finding`) remains open-vocabulary; only
/// `create_finding_from_review` enforces this set so the synthesized
/// finding's category is always one of these well-known values. Unknown kinds
/// are rejected with [`LocalDbError::ConstraintViolation`].
///
/// V1.48 P0 T4: expanded to include `plot_hole` and `world_inconsistency`
/// per `.mstar/knowledge/specs/novel-quality-loop.md` §2.1 (the V1.47 P0
/// quick-closure missed these spec-listed kinds; the producer's
/// `review-report.md` parser emits them and the DB layer must accept).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    /// Craft-level writing issue (prose, dialogue, imagery).
    Craft,
    /// Continuity / consistency across chapters or scenes.
    Continuity,
    /// Pacing or structural rhythm issue.
    Pacing,
    /// Internal consistency within a chapter or passage.
    Consistency,
    /// Catch-all for review findings that don't fit the above.
    Other,
    /// Plot-level issue (introduced-then-dropped thread, contradiction).
    /// V1.48 P0: added per `novel-quality-loop.md` §2.1.
    PlotHole,
    /// World-building inconsistency (timeline, geography, lore).
    /// V1.48 P0: added per `novel-quality-loop.md` §2.1.
    WorldInconsistency,
}

impl FindingKind {
    /// All valid string representations, in canonical order.
    pub const ALL_STRS: &'static [&'static str] = &[
        "craft",
        "continuity",
        "pacing",
        "consistency",
        "other",
        "plot_hole",
        "world_inconsistency",
    ];

    /// Returns the canonical string for this variant.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Craft => "craft",
            Self::Continuity => "continuity",
            Self::Pacing => "pacing",
            Self::Consistency => "consistency",
            Self::Other => "other",
            Self::PlotHole => "plot_hole",
            Self::WorldInconsistency => "world_inconsistency",
        }
    }

    /// Validate a `kind` string against the closed set.
    ///
    /// Returns the validated string on success, or
    /// [`LocalDbError::ConstraintViolation`] if the value is not in
    /// [`ALL_STRS`](Self::ALL_STRS).
    ///
    /// # Errors
    ///
    /// Returns [`LocalDbError::ConstraintViolation`] for unknown kind values.
    pub fn validate(s: &str) -> Result<String, LocalDbError> {
        if Self::ALL_STRS.contains(&s) {
            Ok(s.to_string())
        } else {
            Err(LocalDbError::ConstraintViolation {
                table: "findings".to_string(),
                constraint: format!(
                    "invalid kind '{s}'; expected one of: {}",
                    Self::ALL_STRS.join(", ")
                ),
            })
        }
    }
}

/// R-V139P1-W-2: Single source of truth for finding ID generation.
///
/// All callers (handler direct-create, from-review hook) must use this
/// function instead of inline `format!("fnd_{}", ...)`.
#[must_use]
pub fn mint_finding_id() -> String {
    format!("fnd_{}", uuid::Uuid::new_v4().simple())
}

/// V1.47 P0 fix (qc2 W-2): validate and normalize `rule_suggestion` text.
///
/// - `None` → `Ok(None)` (no patch).
/// - `Some(s)` → trim leading/trailing whitespace, then:
///   - **reject** with [`LocalDbError::ConstraintViolation`] if the trimmed
///     string is empty (caller passed `Some("")` / whitespace-only);
///   - **reject** if the trimmed byte length exceeds
///     [`RULE_SUGGESTION_MAX_BYTES`] (4 KiB);
///   - otherwise return `Ok(Some(trimmed))`.
///
/// No internal-whitespace collapsing is performed: the value is persisted
/// verbatim (after trim) so users can author multi-line or multi-sentence
/// rule suggestions without surprise reformatting.
///
/// This guard prevents accidentally persisting unbounded whitespace or
/// oversized blobs on the finding row. The P0 synthesized finding always
/// passes `None`, but the public DAO surface exists for future callers.
///
/// # Errors
///
/// Returns [`LocalDbError::ConstraintViolation`] when the caller-provided
/// value is empty after trimming or exceeds the byte cap.
pub fn normalize_rule_suggestion(s: Option<&str>) -> Result<Option<String>, LocalDbError> {
    let Some(s) = s else {
        return Ok(None);
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(LocalDbError::ConstraintViolation {
            table: "findings".to_string(),
            constraint: "rule_suggestion must be non-empty after trim".to_string(),
        });
    }
    let byte_len = trimmed.len();
    if byte_len > RULE_SUGGESTION_MAX_BYTES {
        return Err(LocalDbError::ConstraintViolation {
            table: "findings".to_string(),
            constraint: format!(
                "rule_suggestion is {byte_len} bytes; exceeds {RULE_SUGGESTION_MAX_BYTES}-byte cap"
            ),
        });
    }
    Ok(Some(trimmed.to_string()))
}

/// V1.47 P0 fix (qc2 W-2): maximum accepted byte length for `rule_suggestion` text (4 KiB).
///
/// Longer inputs are **rejected** (not truncated) by [`normalize_rule_suggestion`]
/// to surface upstream bugs rather than silently dropping content.
pub const RULE_SUGGESTION_MAX_BYTES: usize = 4096;

/// Validate finding enum fields. Returns [`LocalDbError::ConstraintViolation`] on invalid values.
///
/// R-V139P1-W-1: Runtime validation is the **sole enforcement mechanism** for
/// finding enum fields. `SQLite` does not support `ALTER TABLE ADD CONSTRAINT`
/// for `CHECK` constraints on existing tables; adding `CHECK` to the original
/// `CREATE TABLE` is not retroactively applicable. This function is called on
/// both create and patch paths, providing the only guard against invalid enum
/// values. Any non-`Rust` caller (future API, direct SQL) must be validated
/// before reaching the DB.
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
        "INSERT INTO findings (finding_id, work_id, chapter, severity, status, title, description, target_executor, creator_id, kind, rule_suggestion, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        f.finding_id,
        f.work_id,
        f.chapter,
        f.severity,
        f.status,
        f.title,
        f.description,
        f.target_executor,
        f.creator_id,
        f.kind,
        f.rule_suggestion,
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
                kind as \"kind!\", rule_suggestion,
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
            kind: r.kind,
            rule_suggestion: r.rule_suggestion,
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
                kind as \"kind!\", rule_suggestion,
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
        kind: r.kind,
        rule_suggestion: r.rule_suggestion,
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
             kind            = COALESCE(?, kind),
             rule_suggestion = COALESCE(?, rule_suggestion),
             updated_at      = ?
         WHERE creator_id = ? AND finding_id = ?",
        patch.severity,
        patch.status,
        patch.title,
        patch.description,
        patch.target_executor,
        patch.kind,
        patch.rule_suggestion,
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
    /// V1.47 §2.1 / §8.2: finding category. Callers SHOULD pick from
    /// [`SUGGESTED_FINDING_KINDS`] (or the closed set
    /// [`FindingKind::ALL_STRS`]). Defaults to `"craft"` when empty.
    ///
    /// V1.47 P0 fix (qc2 W-2): the DAO ([`create_finding_from_review`])
    /// validates non-empty `kind` against [`FindingKind::ALL_STRS`] and
    /// rejects unknown values with `ConstraintViolation` before any DB
    /// call. The wire shape of `ReviewVerdictFinding` is unchanged (it
    /// remains `String`); the handler can normalize wire values into the
    /// closed set if it needs to surface a different error shape.
    pub kind: String,
    /// V1.47 §8.2: optional prose suggestion for Layer 2 rules.
    /// Persisted on the finding row only; V1.47 P0 does not write `AGENTS.md`.
    pub rule_suggestion: Option<String>,
    /// V1.47 P0 fix (qc1 W-2): the originating `creator_schedules.schedule_id`
    /// when the finding was synthesized by the review terminal hook.
    /// When `Some`, the INSERT is idempotent — a second call with the same
    /// `(work_id, chapter, source_schedule_id)` triple is a no-op that returns
    /// the existing finding id (partial unique index
    /// `findings_unique_review_per_chapter`). `None` for the manual CRUD path
    /// (no idempotency guard).
    pub source_schedule_id: Option<String>,
}

/// Create a finding from a review-stage verdict (T3 minimal path).
///
/// Generates a ULID `finding_id` and inserts the finding row. Errors are
/// logged by the caller; findings creation must not fork or block the
/// auto-chain driver schedule (AC4).
///
/// **Idempotency** (V1.47 P0 fix — qc1 W-2 / qc2 W-1 / qc3 W-2):
/// When `verdict.source_schedule_id` is `Some`, the INSERT uses
/// `ON CONFLICT DO NOTHING` against the partial unique index
/// `findings_unique_review_per_chapter`. If the conflict target is hit
/// (a finding already exists for this `(work_id, chapter, source_schedule_id)`
/// triple), the existing finding id is returned — the call is a no-op. This
/// guarantees that calling the review terminal hook twice on the same chapter
/// does not create duplicate findings (novel-quality-loop.md §8.3 decision).
///
/// When `verdict.source_schedule_id` is `None` (manual CRUD / API path), the
/// standard insert is used with no idempotency guard.
///
/// V1.48 P0-fix1 (qc3 W-2): this pool-based entrypoint opens its own
/// single-statement transaction and delegates to [`create_finding_from_review_tx`].
/// Batched callers (e.g. the parsed-report path in `nexus-orchestration`) should
/// use [`create_finding_from_review_tx`] directly so N rows share one transaction
/// instead of N round-trips.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the finding enum
/// fields are invalid.
pub async fn create_finding_from_review(
    pool: &SqlitePool,
    verdict: &ReviewVerdictFinding,
) -> Result<String, LocalDbError> {
    let mut tx = pool.begin().await?;
    let finding_id = create_finding_from_review_tx(&mut tx, verdict).await?;
    tx.commit().await?;
    Ok(finding_id)
}

/// Transaction-scoped variant of [`create_finding_from_review`].
///
/// V1.48 P0-fix1 (qc3 W-2): introduced so the parsed-report persistence loop
/// in `nexus-orchestration::auto_chain::persist_parsed_findings` can insert
/// N findings inside a single `SQLite` transaction (one `BEGIN`/`COMMIT` pair)
/// instead of N sequential round-trips.
///
/// The semantics are identical to [`create_finding_from_review`]; only the
/// executor differs (`&mut Transaction` vs `&SqlitePool`). Callers own the
/// transaction boundary and must `commit()` after all rows are inserted.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the finding enum
/// fields are invalid.
pub async fn create_finding_from_review_tx(
    tx: &mut Transaction<'_, Sqlite>,
    verdict: &ReviewVerdictFinding,
) -> Result<String, LocalDbError> {
    validate_finding_enums(&verdict.severity, "open", &verdict.target_executor)?;

    // V1.47 §8.2: `kind` defaults to `"craft"` when empty (defensive — callers
    // should set it explicitly, but the spec lists `craft` as a safe default).
    // V1.47 P0 fix (qc2 W-2): once non-empty, `kind` is validated against the
    // closed set [`FindingKind::ALL_STRS`]; unknown values are rejected with
    // `ConstraintViolation` before any DB call.
    let kind = if verdict.kind.is_empty() {
        "craft".to_string()
    } else {
        FindingKind::validate(&verdict.kind)?
    };
    let now = chrono::Utc::now().timestamp();

    // V1.47 P0 fix (qc2 W-2): reject empty-after-trim and oversized
    // `rule_suggestion` payloads before any DB call.
    let rule_suggestion = normalize_rule_suggestion(verdict.rule_suggestion.as_deref())?;

    let finding_id = mint_finding_id();
    if let Some(source_schedule_id) = &verdict.source_schedule_id {
        // Idempotent path: review terminal hook with a source schedule.
        // SAFETY: dynamic SQL — ON CONFLICT clause with a partial-index
        // conflict target is not supported by sqlx compile-time macros.
        let result = sqlx::query(
            "INSERT INTO findings
               (finding_id, work_id, chapter, severity, status, title,
                description, target_executor, creator_id, kind,
                rule_suggestion, source_schedule_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'open', ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (work_id, chapter, source_schedule_id)
               WHERE source_schedule_id IS NOT NULL
               DO NOTHING",
        )
        .bind(&finding_id)
        .bind(&verdict.work_id)
        .bind(verdict.chapter)
        .bind(&verdict.severity)
        .bind(&verdict.title)
        .bind(&verdict.description)
        .bind(&verdict.target_executor)
        .bind(&verdict.creator_id)
        .bind(&kind)
        .bind(&rule_suggestion)
        .bind(source_schedule_id)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() == 1 {
            return Ok(finding_id);
        }

        // Conflict — the finding already exists. Fetch its id.
        // SAFETY: dynamic SQL — fetch by the unique triple.
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT finding_id FROM findings
                 WHERE work_id = ? AND chapter IS ? AND source_schedule_id = ?",
        )
        .bind(&verdict.work_id)
        .bind(verdict.chapter)
        .bind(source_schedule_id)
        .fetch_optional(&mut **tx)
        .await?;

        existing.ok_or_else(|| LocalDbError::ConstraintViolation {
            table: "findings".to_string(),
            constraint: "idempotent insert reported 0 rows but existing finding not found"
                .to_string(),
        })
    } else {
        // Standard path: manual CRUD / API (no idempotency guard).
        // Mirrors `create_finding`'s INSERT, inlined here so the `_tx`
        // variant does not need to reach back through the pool-based
        // `create_finding` (which would open its own nested transaction).
        // SAFETY: runtime query — mirrors the compile-time-checked INSERT in
        // `create_finding`; the `.sqlx` cache is not warmed for the `_tx`
        // variant's executor shape (see nexus-local-db AGENTS.md waiver
        // R-V140P0-S3). The column list + bind order match `create_finding`.
        sqlx::query(
            "INSERT INTO findings
               (finding_id, work_id, chapter, severity, status, title,
                description, target_executor, creator_id, kind,
                rule_suggestion, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&finding_id)
        .bind(&verdict.work_id)
        .bind(verdict.chapter)
        .bind(&verdict.severity)
        .bind("open")
        .bind(&verdict.title)
        .bind(&verdict.description)
        .bind(&verdict.target_executor)
        .bind(&verdict.creator_id)
        .bind(&kind)
        .bind(&rule_suggestion)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
        Ok(finding_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_rule_suggestion, FindingKind, RULE_SUGGESTION_MAX_BYTES};
    use crate::error::LocalDbError;
    use sqlx::SqlitePool;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    // ── V1.47 P0 (qc2 W-2): closed `kind` set + `rule_suggestion` cap ───────

    /// All enum variants in [`FindingKind::ALL_STRS`] validate successfully
    /// and return the input string unchanged.
    #[test]
    fn finding_kind_validate_accepts_known_values() {
        for &known in FindingKind::ALL_STRS {
            let validated = FindingKind::validate(known)
                .unwrap_or_else(|e| panic!("known kind '{known}' should validate: {e}"));
            assert_eq!(validated, known);
        }
        // sanity: the enum variant count matches the const ALL_STRS slice
        // length. V1.48 P0 T4: expanded from 5 → 7 to include `plot_hole`
        // and `world_inconsistency` per `novel-quality-loop.md` §2.1.
        assert_eq!(
            FindingKind::ALL_STRS.len(),
            7,
            "expected 7 closed-set kinds after V1.48 P0 expansion; got {}",
            FindingKind::ALL_STRS.len()
        );
    }

    /// Unknown `kind` strings are rejected with `ConstraintViolation`.
    #[test]
    fn finding_kind_validate_rejects_unknown() {
        let err = FindingKind::validate("foo").expect_err("unknown kind should be rejected");
        match err {
            LocalDbError::ConstraintViolation { table, constraint } => {
                assert_eq!(table, "findings");
                assert!(
                    constraint.contains('\''),
                    "constraint should quote the bad value: {constraint}"
                );
                assert!(
                    constraint.contains("foo"),
                    "constraint should name the bad value: {constraint}"
                );
                assert!(
                    constraint.contains("craft") && constraint.contains("continuity"),
                    "constraint should list the accepted set: {constraint}"
                );
            }
            other => panic!("expected ConstraintViolation, got {other:?}"),
        }
    }

    /// `rule_suggestion` at exactly the byte cap (4 KiB) is accepted.
    #[test]
    fn rule_suggestion_length_cap_accepts_within_limit() {
        let within = "a".repeat(RULE_SUGGESTION_MAX_BYTES);
        assert_eq!(within.len(), RULE_SUGGESTION_MAX_BYTES);
        let normalized = normalize_rule_suggestion(Some(&within))
            .expect("input exactly at the cap should be accepted");
        assert_eq!(normalized.as_deref(), Some(within.as_str()));
    }

    /// `rule_suggestion` one byte over the cap (4 KiB + 1) is rejected.
    #[test]
    fn rule_suggestion_length_cap_rejects_too_long() {
        let too_long = "a".repeat(RULE_SUGGESTION_MAX_BYTES + 1);
        assert_eq!(too_long.len(), RULE_SUGGESTION_MAX_BYTES + 1);
        let err = normalize_rule_suggestion(Some(&too_long))
            .expect_err("input over the cap should be rejected");
        match err {
            LocalDbError::ConstraintViolation { table, constraint } => {
                assert_eq!(table, "findings");
                assert!(
                    constraint.contains(&format!("{}", too_long.len())),
                    "constraint should report the observed byte length: {constraint}"
                );
                assert!(
                    constraint.contains(&format!("{RULE_SUGGESTION_MAX_BYTES}")),
                    "constraint should name the cap: {constraint}"
                );
            }
            other => panic!("expected ConstraintViolation, got {other:?}"),
        }
    }

    /// `rule_suggestion = Some("   ")` (whitespace-only) is rejected — callers
    /// that intend "no suggestion" must pass `None` explicitly.
    #[test]
    fn rule_suggestion_trimmed_empty_rejected() {
        let err = normalize_rule_suggestion(Some("   "))
            .expect_err("whitespace-only input should be rejected");
        assert!(
            matches!(err, LocalDbError::ConstraintViolation { ref table, .. } if table == "findings"),
            "expected ConstraintViolation on findings, got {err:?}"
        );
        // None stays None — no rejection, no normalization.
        assert!(
            normalize_rule_suggestion(None)
                .expect("None should pass through")
                .is_none(),
            "None should round-trip as Ok(None)"
        );
    }

    // ── C-1 fix: index presence (existing) ──────────────────────────────────

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
