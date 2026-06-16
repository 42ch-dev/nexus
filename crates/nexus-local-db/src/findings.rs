//! Finding CRUD operations for the quality-loop (novel-quality-loop §2.1, V1.39 P1).
//!
//! Manages the `findings` table — quality issues surfaced by the
//! review stage (`novel-chapter-review` preset, V1.47) during auto-chain
//! or on-demand `creator run`. Each finding is scoped to a Work (and
//! optionally a chapter), carries a `kind`, severity, status lifecycle,
//! optional `rule_suggestion`, and a routing hint (`target_executor`)
//! indicating which preset should address it.

use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::error::LocalDbError;

/// Finding record — mirrors DB row.
///
/// V1.48 P1: derives `Deserialize` so the CLI can round-trip finding rows
/// fetched from the daemon Local API (`GET /v1/local/works/{id}/findings`)
/// back into the orchestration builder
/// ([`nexus_orchestration::findings_block::build_open_findings_block`])
/// without a parallel DTO struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    /// V1.48 P3 T3 — tri-state `rule_suggestion` patch.
    ///
    /// - `None` → omit (do not touch the column).
    /// - `Some(Some(value))` → set `rule_suggestion` to `value`.
    /// - `Some(None)` → **clear** `rule_suggestion` to SQL NULL.
    ///
    /// This resolves R-V147P0-03: the previous `COALESCE(?, rule_suggestion)`
    /// semantics made it impossible to clear the column because SQL `NULL`
    /// was treated as "use the existing value" rather than "set to NULL".
    pub rule_suggestion: Option<Option<String>>,
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

/// Valid status values.
///
/// V1.39 (R-V139P1-W-1): original three-state minimum (`open`, `resolved`,
/// `wont_fix`).
///
/// V1.49 (F6 — `findings-lifecycle.md` §2): extended to six states by adding
/// `triaged`, `in_review`, `duplicate`. Existing rows keep their original
/// status values; the runtime set is the sole enforcement (`SQLite` `ALTER`
/// `TABLE` cannot add `CHECK` constraints to existing tables — see
/// [`validate_finding_enums`] and migration `202606170001_extend_findings_status.sql`).
pub const VALID_STATUSES: &[&str] = &[
    "open",
    "resolved",
    "wont_fix",
    "triaged",
    "in_review",
    "duplicate",
];

/// V1.49 F6 — status strings considered **actionable** for the prompt
/// consumer ([`list_open_findings_for_chapter`] / `open_findings_block`).
///
/// Implements `findings-lifecycle.md` §2.2: findings with status in
/// `{ open, triaged }` are included in produce prompts; `in_review` is
/// **excluded** by default (the master-review preset owns that surface),
/// as are the terminal statuses (`resolved`, `wont_fix`, `duplicate`).
pub const ACTIONABLE_FINDING_STATUSES: &[&str] = &["open", "triaged"];

/// V1.49 F6 — returns `true` when `status` is a member of [`VALID_STATUSES`].
///
/// Originally specified as `const fn` in the plan; the stable Rust toolchain
/// (1.93) does not yet allow `matches!` on `&str` in `const` contexts
/// (issue `rust-lang/rust#143874`). The runtime signature is identical —
/// no current caller requires const evaluation. Promote to `const fn` once
/// `PartialEq` for `str` stabilises in const.
#[must_use]
pub fn is_valid_status(status: &str) -> bool {
    VALID_STATUSES.contains(&status)
}

/// V1.49 F6 — Returns `true` when transitioning `from` → `to` is permitted
/// by the lifecycle diagram in `findings-lifecycle.md` §2.1.
///
/// ```text
/// open → triaged | in_review | resolved | wont_fix | duplicate
/// triaged → in_review | resolved | wont_fix | duplicate
/// in_review → resolved | wont_fix | duplicate
/// resolved → (terminal)
/// wont_fix → (terminal)
/// duplicate → (terminal)
/// ```
///
/// `from == to` is **not** a transition and is rejected: callers that want
/// to refresh `updated_at` without changing status must omit `status` from
/// the patch. Both endpoints must already be valid members of
/// [`VALID_STATUSES`]; unknown endpoints return `false`.
///
/// V1.49 P0 W-1 (qc2 S-3): `duplicate` and `in_review` are owner-controlled
/// "hide from prompt" levers. `duplicate` is a **terminal sink** (no
/// outbound edges) — it is not a "parked for later" state. `in_review` is
/// the intended holding pen for master review: it is reachable from
/// `open`/`triaged` and can only advance to a terminal state. Moving a
/// finding into either state excludes it from the actionable set
/// (`open` | `triaged`) and from the produce-prompt consumer; this is by
/// design and creator-scoped (no cross-creator abuse is possible).
#[must_use]
pub fn is_valid_transition(from: &str, to: &str) -> bool {
    if from == to || !is_valid_status(from) || !is_valid_status(to) {
        return false;
    }
    match from {
        "open" => matches!(
            to,
            "triaged" | "in_review" | "resolved" | "wont_fix" | "duplicate"
        ),
        "triaged" => matches!(to, "in_review" | "resolved" | "wont_fix" | "duplicate"),
        "in_review" => matches!(to, "resolved" | "wont_fix" | "duplicate"),
        // Terminal states and (unreachable) unknowns both reject every
        // outbound transition. `is_valid_status` already filtered the
        // unknown `from` case above, so this arm is only reached for the
        // three terminal statuses.
        _ => false,
    }
}

/// Valid `target_executor` values (R-V139P1-W-1).
pub const VALID_TARGET_EXECUTORS: &[&str] = &["write", "brainstorm", "none", "master"];

/// V1.48 P3 T0 — default retention window for resolved findings (days).
///
/// Findings with `status = 'resolved'` whose `updated_at` is older than this
/// many days are eligible for pruning by [`prune_resolved_findings_older_than`].
/// `open` and `wont_fix` rows are never purged.
///
/// **Design decision (T0)**: the retention trigger is a **CLI command**
/// (`creator works findings prune`), not a daemon periodic task. Rationale:
/// simpler, manual control, no background scheduler complexity. The DAO
/// function is the single hook both a future CLI subcommand and a potential
/// daemon task would call.
///
/// **Spec note**: `archived/knowledge/novel-findings-maturity.md` §5.1 lists both `resolved` and
/// `wont_fix` as purge-eligible. The V1.48 P3 Assignment (T2) explicitly
/// restricts pruning to `resolved` only and skips `wont_fix`. This deviation
/// is intentional for this delivery slice; PM reconciles with the overlay at
/// P-last merge.
pub const RETENTION_DEFAULT_DAYS: i64 = 90;

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
/// per `.mstar/knowledge/specs/novel-writing/quality-loop.md` §2.1 (the V1.47 P0
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
    /// V1.48 P0: added per `novel-writing/quality-loop.md` §2.1.
    PlotHole,
    /// World-building inconsistency (timeline, geography, lore).
    /// V1.48 P0: added per `novel-writing/quality-loop.md` §2.1.
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
/// V1.49 F6: the status set expanded to six states (see [`VALID_STATUSES`]).
/// Transition legality (open → triaged, etc.) is **not** checked here — it is
/// enforced by [`update_finding`] which fetches the row's current status
/// before applying the patch. Create paths always seed `status = "open"`, so
/// there is no transition to validate at create time.
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

/// V1.48 P1 — chapter-scoped **actionable** findings query for
/// `novel-writing` prompt injection (`findings-lifecycle.md` §2 Consumer).
///
/// V1.49 F6: the actionable set expanded from `status = 'open'` to
/// `status IN ('open', 'triaged')` per overlay §2.2. `triaged` findings
/// (reviewed but not yet addressed) must reach the produce prompt;
/// `in_review` and the terminal statuses are excluded by default.
///
/// Returns all actionable findings for a Work that should influence the
/// `novel-writing` outline/draft prompt for chapter `N`:
///
/// - rows where `work_id` matches AND `status IN ('open', 'triaged')`
///   AND (`chapter = N` OR `chapter IS NULL`)
///
/// Work-level findings (`chapter IS NULL`) are included so a Work-wide
/// quality issue (e.g. a continuity break that spans chapters) reaches
/// every chapter's prompt, per overlay §2.1.
///
/// **Ordering** (overlay §2.1): `severity` DESC (blocker first, then
/// major, minor, info), then `created_at` ASC (oldest first within a
/// severity bucket). The DAO does NOT impose a count cap — the
/// orchestration builder ([`nexus_orchestration::findings_block`])
/// truncates per the overlay §2.2 limits.
///
/// The function is creator-scoped for the same isolation reasons as
/// [`list_findings`] and [`list_stale_open_findings`].
///
/// **Actionable set source**: [`ACTIONABLE_FINDING_STATUSES`] is the
/// canonical constant; callers that need to mirror the filter (e.g. an
/// HTTP query builder) should read that constant rather than re-hardcoding.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the database query fails.
pub async fn list_open_findings_for_chapter(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    chapter: i64,
) -> Result<Vec<Finding>, LocalDbError> {
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
           AND work_id = ?
           AND status IN ('open', 'triaged')
           AND (chapter = ? OR chapter IS NULL)
         ORDER BY
           CASE severity
             WHEN 'blocker' THEN 4
             WHEN 'major'   THEN 3
             WHEN 'minor'   THEN 2
             ELSE                1
           END DESC,
           created_at ASC",
        creator_id,
        work_id,
        chapter,
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

/// V1.49 F6 — read the current status and reject illegal transitions
/// before the caller writes the new value.
///
/// Returns:
/// - `Ok(())` when the row is missing (the UPDATE will surface `NotFound`
///   via `rows_affected = 0`), or when `current → new_status` is a legal edge.
/// - `Err(IllegalTransition)` when the transition violates
///   [`is_valid_transition`].
///
/// V1.49 P0 W-1: the typed [`LocalDbError::IllegalTransition`] variant
/// carries the structured `from`/`to` pair so the PATCH handler can map it
/// to the stable `INVALID_TRANSITION` code without inspecting constraint
/// text (qc1 W-1 / qc2 W-1).
///
/// V1.49 P0 W-1 (qc1 S-3 / qc3 S-3): this read-before-write is best-effort
/// single-statement under `SQLite`'s serialized writer. A stronger
/// atomicity guarantee is achievable by folding the guard into the UPDATE
/// as a compare-and-swap, e.g.
/// `UPDATE findings SET status = ?, updated_at = ? WHERE creator_id = ? AND finding_id = ? AND status = ?`
/// and detecting a lost race via `rows_affected()`. The current two-statement
/// form is retained for V1.49; the CAS form is the documented upgrade path.
///
/// # Errors
///
/// Returns [`LocalDbError::IllegalTransition`] on illegal transition and
/// [`LocalDbError::Sqlx`] on database failure.
async fn enforce_status_transition(
    pool: &SqlitePool,
    creator_id: &str,
    finding_id: &str,
    new_status: &str,
) -> Result<(), LocalDbError> {
    // SAFETY: runtime query_scalar — only the `status` column is read, and
    // the WHERE clause is creator-scoped. Equivalent to `get_finding` but
    // cheaper (single column, no FromRow mapping).
    let current_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM findings WHERE creator_id = ? AND finding_id = ?")
            .bind(creator_id)
            .bind(finding_id)
            .fetch_optional(pool)
            .await?;

    let Some(from) = current_status else {
        // Row not found — fall through to the UPDATE, which will report
        // `rows_affected = 0` and the caller surfaces NotFound.
        return Ok(());
    };

    if is_valid_transition(&from, new_status) {
        Ok(())
    } else {
        // V1.49 P0 W-1: typed variant so the PATCH handler maps this to
        // `INVALID_TRANSITION` without string-prefix sniffing.
        Err(LocalDbError::IllegalTransition {
            from,
            to: new_status.to_string(),
        })
    }
}

/// Update (patch) a finding, scoped to a creator.
///
/// Only non-`None` fields in `patch` are applied. `updated_at` is always
/// set to `now_epoch`.
///
/// **V1.48 P3 T3** (R-V147P0-03): `rule_suggestion` uses a tri-state
/// [`Option<Option<String>>`] — `Some(None)` clears the column to SQL NULL,
/// `None` omits it entirely. This required switching from a compile-time
/// `COALESCE` UPDATE (which treated SQL NULL as "keep existing") to a dynamic
/// SET-clause builder (mirrors [`works::patch_work`]).
///
/// **V1.49 F6** (`findings-lifecycle.md` §2.1): when `patch.status` is
/// `Some(new_status)`, the DAO fetches the row's current status and rejects
/// the transition via [`is_valid_transition`]. Invalid transitions return
/// [`LocalDbError::IllegalTransition`] and invalid enum values
/// (`severity` / `status` membership / `target_executor`) return
/// [`LocalDbError::InvalidEnum`] — both of which the daemon API maps to
/// HTTP 422 with distinct stable codes (`INVALID_TRANSITION` vs
/// `INVALID_INPUT` — see
/// `nexus-daemon-runtime::api::handlers::findings::update_finding_handler`).
/// This read-before-write is best-effort single-statement: under concurrent
/// writes a TOCTOU race is possible, but `SQLite` serialises writes and the
/// WHERE clause scopes the UPDATE to `(creator_id, finding_id)`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails, the finding is not
/// found, `patch.status` proposes an illegal lifecycle transition
/// ([`LocalDbError::IllegalTransition`]), or a patched enum field is not a
/// member of its allowed set ([`LocalDbError::InvalidEnum`]).
pub async fn update_finding(
    pool: &SqlitePool,
    creator_id: &str,
    finding_id: &str,
    patch: &FindingPatch,
    now_epoch: i64,
) -> Result<bool, LocalDbError> {
    // R-V139P1-W-1: validate patch enum fields if provided.
    // V1.49 P0 W-1: PATCH-path enum failures emit the typed `InvalidEnum`
    // variant (mapped to `INVALID_INPUT` by the handler) so they are
    // distinguishable from illegal transitions (`INVALID_TRANSITION`). The
    // create path continues to use the shared `validate_finding_enums`
    // helper, which still emits `ConstraintViolation`.
    if let Some(ref sev) = patch.severity {
        if !VALID_SEVERITIES.contains(&sev.as_str()) {
            return Err(LocalDbError::InvalidEnum {
                field: "severity",
                value: sev.clone(),
                allowed: VALID_SEVERITIES,
            });
        }
    }
    if let Some(ref st) = patch.status {
        if !VALID_STATUSES.contains(&st.as_str()) {
            return Err(LocalDbError::InvalidEnum {
                field: "status",
                value: st.clone(),
                allowed: VALID_STATUSES,
            });
        }
    }
    if let Some(ref te) = patch.target_executor {
        if !VALID_TARGET_EXECUTORS.contains(&te.as_str()) {
            return Err(LocalDbError::InvalidEnum {
                field: "target_executor",
                value: te.clone(),
                allowed: VALID_TARGET_EXECUTORS,
            });
        }
    }

    // V1.49 F6: enforce the lifecycle transition table. When the caller
    // patches `status`, fetch the row's current status and reject illegal
    // transitions (e.g. resolved → open, terminal → anything) before any
    // write. The DAO already validates that the target string is a known
    // status (above), so `is_valid_transition` only needs to evaluate the
    // state-machine edges.
    if let Some(ref new_status) = patch.status {
        enforce_status_transition(pool, creator_id, finding_id, new_status).await?;
    }

    // Build the SET clause dynamically so `rule_suggestion = ?` is only
    // included when the caller explicitly wants to touch the column.
    // This lets `Some(None)` bind SQL NULL (clear) rather than COALESCE
    // back to the existing value.
    let mut set_clauses = Vec::new();
    if patch.severity.is_some() {
        set_clauses.push("severity = ?");
    }
    if patch.status.is_some() {
        set_clauses.push("status = ?");
    }
    if patch.title.is_some() {
        set_clauses.push("title = ?");
    }
    if patch.description.is_some() {
        set_clauses.push("description = ?");
    }
    if patch.target_executor.is_some() {
        set_clauses.push("target_executor = ?");
    }
    if patch.kind.is_some() {
        set_clauses.push("kind = ?");
    }
    if patch.rule_suggestion.is_some() {
        set_clauses.push("rule_suggestion = ?");
    }
    set_clauses.push("updated_at = ?");

    // SAFETY: dynamic SQL — conditional SET clauses for tri-state
    // rule_suggestion. Mirrors the patch_work pattern in works.rs; bind order
    // matches set_clauses order exactly.
    let sql = format!(
        "UPDATE findings SET {} WHERE creator_id = ? AND finding_id = ?",
        set_clauses.join(", ")
    );
    let mut q = sqlx::query(&sql);
    if let Some(ref v) = patch.severity {
        q = q.bind(v);
    }
    if let Some(ref v) = patch.status {
        q = q.bind(v);
    }
    if let Some(ref v) = patch.title {
        q = q.bind(v);
    }
    if let Some(ref v) = patch.description {
        q = q.bind(v);
    }
    if let Some(ref v) = patch.target_executor {
        q = q.bind(v);
    }
    if let Some(ref v) = patch.kind {
        q = q.bind(v);
    }
    // rule_suggestion is Option<Option<String>> — the if-guard unwraps the
    // outer Option; the inner &Option<String> binds directly (None → NULL).
    if let Some(ref v) = patch.rule_suggestion {
        q = q.bind(v);
    }
    q = q.bind(now_epoch);
    q = q.bind(creator_id);
    q = q.bind(finding_id);
    let result = q.execute(pool).await?;
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

/// V1.48 P3 T2 — purge resolved findings older than the retention window.
///
/// Deletes every finding row with `status = 'resolved'` AND
/// `updated_at < (now_epoch - retention_seconds)`. Returns the number of
/// rows deleted.
///
/// **Retention clock**: `updated_at` (epoch seconds) is used as the proxy
/// for "when the finding was resolved" — [`update_finding`] stamps it on
/// every transition including status → `'resolved'`. No separate
/// `resolved_at` column exists; this is the simplest durable choice that
/// avoids a schema migration for a new column.
///
/// **Scope**: only `'resolved'` rows are purged. `'open'` and `'wont_fix'`
/// rows are never touched (per V1.48 P3 Assignment T2). See
/// [`RETENTION_DEFAULT_DAYS`] for the spec-deviation note.
///
/// **Hermetic testing**: `now_epoch` is a parameter (not wall-clock) so the
/// daemon task / CLI command / tests can exercise the cutoff deterministically
/// — same pattern as [`list_stale_open_findings`].
///
/// **Transaction**: the DELETE runs inside a single `SQLite` transaction so
/// the prune is atomic and future archival side-effects can be added in the
/// same tx without changing the public signature.
///
/// # Errors
///
/// Returns `LocalDbError` if the transaction cannot be started, the DELETE
/// fails, or the commit fails.
pub async fn prune_resolved_findings_older_than(
    pool: &SqlitePool,
    now_epoch: i64,
    retention_seconds: i64,
) -> Result<u32, LocalDbError> {
    let cutoff = now_epoch.saturating_sub(retention_seconds);
    let mut tx = pool.begin().await?;
    let result = sqlx::query!(
        "DELETE FROM findings WHERE status = 'resolved' AND updated_at < ?",
        cutoff
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(u32::try_from(result.rows_affected()).unwrap_or(u32::MAX))
}

/// Count `resolved` findings older than `retention_seconds` without deleting.
///
/// The read-only counterpart of [`prune_resolved_findings_older_than`], used
/// by the `creator works findings prune --dry-run` preview (V1.49 P3,
/// `novel-writing/quality-loop.md` §9.4).
///
/// Same cutoff semantics: `status = 'resolved'` AND `updated_at < now_epoch -
/// retention_seconds`. `open` and `wont_fix` rows are never counted (per the
/// retention scope documented on [`prune_resolved_findings_older_than`]).
///
/// Uses a runtime query (rather than `sqlx::query_scalar!`) so this additive
/// seam does not churn the shared `.sqlx/` offline cache, matching the
/// runtime-query precedent in `work_chapters` (waiver R-V140P0-S3).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_resolved_findings_older_than(
    pool: &SqlitePool,
    now_epoch: i64,
    retention_seconds: i64,
) -> Result<i64, LocalDbError> {
    let cutoff = now_epoch.saturating_sub(retention_seconds);
    // SAFETY: SELECT COUNT against findings — runtime query (R-V140P0-S3).
    let row = sqlx::query(
        "SELECT COUNT(*) AS cnt FROM findings WHERE status = 'resolved' AND updated_at < ?",
    )
    .bind(cutoff)
    .fetch_one(pool)
    .await?;
    let count: i64 = row.get("cnt");
    Ok(count)
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
/// V1.49 P0 W-1 (qc1 S-2): this intentionally queries `status = 'open'`
/// only — **not** the actionable set (`open` | `triaged`). Stale detection
/// is about unactioned *open* findings; the V1.49 actionable-set widening
/// applies solely to the produce-prompt consumer
/// ([`list_open_findings_for_chapter`]).
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
/// V1.49 P0 W-1 (qc1 S-2): this intentionally queries `status = 'open'`
/// only — **not** the actionable set (`open` | `triaged`). The severity
/// summary is the "open" bucket specifically; the V1.49 actionable-set
/// widening applies solely to the produce-prompt consumer
/// ([`list_open_findings_for_chapter`]).
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
/// does not create duplicate findings (novel-writing/quality-loop.md §8.3 decision).
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
    use super::{
        is_valid_status, is_valid_transition, list_open_findings_for_chapter,
        normalize_rule_suggestion, prune_resolved_findings_older_than, update_finding, Finding,
        FindingKind, FindingPatch, ACTIONABLE_FINDING_STATUSES, RULE_SUGGESTION_MAX_BYTES,
        VALID_STATUSES,
    };
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
        // and `world_inconsistency` per `novel-writing/quality-loop.md` §2.1.
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
    /// Per novel-writing/quality-loop.md §2.1: chapter-scoped finding lookups
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

    // ── V1.48 P1 (T4): chapter-scoped open-findings query ────────────────────

    /// Build a minimal `Finding` row for insertion in tests.
    fn row(
        id: &str,
        work_id: &str,
        chapter: Option<i64>,
        severity: &str,
        title: &str,
        created_at: i64,
        status: &str,
    ) -> Finding {
        Finding {
            finding_id: id.to_string(),
            work_id: work_id.to_string(),
            chapter,
            severity: severity.to_string(),
            status: status.to_string(),
            title: title.to_string(),
            description: "desc".to_string(),
            target_executor: "write".to_string(),
            creator_id: "ctr_test".to_string(),
            kind: "craft".to_string(),
            rule_suggestion: None,
            created_at,
            updated_at: created_at,
        }
    }

    /// V1.48 P1 T4 — `list_open_findings_for_chapter` filters by chapter AND
    /// work-level (chapter IS NULL) and orders per overlay §2.1
    /// (severity DESC, then created_at ASC). Closed/resolved rows and
    /// other-chapter rows are excluded.
    #[tokio::test]
    async fn list_open_findings_for_chapter_filters_by_chapter_and_work_level() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_a";
        seed_minimal_work(&pool, WORK, CREATOR).await;
        seed_minimal_work(&pool, "wrk_other", CREATOR).await;

        // Seed:
        //  - chapter=1 open (minor)
        //  - chapter=2 open (blocker)         ← must NOT appear for chapter=1
        //  - chapter=NULL open (major)        ← Work-level, must appear
        //  - chapter=1 resolved (major)       ← must NOT appear
        //  - chapter=1 wont_fix (blocker)     ← must NOT appear
        //  - different work_id, chapter=1     ← must NOT appear
        super::create_finding(
            &pool,
            &row("f1", WORK, Some(1), "minor", "ch1-minor", 100, "open"),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row("f2", WORK, Some(2), "blocker", "ch2-blocker", 100, "open"),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row("f3", WORK, None, "major", "work-level-major", 50, "open"),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "f4",
                WORK,
                Some(1),
                "major",
                "ch1-resolved",
                200,
                "resolved",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "f5",
                WORK,
                Some(1),
                "blocker",
                "ch1-wontfix",
                300,
                "wont_fix",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "f6",
                "wrk_other",
                Some(1),
                "blocker",
                "other-work",
                100,
                "open",
            ),
        )
        .await
        .unwrap();

        let got = list_open_findings_for_chapter(&pool, CREATOR, WORK, 1)
            .await
            .unwrap();

        // Expect: f3 (work-level major), f1 (ch1 minor) in severity-desc order.
        let ids: Vec<&str> = got.iter().map(|f| f.finding_id.as_str()).collect();
        assert_eq!(ids, vec!["f3", "f1"],
            "expected [work-level-major (severity major), ch1-minor (severity minor)] in that order; got {ids:?}");

        // Verify chapter predicate explicitly.
        for f in &got {
            assert!(
                f.chapter == Some(1) || f.chapter.is_none(),
                "found finding for chapter {:?} should not appear for chapter 1",
                f.chapter
            );
        }
    }

    /// V1.48 P1 T4 — ordering check: when multiple findings share a severity,
    /// the tiebreaker is `created_at` ASC (oldest first within the bucket).
    #[tokio::test]
    async fn list_open_findings_for_chapter_orders_by_created_at_asc_within_severity() {
        let (pool, _dir) = fresh_pool().await;
        const WORK: &str = "wrk_b";
        seed_minimal_work(&pool, WORK, "ctr_test").await;

        // Three minors on chapter 1, inserted with decreasing created_at.
        super::create_finding(
            &pool,
            &row("g1", WORK, Some(1), "minor", "newest", 5000, "open"),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row("g2", WORK, Some(1), "minor", "middle", 3000, "open"),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row("g3", WORK, Some(1), "minor", "oldest", 1000, "open"),
        )
        .await
        .unwrap();

        let got = list_open_findings_for_chapter(&pool, "ctr_test", WORK, 1)
            .await
            .unwrap();
        let ids: Vec<&str> = got.iter().map(|f| f.finding_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["g3", "g2", "g1"],
            "expected oldest-first within minor bucket; got {ids:?}"
        );
    }

    /// V1.48 P1 T4 — empty result when no open findings match.
    #[tokio::test]
    async fn list_open_findings_for_chapter_returns_empty_when_no_matches() {
        let (pool, _dir) = fresh_pool().await;
        // No seed — table is empty.
        let got = list_open_findings_for_chapter(&pool, "ctr_test", "wrk_c", 1)
            .await
            .unwrap();
        assert!(got.is_empty(), "expected empty result on unseeded table");
    }

    // ── V1.48 P3 T2: resolved-finding retention pruning ─────────────────────

    /// V1.48 P3 T2 — old `resolved` rows past the retention window are purged;
    /// `open` and `wont_fix` rows are never touched.
    ///
    /// Seeds:
    ///  - `old_resolved`   — resolved, updated_at well past the cutoff   ← purged
    ///  - `old_open`       — open,     created_at same vintage           ← kept (open)
    ///  - `old_wont_fix`   — wont_fix, updated_at same vintage           ← kept (wont_fix)
    ///  - `recent_resolved`— resolved, updated_at inside the window      ← kept (recent)
    #[tokio::test]
    async fn findings_retention_removes_old_resolved_rows() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_prune";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        let now: i64 = 10_000_000;
        let retention_seconds: i64 = 90 * 24 * 3_600; // 90 days
        let old_ts = now - retention_seconds - 1; // 1s past the cutoff

        super::create_finding(
            &pool,
            &row(
                "pr1",
                WORK,
                Some(1),
                "major",
                "old_resolved",
                old_ts,
                "resolved",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row("pr2", WORK, Some(1), "major", "old_open", old_ts, "open"),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "pr3",
                WORK,
                Some(1),
                "major",
                "old_wont_fix",
                old_ts,
                "wont_fix",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "pr4",
                WORK,
                Some(1),
                "major",
                "recent_resolved",
                now,
                "resolved",
            ),
        )
        .await
        .unwrap();

        let deleted = prune_resolved_findings_older_than(&pool, now, retention_seconds)
            .await
            .unwrap();
        assert_eq!(deleted, 1, "exactly one old resolved row should be pruned");

        // Verify: pr1 gone; pr2, pr3, pr4 still present.
        let remaining: Vec<String> = sqlx::query_scalar(
            "SELECT finding_id FROM findings WHERE work_id = ? ORDER BY finding_id",
        )
        .bind(WORK)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            remaining,
            vec!["pr2".to_string(), "pr3".to_string(), "pr4".to_string()],
            "open, wont_fix, and recent resolved rows must survive the prune"
        );
    }

    /// V1.49 P3 — `count_resolved_findings_older_than` previews the prune
    /// count without deleting, and the count matches what
    /// [`prune_resolved_findings_older_than`] actually deletes (§9.4).
    #[tokio::test]
    async fn findings_retention_count_preview_matches_prune() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_count";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        let now: i64 = 20_000_000;
        let retention_seconds: i64 = 90 * 24 * 3_600;
        let old_ts = now - retention_seconds - 100_000;

        // cp1: old resolved (eligible). cp2: old open (skipped).
        // cp3: old wont_fix (skipped). cp4: recent resolved (skipped).
        super::create_finding(
            &pool,
            &row(
                "cp1",
                WORK,
                Some(1),
                "major",
                "old_resolved",
                old_ts,
                "resolved",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row("cp2", WORK, Some(1), "major", "old_open", old_ts, "open"),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "cp3",
                WORK,
                Some(1),
                "major",
                "old_wont_fix",
                old_ts,
                "wont_fix",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "cp4",
                WORK,
                Some(1),
                "major",
                "recent_resolved",
                now,
                "resolved",
            ),
        )
        .await
        .unwrap();

        // Preview must report exactly the old resolved count (1)...
        let preview = super::count_resolved_findings_older_than(&pool, now, retention_seconds)
            .await
            .unwrap();
        assert_eq!(preview, 1, "count preview should report 1 old resolved row");

        // ...and nothing was deleted yet.
        let total_before: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM findings WHERE work_id = ?")
                .bind(WORK)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(total_before, 4, "count preview must not delete any rows");

        // The actual prune deletes the same count the preview reported.
        let deleted = prune_resolved_findings_older_than(&pool, now, retention_seconds)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
    }

    /// V1.48 P3 T2 — `open` rows are never purged even when their
    /// `updated_at` is far past the retention window.
    #[tokio::test]
    async fn findings_retention_skips_open_rows() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_prune_open";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        let now: i64 = 20_000_000;
        let retention_seconds: i64 = 90 * 24 * 3_600;
        let old_ts = now - retention_seconds - 100_000;

        // Only open rows — all old.
        super::create_finding(
            &pool,
            &row(
                "sk1",
                WORK,
                Some(1),
                "blocker",
                "old_open_1",
                old_ts,
                "open",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row("sk2", WORK, Some(1), "minor", "old_open_2", old_ts, "open"),
        )
        .await
        .unwrap();

        let deleted = prune_resolved_findings_older_than(&pool, now, retention_seconds)
            .await
            .unwrap();
        assert_eq!(deleted, 0, "no rows should be pruned when all are open");

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings WHERE work_id = ?")
            .bind(WORK)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 2, "both open rows must survive");
    }

    /// V1.48 P3 T2 — `resolved` rows whose `updated_at` falls inside the
    /// retention window are kept.
    #[tokio::test]
    async fn findings_retention_skips_recent_resolved_rows() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_prune_recent";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        let now: i64 = 30_000_000;
        let retention_seconds: i64 = 90 * 24 * 3_600;

        // A resolved row exactly at the cutoff boundary (not older).
        let boundary_ts = now - retention_seconds;
        // A resolved row 1 second inside the window.
        let inside_ts = now - retention_seconds + 1;

        super::create_finding(
            &pool,
            &row(
                "rc1",
                WORK,
                Some(1),
                "major",
                "boundary_resolved",
                boundary_ts,
                "resolved",
            ),
        )
        .await
        .unwrap();
        super::create_finding(
            &pool,
            &row(
                "rc2",
                WORK,
                Some(1),
                "major",
                "inside_resolved",
                inside_ts,
                "resolved",
            ),
        )
        .await
        .unwrap();

        let deleted = prune_resolved_findings_older_than(&pool, now, retention_seconds)
            .await
            .unwrap();
        // boundary_ts == cutoff → NOT < cutoff → kept.
        // inside_ts  > cutoff   → kept.
        assert_eq!(deleted, 0, "boundary and inside-window rows must survive");

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings WHERE work_id = ?")
            .bind(WORK)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 2, "both recent resolved rows must survive");
    }

    // ── V1.48 P3 T3: FindingPatch tri-state rule_suggestion (R-V147P0-03) ───

    /// Helper: build a FindingPatch with only `rule_suggestion` set.
    fn patch_rule_suggestion(v: Option<Option<String>>) -> FindingPatch {
        FindingPatch {
            rule_suggestion: v,
            ..Default::default()
        }
    }

    /// V1.48 P3 T3 — `Some(None)` clears `rule_suggestion` to SQL NULL.
    ///
    /// Seeds a finding with `rule_suggestion = Some("...")`, patches with
    /// `Some(None)`, then verifies the column is NULL after the update.
    #[tokio::test]
    async fn update_finding_can_clear_rule_suggestion_to_null() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_clr";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        // Seed with a non-empty rule_suggestion.
        let mut f = row("cl1", WORK, Some(1), "major", "clear-test", 1000, "open");
        f.rule_suggestion = Some("original suggestion".to_string());
        super::create_finding(&pool, &f).await.unwrap();

        // Verify seed.
        let before = super::get_finding(&pool, CREATOR, "cl1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            before.rule_suggestion.as_deref(),
            Some("original suggestion")
        );

        // Patch: Some(None) → clear to NULL.
        let updated = update_finding(
            &pool,
            CREATOR,
            "cl1",
            &patch_rule_suggestion(Some(None)),
            2000,
        )
        .await
        .unwrap();
        assert!(updated, "row should be updated");

        let after = super::get_finding(&pool, CREATOR, "cl1")
            .await
            .unwrap()
            .unwrap();
        assert!(
            after.rule_suggestion.is_none(),
            "rule_suggestion should be NULL after Some(None) clear; got {:?}",
            after.rule_suggestion
        );
    }

    /// V1.48 P3 T3 — `Some(Some(value))` sets `rule_suggestion` to the value.
    #[tokio::test]
    async fn update_finding_can_set_rule_suggestion() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_set";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        // Seed with no rule_suggestion (NULL).
        super::create_finding(
            &pool,
            &row("st1", WORK, Some(1), "major", "set-test", 1000, "open"),
        )
        .await
        .unwrap();

        // Patch: Some(Some("new value")) → set.
        let updated = update_finding(
            &pool,
            CREATOR,
            "st1",
            &patch_rule_suggestion(Some(Some("new suggestion".to_string()))),
            2000,
        )
        .await
        .unwrap();
        assert!(updated, "row should be updated");

        let after = super::get_finding(&pool, CREATOR, "st1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            after.rule_suggestion.as_deref(),
            Some("new suggestion"),
            "rule_suggestion should be set to the new value"
        );
    }

    /// V1.48 P3 T3 — `None` (outer) leaves `rule_suggestion` unchanged.
    #[tokio::test]
    async fn update_finding_can_omit_rule_suggestion_to_keep_unchanged() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_omit";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        // Seed with a non-empty rule_suggestion.
        let mut f = row("om1", WORK, Some(1), "major", "omit-test", 1000, "open");
        f.rule_suggestion = Some("keep me".to_string());
        super::create_finding(&pool, &f).await.unwrap();

        // Patch a DIFFERENT field (severity) while omitting rule_suggestion
        // (None outer). The rule_suggestion must survive unchanged.
        let mut patch = FindingPatch {
            severity: Some("minor".to_string()),
            ..Default::default()
        };
        patch.rule_suggestion = None; // omit
        let updated = update_finding(&pool, CREATOR, "om1", &patch, 2000)
            .await
            .unwrap();
        assert!(updated, "row should be updated");

        let after = super::get_finding(&pool, CREATOR, "om1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.severity, "minor", "severity should be updated");
        assert_eq!(
            after.rule_suggestion.as_deref(),
            Some("keep me"),
            "rule_suggestion must be unchanged when omitted from the patch"
        );
    }

    // ── V1.49 F6: status enum + lifecycle transitions ───────────────────────

    /// V1.49 F6 — `VALID_STATUSES` carries the six-state lifecycle enum
    /// per `findings-lifecycle.md` §2.
    #[test]
    fn valid_statuses_carries_six_state_lifecycle() {
        assert_eq!(
            VALID_STATUSES,
            &[
                "open",
                "resolved",
                "wont_fix",
                "triaged",
                "in_review",
                "duplicate"
            ],
            "VALID_STATUSES must match findings-lifecycle.md §2 enum"
        );
        assert_eq!(
            ACTIONABLE_FINDING_STATUSES,
            &["open", "triaged"],
            "ACTIONABLE_FINDING_STATUSES must match §2.2 actionable set"
        );
    }

    /// V1.49 F6 — `is_valid_status` accepts every lifecycle member and
    /// rejects unknown strings (including plausible-but-wrong variants
    /// like `closed`, `pending`, `triage`).
    #[test]
    fn is_valid_status_accepts_lifecycle_members_only() {
        for &known in VALID_STATUSES {
            assert!(
                is_valid_status(known),
                "is_valid_status should accept '{known}'"
            );
        }
        for &unknown in &[
            "",
            "closed",
            "pending",
            "triage",
            "in-review",
            "RESOLVED",
            "open ",
        ] {
            assert!(
                !is_valid_status(unknown),
                "is_valid_status should reject '{unknown}'"
            );
        }
    }

    /// V1.49 F6 — `is_valid_transition` implements the lifecycle diagram
    /// in `findings-lifecycle.md` §2.1. Every documented edge accepts; the
    /// three terminal states reject every outbound transition; `from == to`
    /// is rejected (no-op refresh must omit the patch field).
    #[test]
    fn is_valid_transition_matches_lifecycle_diagram() {
        // open → triaged | in_review | resolved | wont_fix | duplicate
        for to in ["triaged", "in_review", "resolved", "wont_fix", "duplicate"] {
            assert!(
                is_valid_transition("open", to),
                "open → {to} should be a valid transition"
            );
        }
        // triaged → in_review | resolved | wont_fix | duplicate
        for to in ["in_review", "resolved", "wont_fix", "duplicate"] {
            assert!(
                is_valid_transition("triaged", to),
                "triaged → {to} should be a valid transition"
            );
        }
        // in_review → resolved | wont_fix | duplicate
        for to in ["resolved", "wont_fix", "duplicate"] {
            assert!(
                is_valid_transition("in_review", to),
                "in_review → {to} should be a valid transition"
            );
        }
        // Forbidden regressions: open/triaged are not reachable from in_review.
        assert!(
            !is_valid_transition("in_review", "open"),
            "in_review → open should be rejected"
        );
        assert!(
            !is_valid_transition("in_review", "triaged"),
            "in_review → triaged should be rejected"
        );
        // Terminal states: no outbound transitions.
        for terminal in ["resolved", "wont_fix", "duplicate"] {
            for target in [
                "open",
                "triaged",
                "in_review",
                "resolved",
                "wont_fix",
                "duplicate",
            ] {
                assert!(
                    !is_valid_transition(terminal, target),
                    "{terminal} → {target} should be rejected (terminal state)"
                );
            }
        }
        // `from == to` is rejected (callers omit the patch field to refresh).
        for &s in VALID_STATUSES {
            assert!(
                !is_valid_transition(s, s),
                "self-transition {s} → {s} should be rejected; omit the patch field to refresh"
            );
        }
        // Unknown endpoints are rejected even on the otherwise-permissive
        // `open` source.
        assert!(
            !is_valid_transition("open", "closed"),
            "unknown target 'closed' should be rejected"
        );
        assert!(
            !is_valid_transition("closed", "open"),
            "unknown source 'closed' should be rejected"
        );
    }

    /// V1.49 F6 — `update_finding` accepts the canonical happy path
    /// `open → triaged → in_review → resolved` and stamps `updated_at`.
    #[tokio::test]
    async fn update_finding_accepts_canonical_lifecycle_path() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_lifecycle";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        super::create_finding(
            &pool,
            &row(
                "lc1",
                WORK,
                Some(1),
                "major",
                "lifecycle-test",
                1_000,
                "open",
            ),
        )
        .await
        .unwrap();

        let advance = |status: &str| {
            let status = status.to_string();
            let pool = pool.clone();
            async move {
                let patch = FindingPatch {
                    status: Some(status.clone()),
                    ..Default::default()
                };
                update_finding(&pool, CREATOR, "lc1", &patch, 2_000)
                    .await
                    .expect("transition should succeed");
                super::get_finding(&pool, CREATOR, "lc1")
                    .await
                    .unwrap()
                    .unwrap()
            }
        };

        let after_triage = advance("triaged").await;
        assert_eq!(after_triage.status, "triaged");
        assert_eq!(after_triage.updated_at, 2_000);

        let after_review = advance("in_review").await;
        assert_eq!(after_review.status, "in_review");

        let after_resolved = advance("resolved").await;
        assert_eq!(after_resolved.status, "resolved");
    }

    /// V1.49 F6 — `update_finding` accepts the direct terminal transitions
    /// `open → wont_fix` and `open → duplicate` (no intermediate triage).
    #[tokio::test]
    async fn update_finding_accepts_open_to_terminal_transitions() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_open_terminal";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        super::create_finding(
            &pool,
            &row("ot1", WORK, Some(1), "minor", "to-wont-fix", 1_000, "open"),
        )
        .await
        .unwrap();
        let patch = FindingPatch {
            status: Some("wont_fix".to_string()),
            ..Default::default()
        };
        update_finding(&pool, CREATOR, "ot1", &patch, 2_000)
            .await
            .expect("open → wont_fix should succeed");
        assert_eq!(
            super::get_finding(&pool, CREATOR, "ot1")
                .await
                .unwrap()
                .unwrap()
                .status,
            "wont_fix"
        );

        super::create_finding(
            &pool,
            &row("ot2", WORK, Some(1), "minor", "to-duplicate", 1_000, "open"),
        )
        .await
        .unwrap();
        let patch = FindingPatch {
            status: Some("duplicate".to_string()),
            ..Default::default()
        };
        update_finding(&pool, CREATOR, "ot2", &patch, 2_000)
            .await
            .expect("open → duplicate should succeed");
        assert_eq!(
            super::get_finding(&pool, CREATOR, "ot2")
                .await
                .unwrap()
                .unwrap()
                .status,
            "duplicate"
        );
    }

    /// V1.49 F6 — `update_finding` rejects illegal transitions with
    /// `IllegalTransition`. Three representative rejections cover the
    /// terminal-locked, self-loop, and reverse-edge classes.
    #[tokio::test]
    async fn update_finding_rejects_illegal_transitions() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_reject";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        // Seed a resolved row (terminal — no outbound transitions).
        super::create_finding(
            &pool,
            &row(
                "rj1",
                WORK,
                Some(1),
                "major",
                "resolved-row",
                1_000,
                "resolved",
            ),
        )
        .await
        .unwrap();

        // (a) resolved → open: rejected (terminal state).
        let err = update_finding(
            &pool,
            CREATOR,
            "rj1",
            &FindingPatch {
                status: Some("open".to_string()),
                ..Default::default()
            },
            2_000,
        )
        .await
        .expect_err("resolved → open must be rejected");
        match err {
            LocalDbError::IllegalTransition { from, to } => {
                assert_eq!(from.as_str(), "resolved");
                assert_eq!(to.as_str(), "open");
            }
            other => panic!("expected IllegalTransition, got {other:?}"),
        }

        // (b) self-loop resolved → resolved: rejected (callers must omit
        // the patch field to refresh `updated_at`).
        let err = update_finding(
            &pool,
            CREATOR,
            "rj1",
            &FindingPatch {
                status: Some("resolved".to_string()),
                ..Default::default()
            },
            2_000,
        )
        .await
        .expect_err("self-loop resolved → resolved must be rejected");
        assert!(
            matches!(err, LocalDbError::IllegalTransition { .. }),
            "self-loop should surface as IllegalTransition, got {err:?}"
        );

        // (c) reverse-edge in_review → open: rejected (in_review may only
        // advance to terminal states per §2.1).
        super::create_finding(
            &pool,
            &row(
                "rj2",
                WORK,
                Some(1),
                "major",
                "in-review-row",
                1_000,
                "in_review",
            ),
        )
        .await
        .unwrap();
        let err = update_finding(
            &pool,
            CREATOR,
            "rj2",
            &FindingPatch {
                status: Some("open".to_string()),
                ..Default::default()
            },
            2_000,
        )
        .await
        .expect_err("in_review → open must be rejected");
        assert!(
            matches!(err, LocalDbError::IllegalTransition { .. }),
            "reverse-edge should surface as IllegalTransition, got {err:?}"
        );

        // The rejected rows must be unchanged.
        assert_eq!(
            super::get_finding(&pool, CREATOR, "rj1")
                .await
                .unwrap()
                .unwrap()
                .status,
            "resolved",
            "rejected transition must not mutate the row"
        );
    }

    /// V1.49 F6 — `update_finding` still rejects unknown status values
    /// (the membership check precedes the transition check).
    ///
    /// V1.49 P0 W-1: the PATCH-path membership failure now surfaces as the
    /// typed `InvalidEnum` variant (field=`status`) rather than the generic
    /// `ConstraintViolation`, so the handler can map it to `INVALID_INPUT`.
    #[tokio::test]
    async fn update_finding_rejects_unknown_status_value() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_unknown_status";
        seed_minimal_work(&pool, WORK, CREATOR).await;
        super::create_finding(
            &pool,
            &row("uk1", WORK, Some(1), "minor", "uk", 1_000, "open"),
        )
        .await
        .unwrap();

        let err = update_finding(
            &pool,
            CREATOR,
            "uk1",
            &FindingPatch {
                status: Some("closed".to_string()),
                ..Default::default()
            },
            2_000,
        )
        .await
        .expect_err("unknown status value 'closed' must be rejected");
        assert!(
            matches!(
                err,
                LocalDbError::InvalidEnum {
                    field: "status",
                    ..
                }
            ),
            "unknown status should surface as InvalidEnum(field=status), got {err:?}"
        );
        assert!(
            err.to_string().contains("closed"),
            "InvalidEnum message should echo the rejected value: {err}"
        );
    }

    /// V1.49 F6 — `list_open_findings_for_chapter` includes `open` AND
    /// `triaged` rows (actionable set per §2.2) and excludes `in_review`
    /// and the terminal statuses by default.
    #[tokio::test]
    async fn list_open_findings_for_chapter_matches_v149_actionable_set() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_actionable";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        // Seed one finding per status, all on chapter 1, all the same
        // severity so the ordering is by created_at ASC within the bucket.
        // Insert in lifecycle-enum order so created_at is monotonic.
        let statuses = [
            ("open", 1_000),
            ("triaged", 2_000),
            ("in_review", 3_000),
            ("resolved", 4_000),
            ("wont_fix", 5_000),
            ("duplicate", 6_000),
        ];
        for (idx, (status, ts)) in statuses.iter().enumerate() {
            let id = format!("ac{}", idx);
            // Bypass create_finding's create-time validator by writing
            // directly so non-`open` seed rows exist for the SELECT.
            // SAFETY: test-only — direct INSERT to seed lifecycle states
            // that the create path (which forces status = 'open') cannot
            // produce. The runtime validation is the sole gate per
            // R-V139P1-W-1; the values are all members of VALID_STATUSES.
            sqlx::query(
                "INSERT INTO findings
                   (finding_id, work_id, chapter, severity, status, title,
                    description, target_executor, creator_id, kind,
                    created_at, updated_at)
                 VALUES (?, ?, 1, 'major', ?, ?, 'desc', 'write', ?, 'craft', ?, ?)",
            )
            .bind(&id)
            .bind(WORK)
            .bind(status)
            .bind(format!("{status}-seed"))
            .bind(CREATOR)
            .bind(ts)
            .bind(ts)
            .execute(&pool)
            .await
            .unwrap();
        }

        let got = list_open_findings_for_chapter(&pool, CREATOR, WORK, 1)
            .await
            .unwrap();
        let returned_statuses: Vec<String> = got.iter().map(|f| f.status.clone()).collect();
        assert_eq!(
            returned_statuses,
            vec!["open".to_string(), "triaged".to_string()],
            "actionable set must include only open + triaged; got {returned_statuses:?}"
        );

        // Severity ordering within the actionable bucket is preserved
        // (severity DESC, then created_at ASC) — both seeds share `major`,
        // so created_at ASC wins; open (ts=1000) precedes triaged (ts=2000).
        let ids: Vec<&str> = got.iter().map(|f| f.finding_id.as_str()).collect();
        assert_eq!(ids, vec!["ac0", "ac1"], "expected created_at ASC ordering");
    }

    /// V1.49 F6 — `list_open_findings_for_chapter` keeps the work-level
    /// (`chapter IS NULL`) inclusion for the new `triaged` status: a
    /// work-level triaged finding must reach every chapter's prompt.
    #[tokio::test]
    async fn list_open_findings_for_chapter_includes_work_level_triaged() {
        let (pool, _dir) = fresh_pool().await;
        const CREATOR: &str = "ctr_test";
        const WORK: &str = "wrk_work_triaged";
        seed_minimal_work(&pool, WORK, CREATOR).await;

        // Work-level triaged finding.
        // SAFETY: test-only direct INSERT — see previous test for rationale.
        sqlx::query(
            "INSERT INTO findings
               (finding_id, work_id, chapter, severity, status, title,
                description, target_executor, creator_id, kind,
                created_at, updated_at)
             VALUES ('wt1', ?, NULL, 'blocker', 'triaged', 'work-triaged',
                     'desc', 'write', ?, 'craft', 500, 500)",
        )
        .bind(WORK)
        .bind(CREATOR)
        .execute(&pool)
        .await
        .unwrap();

        let got = list_open_findings_for_chapter(&pool, CREATOR, WORK, 7)
            .await
            .unwrap();
        assert_eq!(got.len(), 1, "work-level triaged finding should reach ch7");
        assert_eq!(got[0].finding_id, "wt1");
        assert_eq!(got[0].status, "triaged");
    }

    /// Insert a minimal works row to satisfy the `findings.work_id` FK.
    async fn seed_minimal_work(pool: &SqlitePool, work_id: &str, creator_id: &str) {
        // SAFETY: test-only — minimal works row for FK satisfaction.
        sqlx::query(
            r"INSERT INTO works
                 (work_id, creator_id, workspace_slug, status, title,
                  long_term_goal, initial_idea, intake_status, inspiration_log,
                  primary_preset_id, schedule_ids, created_at, updated_at,
                  current_stage, stage_status)
               VALUES (?, ?, 'ws', 'active', 't', 'g', 'i', 'complete', '[]',
                       'novel-writing', '[]', '2026-06-16T10:00:00Z',
                       '2026-06-16T10:00:00Z', 'produce', 'active')",
        )
        .bind(work_id)
        .bind(creator_id)
        .execute(pool)
        .await
        .unwrap();
    }
}
