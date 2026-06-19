//! Auto-chain engine — `on_complete` handler for FL-E stage advancement and
//! chapter outer loop (V1.39 §5.4–5.6).
//!
//! When a schedule reaches a terminal state, this module determines the next
//! step for auto-chain-enabled Works:
//!
//! 1. **Stage advance**: after intake/research/review completes → enqueue next stage
//! 2. **Chapter loop**: after persist for chapter N → enqueue produce for chapter N+1
//!    (if chapters remain)
//! 3. **Work completion**: after persist for the last chapter → mark Work completed
//!
//! Checkpoint fields on the Work record track the continuation state:
//! - `auto_chain_enabled`: whether auto-chain is active (default true)
//! - `driver_schedule_id`: the currently-running FL-E driver schedule
//! - `auto_chain_interrupted`: set when driver is interrupted externally

use nexus_contracts::local::orchestration::{stage_index, FL_E_STAGES};
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_local_db::findings::{self, ReviewVerdictFinding};
use nexus_local_db::novel_pool_entries;
use nexus_local_db::works::{self, WorkPatch, WorkRecord};
use sqlx::SqlitePool;

use crate::completion_lock::{self, CompletionLock};

/// R-V139P0-W-B: per-process monotonic counter for ACH schedule ID collision resistance.
static ACH_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// R-V147P0-05 (hotfix H-1): per-process monotonic counter for `RVM` schedule
/// ID collision resistance. The previous `enqueue_review_master_schedule` minted
/// `RVM<ts_ms>` which collided when the stale-findings watcher (or repeated
/// sweeps within one tick) enqueued two opt-in Works in the same millisecond.
/// Mirrors the `ACH_COUNTER` fix (R-V139P0-W-B) for the same class of bug.
static RVM_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// V1.50 T-A P1: per-process monotonic counter for cron-triggered schedule ID
/// collision resistance. Two cron roles can fire in the same millisecond
/// (e.g. brainstorm + write both matching the same minute), so the bare
/// `CRON<ts_ms>` prefix would collide. Mirrors `ACH_COUNTER` / `RVM_COUNTER`.
static CRON_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

use crate::stage_gates::{self, WorkFields};

/// Result of an `on_schedule_complete` evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainAction {
    /// No further action needed (auto-chain disabled, work complete, or not an FL-E driver).
    NoAction,
    /// Advance to the next FL-E stage for the current chapter.
    AdvanceStage { work_id: String, next_stage: String },
    /// Start the produce stage for the next chapter (chapter outer loop).
    /// V1.42: includes volume for cross-volume chaining.
    NextChapter {
        work_id: String,
        next_chapter: i32,
        next_volume: i32,
    },
    /// The Work is complete — all chapters finalized.
    WorkComplete { work_id: String },
}

/// Error type for auto-chain operations.
#[derive(Debug, thiserror::Error)]
pub enum AutoChainError {
    /// Database operation failed.
    #[error("database error: {0}")]
    Database(#[from] nexus_local_db::LocalDbError),
    /// Work record not found.
    #[error("work not found: {0}")]
    WorkNotFound(String),
    /// Invalid state for auto-chain operation.
    #[error("invalid state: {0}")]
    InvalidState(String),
}

/// Look up the Work record associated with a completed schedule.
///
/// Matches by `driver_schedule_id` on the works table. Returns `None` if no
/// Work has this schedule as its driver (e.g., non-FL-E schedules).
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the database query fails.
pub async fn find_work_for_driver(
    pool: &SqlitePool,
    schedule_id: &str,
) -> Result<Option<WorkRecord>, AutoChainError> {
    // SAFETY: dynamic SQL — driver_schedule_id lookup is a simple equality filter.
    let row = sqlx::query(&format!(
        "SELECT {} FROM works WHERE driver_schedule_id = ? LIMIT 1",
        works::WORKS_COLUMNS
    ))
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    Ok(row.as_ref().map(works::row_to_work_record))
}

/// V1.47 P0 — Review-stage findings producer hook.
///
/// This is the **single code path** (spec §5.5.6 "Trigger paths — both
/// required") that persists ≥1 finding row when a `novel-chapter-review`
/// schedule reaches terminal status. It is called from the supervisor's
/// `on_schedule_terminal(Completed)` for **both** the auto-chain driver
/// schedule and on-demand `creator run novel-chapter-review <work_id>`
/// schedules, satisfying acceptance criteria #1 and #2 of the V1.47 P0 plan.
///
/// # Behavior
///
/// 1. Loads the schedule row to read `preset_id`, `work_id`, `creator_id`.
/// 2. Returns `Ok(0)` early when the preset is not `novel-chapter-review`
///    (no-op for non-review schedules).
/// 3. Loads the Work record for `chapter` context (`work.current_chapter`)
///    and `work_ref` (for the report path).
/// 4. **V1.48 P0 T2**: when `workspace_dir` is `Some`, reads
///    `Works/<work_ref>/Logs/review/review-report.md` and parses it via
///    [`crate::review_report::parse_review_report`]. When ≥1 finding parses,
///    each row is persisted via [`findings::create_finding_from_review_tx`]
///    inside a single transaction with the parsed `kind` / `severity` /
///    `body` / optional `rule_suggestion`
///    (per `.mstar/archived/knowledge/novel-findings-maturity.md` §1.2).
/// 5. **Fallback** (V1.47 placeholder shape): when the report is missing,
///    unparsable, or yields zero findings — OR when `workspace_dir` is
///    `None` (e.g. hermetic DB-only tests) — synthesizes ≥1 finding with
///    safe defaults (`kind=craft`, `severity=info`, `target_executor=none`)
///    and emits `tracing::warn!` so operators can see the degrade (spec §1.3).
///
/// Spec §8.4 invariant: finding creation MUST NOT fork or cancel the active
/// FL-E driver schedule. This function performs only DB INSERT(s) and does
/// not touch `driver_schedule_id`.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the schedule/Work lookup or the
/// finding INSERT fails. Errors are logged by the caller and do **not**
/// block the supervisor terminal transition.
pub async fn persist_review_findings_for_schedule(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
) -> Result<usize, AutoChainError> {
    // R-V147P0-06 (V1.48 P0 T3): hoisted to `preset_ids` SSOT; referenced
    // from the supervisor terminal guard and the STAGE_PRESET_ALLOWLIST too.
    use crate::preset_ids::NOVEL_CHAPTER_REVIEW_PRESET_ID as REVIEW_PRESET_ID;

    // SAFETY: dynamic SQL — single-row schedule lookup by PK. `work_id` is
    // nullable (added in 202606080002_creator_schedules_work_id.sql), so we
    // cannot use `as "work_id!"` (NOT NULL assertion). `creator_id` and
    // `preset_id` are NOT NULL per the original 20260419 migration.
    let row = sqlx::query(
        "SELECT preset_id, work_id, creator_id
         FROM creator_schedules
         WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    let Some(row) = row else {
        // Schedule row missing — nothing to do (caller already updated status).
        tracing::debug!(
            schedule_id,
            "review-findings: schedule row not found; skipping"
        );
        return Ok(0);
    };

    // SAFETY: dynamic-SQL row → typed fields via ColumnIndex + TypeCheck.
    // Columns are positional/named; sqlx runtime decode is fine for this
    // nullable schema.
    let preset_id: String = sqlx::Row::try_get(&row, "preset_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode preset_id: {e}")))?;
    let creator_id: String = sqlx::Row::try_get(&row, "creator_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode creator_id: {e}")))?;
    let work_id: Option<String> = sqlx::Row::try_get(&row, "work_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode work_id: {e}")))?;

    if preset_id != REVIEW_PRESET_ID {
        // Not a review schedule — no-op.
        return Ok(0);
    }

    let Some(work_id) = work_id else {
        tracing::warn!(
            schedule_id,
            "review-findings: schedule has NULL work_id; skipping"
        );
        return Ok(0);
    };

    // Work row may be missing for malformed schedules; log + return.
    let work = match works::get_work(pool, &creator_id, &work_id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            tracing::warn!(
                schedule_id,
                work_id = %work_id,
                "review-findings: work not found; skipping"
            );
            return Ok(0);
        }
        Err(e) => return Err(AutoChainError::from(e)),
    };

    // Derive chapter context from Work's current_chapter (V1.38 §4.5.2).
    // `current_chapter` is 0 until first finalize; treat 0 as Work-level.
    let chapter: Option<i64> = if work.current_chapter > 0 {
        Some(i64::from(work.current_chapter))
    } else {
        None
    };

    // V1.48 P0 T2: parse `Works/<work_ref>/Logs/review/review-report.md`
    // when a workspace_dir is available. Spec
    // (`.mstar/archived/knowledge/novel-findings-maturity.md` §1.2) — parsed
    // findings persist with their `kind` / `severity` / `body` / optional
    // `rule_suggestion`. Any failure (missing file, read error, parse error,
    // zero parsed findings) falls through to the V1.47 placeholder synthesis
    // below with a `tracing::warn!` per spec §1.3.
    let work_ref_or_id = work.work_ref.as_deref().unwrap_or(&work_id).to_string();

    if let Some(ws_dir) = workspace_dir {
        if let Some(count) = try_persist_parsed_findings(
            pool,
            ws_dir,
            &work_ref_or_id,
            &work_id,
            &creator_id,
            chapter,
            schedule_id,
        )
        .await?
        {
            return Ok(count);
        }
    }

    // ── V1.47 placeholder synthesis (fallback per spec §1.3) ───────────────
    persist_placeholder_finding(
        pool,
        &work,
        &work_id,
        &creator_id,
        chapter,
        &work_ref_or_id,
        schedule_id,
    )
    .await
}

/// Persist the V1.47 single placeholder finding (spec §8.2 safe defaults).
///
/// Used as the documented fallback whenever the parsed path does not yield a
/// persisted row (missing report, parse failure, zero findings, all-rows
/// conflict, or `workspace_dir=None`).
async fn persist_placeholder_finding(
    pool: &SqlitePool,
    work: &WorkRecord,
    work_id: &str,
    creator_id: &str,
    chapter: Option<i64>,
    work_ref_or_id: &str,
    schedule_id: &str,
) -> Result<usize, AutoChainError> {
    // Synthesize the minimum viable review finding per spec §8.2.
    let chapter_ctx = chapter.map_or_else(|| "work-level".to_string(), |c| format!("chapter {c}"));
    let title = format!("Review pass completed ({chapter_ctx})");
    let description = format!(
        "Automated review pass for Work '{}' ({}) — {chapter_ctx}.\n\
         The full review report is written under Works/{}/Logs/review/.\n\
         \n\
         (V1.47 P0: synthesized finding — the LLM review output is not parsed\n\
         at the supervisor layer in this slice. A follow-up will parse the\n\
         structured review artifact for richer kind/severity/rule_suggestion.)",
        work.title, work_ref_or_id, work_ref_or_id,
    );

    let verdict = ReviewVerdictFinding {
        work_id: work_id.to_string(),
        chapter,
        // Safe defaults per spec §8.2 + §2.1; the synthesized finding is
        // intentionally non-disruptive (`info` severity, `none` executor).
        severity: "info".to_string(),
        title,
        description,
        target_executor: "none".to_string(),
        creator_id: creator_id.to_string(),
        kind: "craft".to_string(),
        // Optional — no rule suggestion in the synthesized path.
        rule_suggestion: None,
        // V1.47 P0 fix (qc1 W-2): pass the originating schedule_id so the
        // INSERT is idempotent — a second terminal transition for the same
        // review schedule is a no-op (partial unique index
        // `findings_unique_review_per_chapter`).
        source_schedule_id: Some(schedule_id.to_string()),
    };

    match findings::create_finding_from_review(pool, &verdict).await {
        Ok(finding_id) => {
            tracing::info!(
                schedule_id,
                work_id,
                finding_id = %finding_id,
                "review-findings: persisted finding for review pass"
            );
            Ok(1)
        }
        Err(e) => {
            // R-V139P1-W-6: log + propagate so the caller can record the
            // failure without blocking the terminal transition.
            tracing::warn!(
                schedule_id,
                work_id,
                error = %e,
                "review-findings: failed to persist finding"
            );
            Err(AutoChainError::from(e))
        }
    }
}

/// Try to read + parse `Works/<work_ref>/Logs/review/review-report.md` and
/// persist one finding per parsed row (V1.48 P0 T2).
///
/// Returns:
/// - `Ok(Some(count))` — parsed findings existed and `count` rows were
///   persisted. The caller returns this count directly.
/// - `Ok(None)` — either `workspace_dir` was not provided, OR the report
///   was missing/unreadable/unparseable, OR it parsed zero findings, OR
///   the parsed rows all hit the idempotent conflict and zero were inserted.
///   In every `None` case the caller falls back to the V1.47 placeholder
///   synthesis path (spec §1.3).
///
/// Each non-`None` failure branch emits a `tracing::warn!` with the
/// schedule/work context so operators can see the degrade. V1.48 P0-fix1
/// (qc3 W-3): every fallback `warn!` includes `chapter` per spec §1.3
/// (operator-debugging field for chapter-scoped review passes).
//
// `too_many_lines`: this is a single match dispatching one documented fallback
// `warn!` arm per `ReportLoadError` variant (spec §1.3 mandates each branch).
// Splitting the arms into helpers would hide the linear fallback contract
// without reducing real complexity. Mirrors the `work_chapters` reconcile and
// `rules_runtime::handle_rules_reset` precedent.
#[allow(clippy::too_many_lines)]
async fn try_persist_parsed_findings(
    pool: &SqlitePool,
    workspace_dir: &std::path::Path,
    work_ref: &str,
    work_id: &str,
    creator_id: &str,
    chapter: Option<i64>,
    schedule_id: &str,
) -> Result<Option<usize>, AutoChainError> {
    match load_and_parse_review_report(workspace_dir, work_ref) {
        Ok(parsed) if !parsed.findings.is_empty() => {
            let count =
                persist_parsed_findings(pool, &parsed, work_id, creator_id, chapter, schedule_id)
                    .await?;
            if count > 0 {
                tracing::info!(
                    schedule_id,
                    work_id,
                    parsed_count = count,
                    "review-findings: persisted parsed findings from review-report.md"
                );
                return Ok(Some(count));
            }
            // Parsed rows existed but none persisted (e.g. idempotent
            // conflict). Fall through to placeholder synthesis so the
            // spec §8.2 "≥1 finding per review pass" guarantee holds.
            tracing::debug!(
                schedule_id,
                work_id,
                "review-findings: parsed {} rows but 0 persisted; falling back to placeholder",
                parsed.findings.len()
            );
            Ok(None)
        }
        Ok(_) => {
            tracing::warn!(
                schedule_id,
                work_id,
                work_ref,
                chapter = ?chapter,
                "review-findings: review-report.md parsed but contained no issues; \
                 falling back to V1.47 placeholder synthesis"
            );
            Ok(None)
        }
        Err(ReportLoadError::Missing) => {
            tracing::warn!(
                schedule_id,
                work_id,
                work_ref,
                chapter = ?chapter,
                "review-findings: review-report.md not found; \
                 falling back to V1.47 placeholder synthesis"
            );
            Ok(None)
        }
        Err(ReportLoadError::Read(ref path, ref e)) => {
            tracing::warn!(
                schedule_id,
                work_id,
                chapter = ?chapter,
                path = %path.display(),
                error = %e,
                "review-findings: failed to read review-report.md; \
                 falling back to V1.47 placeholder synthesis"
            );
            Ok(None)
        }
        Err(ReportLoadError::Parse(ref reason)) => {
            tracing::warn!(
                schedule_id,
                work_id,
                work_ref,
                chapter = ?chapter,
                parse_error = %reason,
                "review-findings: review-report.md failed to parse; \
                 falling back to V1.47 placeholder synthesis"
            );
            Ok(None)
        }
        // V1.48 P0-fix1 (qc3 W-1): bounded-read cap exceeded. `chapter`
        // included per spec §1.3 (qc3 W-3 adds it to every fallback arm).
        Err(ReportLoadError::TooLarge {
            size_bytes,
            cap_bytes,
        }) => {
            tracing::warn!(
                schedule_id,
                work_id,
                work_ref,
                chapter = ?chapter,
                size_bytes,
                cap_bytes,
                "review-findings: review-report.md exceeds bounded-read cap; \
                 falling back to V1.47 placeholder synthesis"
            );
            Ok(None)
        }
        // V1.49 P3 (R-V148P0-W1): resolved path escaped Works/<work_ref>/.
        // qc3 W-3: `chapter` included on every fallback arm.
        Err(ReportLoadError::PathEscape {
            work_ref: ref escaped,
        }) => {
            tracing::warn!(
                schedule_id,
                work_id,
                work_ref,
                escaped_work_ref = %escaped,
                chapter = ?chapter,
                "review-findings: review-report.md path escapes Works/<work_ref>/ \
                 (traversal or symlink); rejecting before read; falling back to \
                 V1.47 placeholder synthesis"
            );
            Ok(None)
        }
    }
}

/// Failures that can occur while loading + parsing `review-report.md`.
///
/// Used by [`persist_review_findings_for_schedule`] to emit the right
/// `tracing::warn!` shape per `.mstar/archived/knowledge/novel-findings-maturity.md`
/// §1.3 (each branch is a documented fallback trigger).
#[derive(Debug)]
enum ReportLoadError {
    /// File does not exist at the resolved path.
    Missing,
    /// File exists but could not be read.
    Read(std::path::PathBuf, std::io::Error),
    /// File was read but the parser rejected it.
    Parse(String),
    /// V1.48 P0-fix1 (qc3 W-1): file size exceeds the bounded-read cap.
    /// Falling back to placeholder synthesis is the documented safe degrade.
    TooLarge { size_bytes: u64, cap_bytes: u64 },
    /// V1.49 P3 (R-V148P0-W1): the resolved report path escapes the
    /// `Works/<work_ref>/` subtree (path traversal in `work_ref` or symlink
    /// escape). Rejecting before the read is the documented safe degrade.
    PathEscape { work_ref: String },
}

/// Upper bound on how many bytes of `review-report.md` the supervisor will
/// buffer into memory on the `on_schedule_terminal` hot path.
///
/// V1.48 P0-fix1 (qc3 W-1): a malformed or runaway LLM report must not
/// consume unbounded memory on the producer path. The cap is sized for
/// ~50 findings × ~2 KiB of prose (≈ 100 KiB typical) with a 2.5× headroom.
/// The downstream [`persist_parsed_findings`] truncates each finding body to
/// 2 000 chars anyway, so the persisted footprint stays bounded even when
/// this cap is reached. If a legitimate report ever exceeds this, operators
/// see a `tracing::warn!` with `size_bytes` / `cap_bytes` and the producer
/// degrades to the V1.47 placeholder.
const MAX_REVIEW_REPORT_BYTES: u64 = 256 * 1024;

/// Resolve `<workspace_dir>/Works/<work_ref>/Logs/review/review-report.md`,
/// read it, and parse it via [`crate::review_report::parse_review_report`].
///
/// Hermetic-friendly: takes an explicit `workspace_dir` so callers (and tests)
/// control the FS root. The path layout is provided by `nexus-home-layout`
/// (`work_logs_subdir`).
///
/// V1.48 P0-fix1 (qc3 W-1): the read is bounded by [`MAX_REVIEW_REPORT_BYTES`].
/// `metadata()` is used for both missing-detection and the size check, so the
/// happy path is two syscalls (stat + read). This also incidentally closes
/// qc3 S-2 (drops the redundant `exists()` pre-check that previously made the
/// path 3 syscalls).
fn load_and_parse_review_report(
    workspace_dir: &std::path::Path,
    work_ref: &str,
) -> Result<crate::review_report::ParsedReviewReport, ReportLoadError> {
    // V1.49 P3 (R-V148P0-W1): defense-in-depth path guard. Reject traversal
    // segments in `work_ref` BEFORE constructing the path, then verify (after
    // confirming the file exists) that the resolved report stays under the
    // canonicalized `Works/<work_ref>/` subtree. This blocks path traversal
    // and symlink escape before P1/P2 prompt-injection surfaces grow.
    if work_ref.is_empty()
        || work_ref.contains("..")
        || work_ref.contains('/')
        || work_ref.contains('\\')
        || work_ref.contains('\0')
    {
        return Err(ReportLoadError::PathEscape {
            work_ref: work_ref.to_string(),
        });
    }

    let review_dir = nexus_home_layout::work_logs_subdir(workspace_dir, work_ref, "review");
    let report_path = review_dir.join("review-report.md");
    let metadata = match std::fs::metadata(&report_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ReportLoadError::Missing);
        }
        Err(e) => return Err(ReportLoadError::Read(report_path, e)),
    };

    // Canonical guard: the resolved report must live under the canonicalized
    // `Works/<work_ref>/` subtree. Catches symlink escape (e.g. `Works/<work_ref>`
    // itself being a symlink pointing outside the workspace). `canonicalize`
    // succeeds here because `metadata` already confirmed the file exists.
    let canonical_work_root = workspace_dir
        .canonicalize()
        .unwrap_or_else(|_| workspace_dir.to_path_buf())
        .join("Works")
        .join(work_ref);
    if !report_path
        .canonicalize()
        .is_ok_and(|canonical_report| canonical_report.starts_with(&canonical_work_root))
    {
        return Err(ReportLoadError::PathEscape {
            work_ref: work_ref.to_string(),
        });
    }

    let size_bytes = metadata.len();
    if size_bytes > MAX_REVIEW_REPORT_BYTES {
        return Err(ReportLoadError::TooLarge {
            size_bytes,
            cap_bytes: MAX_REVIEW_REPORT_BYTES,
        });
    }
    let content = match std::fs::read_to_string(&report_path) {
        Ok(c) => c,
        Err(e) => return Err(ReportLoadError::Read(report_path, e)),
    };
    crate::review_report::parse_review_report(&content)
        .map_err(|e| ReportLoadError::Parse(e.to_string()))
}

/// Persist each parsed finding as its own row via the from-review DAO.
///
/// Each row inherits a **per-finding-indexed** `source_schedule_id` of the
/// form `<schedule_id>#<idx>` so the partial unique index
/// `findings_unique_review_per_chapter` keeps the insert idempotent per
/// `(work_id, chapter, source_schedule_id)` while still allowing multiple
/// distinct findings from one review report. A retry that re-parses the
/// same report hits the same indices and is a no-op; the V1.47 placeholder
/// path uses the bare `schedule_id` (no suffix), so the two paths never
/// collide on the index.
///
/// V1.48 P0-fix1 (qc3 W-2): all parsed rows for one review report are
/// persisted inside a **single `SQLite` transaction** (`BEGIN; …; COMMIT;`)
/// via [`findings::create_finding_from_review_tx`]. This replaces the
/// previous N-sequential-`INSERT` round-trips with one transaction boundary
/// so a 20-issue report is now one DB round-trip envelope instead of 20.
/// Idempotency semantics (`ON CONFLICT DO NOTHING` on the partial unique
/// index) are unchanged. Per-row insert failures are still logged and do not
/// abort the transaction (`SQLite` does not poison the tx on a statement-level
/// error); whatever succeeded commits at the end.
///
/// Returns the count of rows actually inserted (best-effort: rows that hit
/// the idempotent conflict are not counted).
async fn persist_parsed_findings(
    pool: &SqlitePool,
    parsed: &crate::review_report::ParsedReviewReport,
    work_id: &str,
    creator_id: &str,
    chapter: Option<i64>,
    schedule_id: &str,
) -> Result<usize, AutoChainError> {
    let mut inserted = 0usize;
    // V1.48 P0-fix1 (qc3 W-2): one transaction wraps the whole batch so N
    // parsed findings cost one DB round-trip envelope (BEGIN + N inserts via
    // the `_tx` DAO variant + COMMIT) instead of N independent round-trips.
    let mut tx = pool.begin().await.map_err(|e| {
        let db_err = nexus_local_db::LocalDbError::from(e);
        tracing::warn!(
            schedule_id,
            work_id,
            error = %db_err,
            "review-findings: failed to begin parsed-findings transaction; \
             falling back to placeholder synthesis"
        );
        AutoChainError::from(db_err)
    })?;
    tracing::debug!(
        schedule_id,
        work_id,
        finding_count = parsed.findings.len(),
        "review-findings: persisting parsed findings in single transaction"
    );
    for (idx, finding) in parsed.findings.iter().enumerate() {
        // Truncate body to a safe upper bound to avoid DB bloat. The from-review
        // DAO already enforces rule_suggestion size; the body uses the same
        // finding.description column.
        const BODY_MAX_CHARS: usize = 2_000;
        let body: String = if finding.body.len() > BODY_MAX_CHARS {
            // Collect char units to avoid splitting a UTF-8 boundary.
            finding.body.chars().take(BODY_MAX_CHARS).collect()
        } else {
            finding.body.clone()
        };

        let title_preview: String = body.chars().take(80).collect();
        // Per-finding source id — distinct from the bare `schedule_id` used
        // by the V1.47 placeholder path (so they cannot collide on the
        // partial unique index), and stable across retries (same schedule +
        // same report → same indices → same conflict resolution = no-op).
        let source_schedule_id = format!("{schedule_id}#{idx}");
        let verdict = ReviewVerdictFinding {
            work_id: work_id.to_string(),
            chapter,
            severity: finding.severity.clone(),
            title: format!("Review finding: {title_preview}"),
            description: body,
            target_executor: finding.target_executor.clone(),
            creator_id: creator_id.to_string(),
            kind: finding.kind.clone(),
            rule_suggestion: finding.rule_suggestion.clone(),
            source_schedule_id: Some(source_schedule_id),
        };
        // `_tx` variant executes against the shared transaction.
        match findings::create_finding_from_review_tx(&mut tx, &verdict).await {
            Ok(_) => inserted += 1,
            Err(e) => {
                tracing::warn!(
                    schedule_id,
                    work_id,
                    error = %e,
                    "review-findings: failed to persist one parsed finding (continuing batch)"
                );
            }
        }
    }
    tx.commit()
        .await
        .map_err(|e| AutoChainError::from(nexus_local_db::LocalDbError::from(e)))?;
    Ok(inserted)
}

/// Determine the next chain action after a schedule completes.
///
/// This is the core decision function of the auto-chain engine. It evaluates:
/// 1. Whether auto-chain is enabled for this Work
/// 2. The current FL-E stage and chapter state
/// 3. Whether more chapters remain
///
/// Returns the appropriate `ChainAction` to execute.
#[must_use]
pub fn evaluate_next_step(work: &WorkRecord) -> ChainAction {
    // If auto-chain is disabled, no automatic advancement
    if !work.auto_chain_enabled {
        return ChainAction::NoAction;
    }

    // If the auto-chain was interrupted, don't resume automatically
    if work.auto_chain_interrupted {
        return ChainAction::NoAction;
    }

    // If work is already completed, nothing to do
    if work.status == "completed" {
        return ChainAction::NoAction;
    }

    let current_stage = work.current_stage.as_str();
    let current_idx = stage_index(current_stage).unwrap_or(0);

    // After persist (last FL-E stage): check for chapter loop or work completion
    if current_stage == "persist" && work.stage_status == "complete" {
        return evaluate_after_persist(work);
    }

    // After any other stage completes: advance to the next stage
    if work.stage_status == "complete" && current_idx < FL_E_STAGES.len() - 1 {
        let next_idx = current_idx + 1;
        if let Some(&next_stage) = FL_E_STAGES.get(next_idx) {
            return ChainAction::AdvanceStage {
                work_id: work.work_id.clone(),
                next_stage: next_stage.to_string(),
            };
        }
    }

    // Intake stage with status "skipped" — advance to research
    if current_stage == "intake" && work.stage_status == "skipped" {
        return ChainAction::AdvanceStage {
            work_id: work.work_id.clone(),
            next_stage: "research".to_string(),
        };
    }

    ChainAction::NoAction
}

/// Evaluate what happens after the persist stage completes.
///
/// This handles the chapter outer loop:
/// - If more chapters remain → start produce for chapter N+1
/// - If all chapters done → mark work as completed
///
/// For single-volume Works (the common case), uses the flat `current_chapter`
/// comparison. For multi-volume Works, callers should use
/// [`evaluate_after_persist_volume_aware`] instead.
fn evaluate_after_persist(work: &WorkRecord) -> ChainAction {
    let total_chapters = work.total_planned_chapters.unwrap_or(0);
    let current_chapter = work.current_chapter;

    if total_chapters <= 0 {
        // No chapter tracking — single-pass work, mark complete
        return ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        };
    }

    // Check if there are more chapters to process
    // current_chapter is the latest finalized chapter number
    if current_chapter < total_chapters {
        let next_chapter = current_chapter + 1;
        ChainAction::NextChapter {
            work_id: work.work_id.clone(),
            next_chapter,
            next_volume: 1, // V1.42: single-volume path defaults to 1
        }
    } else {
        // All chapters finalized
        ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        }
    }
}

/// V1.42 volume-aware version of [`evaluate_after_persist`].
///
/// Queries the DB for the next non-finalized chapter across all volumes.
/// Falls back to the flat `evaluate_after_persist` logic if the volume-aware
/// query returns `None` (e.g. all chapters finalized).
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the DB query fails.
pub async fn evaluate_after_persist_volume_aware(
    pool: &SqlitePool,
    work: &WorkRecord,
) -> Result<ChainAction, AutoChainError> {
    let total_chapters = work.total_planned_chapters.unwrap_or(0);

    if total_chapters <= 0 {
        return Ok(ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        });
    }

    // Try volume-aware next chapter selection
    let next =
        nexus_local_db::work_chapters::next_chapter_volume_aware(pool, &work.work_id).await?;

    match next {
        Some((volume, chapter)) => Ok(ChainAction::NextChapter {
            work_id: work.work_id.clone(),
            next_chapter: chapter,
            next_volume: volume,
        }),
        None => Ok(ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        }),
    }
}

/// Build the schedule request for an auto-chain step (stage advance or next chapter).
///
/// Constructs a correctly-shaped `AddScheduleRequest` using the shared
/// [`stage_gates::build_schedule_for_stage`] facade.
///
/// V1.44 P2 (F-004): `volume` is threaded through to `WorkFields` so the
/// `novel-writing` preset input includes a `volume` template var for
/// cross-volume context preservation.
///
/// V1.48 P1 (overlay §2 Consumer): `open_findings_block` is threaded
/// through to `WorkFields` so the `novel-writing` preset input includes
/// an `open_findings_block` template var. The caller
/// ([`enqueue_auto_chain_schedule`]) computes the block via
/// [`crate::findings_block::build_open_findings_block`] from the
/// chapter-scoped DAO query. Pass `None` when the stage is not `produce`
/// or no chapter is selected; the preset's `{{#if open_findings_block}}`
/// guard omits the section in that case.
#[allow(clippy::missing_panics_doc)] // panic only on invalid stage names, which we validate
pub fn build_auto_chain_schedule(
    stage: &str,
    creator_id: &str,
    work: &WorkRecord,
    chapter: Option<i32>,
    volume: Option<i32>,
    open_findings_block: Option<String>,
) -> Option<AddScheduleRequest> {
    let work_ref = work.work_ref.clone();
    let chapter_label = chapter.map(stage_gates::chapter_label);

    // Fix W-2: when the stage is produce (following research), include the
    // research artifacts directory in the preset input so produce can see
    // research-derived material (AC2, AC3).
    let research_artifacts_dir = if stage == "produce" {
        work.driver_schedule_id
            .as_ref()
            .map(|sid| format!(".nexus42/references/{sid}/"))
    } else {
        None
    };

    let fields = WorkFields {
        work_id: work.work_id.clone(),
        fl_e_stage: stage.to_string(),
        creative_brief: work.creative_brief.clone().unwrap_or_default(),
        inspiration_log: work.inspiration_log.clone(),
        work_ref,
        chapter,
        chapter_label,
        outline_path: None,
        body_path: None,
        slug: None,
        research_artifacts_dir,
        workspace_dir: None,
        world_kb_block: None,
        open_findings_block,
        world_id: work.world_id.clone(),
        volume,
    };

    stage_gates::build_schedule_for_stage(stage, creator_id, &fields)
}

/// Update the Work checkpoint after an auto-chain step is enqueued.
///
/// Sets the new `driver_schedule_id` and resets `auto_chain_interrupted`.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn update_checkpoint(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    new_stage: &str,
    driver_schedule_id: Option<&str>,
    chapter: Option<i32>,
) -> Result<WorkRecord, AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    let patch = WorkPatch {
        current_stage: Some(new_stage.to_string()),
        stage_status: Some("active".to_string()),
        driver_schedule_id: driver_schedule_id.map(|s| Some(s.to_string())),
        auto_chain_interrupted: Some(false),
        ..Default::default()
    };

    if let Some(ch) = chapter {
        // For chapter loop, the current_chapter update happens at finalize time
        // (via novel_chapter_transition). We don't advance it here.
        // But we need to set the stage to "produce" for the new chapter.
        let _ = ch; // chapter is used for the schedule input, not the patch
    }

    works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)
}

/// Mark a Work as completed.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn mark_work_completed(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<WorkRecord, AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    // Step 1: DB patch — status + novel_completion_status + completion_locked_at
    let patch = WorkPatch {
        status: Some("completed".to_string()),
        current_stage: Some("persist".to_string()),
        stage_status: Some("complete".to_string()),
        driver_schedule_id: Some(None), // clear driver
        auto_chain_interrupted: Some(false),
        novel_completion_status: Some(Some("finalize_complete".to_string())),
        completion_locked_at: Some(Some(now.clone())),
        ..Default::default()
    };

    let updated = works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)?;

    // Step 1.5: Update pool entry to `completed` (DF-61 §5.4).
    // The pool row may not exist if the Work was created outside the
    // selection pool (e.g., `creator run start`).
    match novel_pool_entries::mark_pool_entry_completed_for_work(pool, creator_id, work_id).await {
        Ok(()) => {}
        Err(e) => {
            // Pool update failed — clear completion_locked_at so the
            // supervisor retries on the next tick (qc2 W-03, qc3 F-003).
            tracing::error!(
                target: "novel.completion",
                work_id = %work_id,
                creator_id = %creator_id,
                error = %e,
                "mark_work_completed: pool entry update FAILED — \
                 clearing completion_locked_at for supervisor retry"
            );
            let clear_lock = WorkPatch {
                completion_locked_at: Some(None),
                ..Default::default()
            };
            let retry_now = chrono::Utc::now().to_rfc3339();
            if let Err(clear_err) =
                works::patch_work(pool, creator_id, work_id, &clear_lock, &retry_now).await
            {
                tracing::error!(
                    target: "novel.completion",
                    work_id = %work_id,
                    error = %clear_err,
                    "mark_work_completed: failed to clear completion_locked_at after pool update failure"
                );
            }
        }
    }

    // Step 2: Write completion-lock file (best-effort; non-blocking for Work completion)
    if let Some(ref _work_ref) = updated.work_ref {
        let lock = CompletionLock {
            schema_version: 1,
            work_id: work_id.to_string(),
            locked_at: now.clone(),
            reason: "completion".to_string(),
        };
        // We don't have workspace_dir here — the caller (supervisor) should
        // write the lock file after calling this function if they have the path.
        // For now, we log an info-level note. The actual file I/O is done by
        // the supervisor or CLI layer that has access to the workspace dir.
        tracing::info!(
            target: "novel.completion",
            work_id = %work_id,
            creator_id = %creator_id,
            completion_locked_at = %now,
            work_ref = ?updated.work_ref,
            "mark_work_completed: DB columns set; completion-lock file \
             should be written by caller"
        );
        let _ = lock; // used by caller
    }

    Ok(updated)
}

/// Write the completion-lock file for a completed Work (DF-60 §3).
///
/// Call this after `mark_work_completed` succeeds, providing the workspace
/// directory and the Work record (for `work_ref`). This is separated from
/// `mark_work_completed` because the supervisor does not have access to the
/// workspace directory — the daemon layer calls this function.
///
/// # Errors
///
/// Returns `std::io::Error` if the file cannot be written.
pub fn write_completion_lock_for_work(
    workspace_dir: &std::path::Path,
    work: &WorkRecord,
    locked_at: &str,
) -> Result<(), std::io::Error> {
    let work_ref = work.work_ref.as_deref().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "work {} has no work_ref; cannot write completion-lock",
                work.work_id
            ),
        )
    })?;

    let lock = CompletionLock {
        schema_version: 1,
        work_id: work.work_id.clone(),
        locked_at: locked_at.to_string(),
        reason: "completion".to_string(),
    };

    completion_lock::write_completion_lock(workspace_dir, work_ref, &lock)
}

/// Clear the `driver_schedule_id` on a Work (e.g., when schedule completes or is cancelled).
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn clear_driver(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<(), AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    let patch = WorkPatch {
        driver_schedule_id: Some(None),
        ..Default::default()
    };

    works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)?;

    Ok(())
}

/// Set the `driver_schedule_id` on a Work and mark the stage as active.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn set_driver(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    schedule_id: &str,
    stage: &str,
) -> Result<(), AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    let patch = WorkPatch {
        current_stage: Some(stage.to_string()),
        stage_status: Some("active".to_string()),
        driver_schedule_id: Some(Some(schedule_id.to_string())),
        auto_chain_interrupted: Some(false),
        ..Default::default()
    };

    works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)?;

    Ok(())
}

// Fix A (W-A): Shared enqueue logic — single source of truth for ACH schedule
// ID minting, pending INSERT, and set_driver. Used by both the supervisor
// terminal hook and the boot recovery path to eliminate duplication.

// V1.48 P1 (overlay §2 Consumer): render the open-findings prompt block
// for the produce stage with a selected chapter. Returns `None` when the
// stage is not `produce`, no chapter is selected, no actionable findings
// exist, or the DAO errors (best-effort: logs and proceeds without the
// block so the auto-chain step is not blocked by a findings-fetch failure).
//
// V1.49 F6: the DAO (`list_open_findings_for_chapter`) now returns rows
// with `status IN ('open', 'triaged')` per `findings-lifecycle.md` §2.2.
// The canonical actionable set lives in
// `nexus_local_db::findings::ACTIONABLE_FINDING_STATUSES` (mirrored by
// `crate::findings_block::ACTIONABLE_FINDING_STATUSES`); this call site
// does not re-filter — the DAO is the source of truth.
async fn compute_open_findings_block_for_produce(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    stage: &str,
    chapter: Option<i32>,
) -> Option<String> {
    if stage != "produce" {
        return None;
    }
    let ch = chapter?;
    let findings = match nexus_local_db::findings::list_open_findings_for_chapter(
        pool,
        creator_id,
        work_id,
        i64::from(ch),
    )
    .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(
                target: "fl_e.auto_chain",
                work_id = %work_id,
                chapter = ch,
                error = %e,
                "Failed to fetch open findings for prompt block; proceeding without block"
            );
            return None;
        }
    };
    let label = stage_gates::chapter_label(ch);
    let block = crate::findings_block::build_open_findings_block(&findings, &label);
    if block.is_empty() {
        None
    } else {
        Some(block)
    }
}

/// V1.49 P1 — Foreshadowing promotion hook (narrative-indexes overlay §4).
///
/// Called from the supervisor's `on_schedule_terminal(Completed)` for
/// `novel-writing` schedules. After a produce run writes one or more chapter
/// outlines under `Works/<work_ref>/Outlines/chapters/`, this hook extracts
/// every `## Foreshadowing Touched (F###)` section and promotes the inline
/// `F###` declarations into `Works/<work_ref>/Outlines/foreshadowing.md` via
/// [`crate::narrative_index::promote_outline_to_index`].
///
/// # Behavior
///
/// 1. Loads the schedule row to read `preset_id`, `work_id`, `creator_id`.
/// 2. Returns `Ok(0)` early when the preset is not `novel-writing`.
/// 3. Loads the Work record for `work_ref`.
/// 4. When `workspace_dir` is `Some`, scans every `Outlines/chapters/*.md`
///    outline, extracts its foreshadowing section, and promotes it. Promotion
///    is idempotent, so re-scanning all outlines on every produce run is safe.
/// 5. When `workspace_dir` is `None` (hermetic DB-only tests) or no outlines
///    declare foreshadowing, this is a no-op (`Ok(0)`).
///
/// Best-effort + non-blocking by contract: the caller logs any `Err` and does
/// NOT fail the terminal transition (mirrors `persist_review_findings_for_schedule`).
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the schedule/Work lookup fails.
/// Promotion-internal errors (e.g. conflicting-description duplicate) are
/// logged at `warn!` and counted as zero for that outline so one bad outline
/// does not abort the rest.
pub async fn promote_foreshadowing_for_schedule(
    pool: &SqlitePool,
    schedule_id: &str,
    workspace_dir: Option<&std::path::Path>,
) -> Result<usize, AutoChainError> {
    use crate::preset_ids::NOVEL_WRITING_PRESET_ID;

    // SAFETY: dynamic SQL — single-row schedule lookup by PK (nullable work_id).
    let row = sqlx::query(
        "SELECT preset_id, work_id, creator_id
         FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    let Some(row) = row else {
        tracing::debug!(
            schedule_id,
            "foreshadowing-promote: schedule row not found; skipping"
        );
        return Ok(0);
    };

    let preset_id: String = sqlx::Row::try_get(&row, "preset_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode preset_id: {e}")))?;
    let work_id: Option<String> = sqlx::Row::try_get(&row, "work_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode work_id: {e}")))?;
    let creator_id: String = sqlx::Row::try_get(&row, "creator_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode creator_id: {e}")))?;

    if preset_id != NOVEL_WRITING_PRESET_ID {
        return Ok(0);
    }
    let Some(work_id) = work_id else {
        tracing::warn!(
            schedule_id,
            "foreshadowing-promote: schedule has NULL work_id; skipping"
        );
        return Ok(0);
    };
    let Some(ws_dir) = workspace_dir else {
        // Hermetic DB-only tests / no workspace bound — nothing to promote.
        tracing::debug!(
            schedule_id,
            "foreshadowing-promote: no workspace_dir; skipping"
        );
        return Ok(0);
    };

    let work = match works::get_work(pool, &creator_id, &work_id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            tracing::warn!(
                schedule_id,
                work_id = %work_id,
                "foreshadowing-promote: work not found; skipping"
            );
            return Ok(0);
        }
        Err(e) => return Err(AutoChainError::from(e)),
    };
    let Some(work_ref) = work.work_ref.as_deref() else {
        tracing::warn!(
            schedule_id,
            work_id = %work_id,
            "foreshadowing-promote: work has no work_ref; skipping"
        );
        return Ok(0);
    };

    let work_dir = ws_dir.join("Works").join(work_ref);
    let outlines_chapters = work_dir.join("Outlines").join("chapters");
    if !outlines_chapters.is_dir() {
        tracing::debug!(
            schedule_id,
            work_ref,
            "foreshadowing-promote: no Outlines/chapters/ dir; skipping"
        );
        return Ok(0);
    }

    promote_outlines_in(&work_dir, &outlines_chapters, schedule_id, work_ref)
}

/// Scan every `*-outline.md` in `outlines_dir`, extract its foreshadowing
/// section, and promote it into `work_dir/Outlines/foreshadowing.md`.
///
/// Returns the total count of newly-allocated `F###` ids across all outlines.
/// Per-outline promotion errors (e.g. conflicting-description duplicate) are
/// logged at `warn!` and counted as zero so one bad outline does not abort the
/// rest. Outline filenames are sorted for reproducible promotion order.
fn promote_outlines_in(
    work_dir: &std::path::Path,
    outlines_dir: &std::path::Path,
    schedule_id: &str,
    work_ref: &str,
) -> Result<usize, AutoChainError> {
    let mut entries: Vec<String> = std::fs::read_dir(outlines_dir)
        .map_err(|e| AutoChainError::InvalidState(format!("read {}: {e}", outlines_dir.display())))?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_string_lossy().to_string();
            (p.is_file() && name.ends_with("-outline.md")).then_some(name)
        })
        .collect();
    entries.sort();

    let mut total = 0usize;
    for name in &entries {
        let path = outlines_dir.join(name);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(section) = crate::narrative_index::extract_foreshadowing_section(&content) else {
            continue;
        };
        match crate::narrative_index::promote_outline_to_index(work_dir, &section) {
            Ok(allocated) => {
                total += allocated.len();
                if !allocated.is_empty() {
                    tracing::info!(
                        schedule_id,
                        work_ref,
                        outline = %name,
                        allocated = ?allocated,
                        "foreshadowing-promote: promoted inline F### declarations"
                    );
                }
            }
            Err(e) => {
                // Conflicting-description duplicate etc. — surface to the
                // operator without aborting the remaining outlines or the
                // terminal transition.
                tracing::warn!(
                    schedule_id,
                    work_ref,
                    outline = %name,
                    error = %e,
                    "foreshadowing-promote: promotion failed for one outline (non-fatal)"
                );
            }
        }
    }
    Ok(total)
}

/// Enqueue a new auto-chain schedule and update the Work checkpoint.
///
/// This is the single shared path for:
/// 1. Supervisor `on_schedule_terminal` → `enqueue_auto_chain_step`
/// 2. Boot `resume_auto_chain_work`
///
/// It owns: (a) schedule ID generation (`ACH{timestamp}`), (b) pending schedule
/// INSERT into `creator_schedules`, (c) `set_driver` call on the Work.
///
/// # Errors
///
/// Returns `AutoChainError::InvalidState` if no schedule mapping exists for the
/// given stage. Returns `AutoChainError::Database` if any DB operation fails.
pub async fn enqueue_auto_chain_schedule(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    stage: &str,
    chapter: Option<i32>,
    volume: Option<i32>,
    work: &WorkRecord,
) -> Result<String, AutoChainError> {
    // V1.48 P1 (overlay §2 Consumer): render the open-findings prompt
    // block when the produce stage targets a selected chapter.
    let open_findings_block =
        compute_open_findings_block_for_produce(pool, creator_id, work_id, stage, chapter).await;

    let schedule_req = build_auto_chain_schedule(
        stage,
        creator_id,
        work,
        chapter,
        volume,
        open_findings_block,
    )
    .ok_or_else(|| {
        AutoChainError::InvalidState(format!("no schedule mapping for stage '{stage}'"))
    })?;

    // Fix A: Single source of truth for ACH schedule ID format.
    // R-V139P0-W-B: append per-process monotonic counter for collision resistance.
    // Pure-timestamp IDs could collide under millisecond-granule concurrent enqueue;
    // the counter provides unique suffix without adding a new crate dependency.

    let counter = ACH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let schedule_id = format!(
        "ACH{}{:06x}",
        chrono::Utc::now().format("%Y%m%d%H%M%S%3f"),
        counter & 0x00FF_FFFF
    );
    let now_ts = chrono::Utc::now().timestamp();

    // SAFETY: dynamic SQL — auto-chain schedule insert with derived params.
    // R-V139P5-S4: read preset_version from the manifest mapping instead of
    // hard-coding 1. Keep in sync with embedded-presets/*/preset.yaml `version:`.
    let preset_version = preset_version_for_id(&schedule_req.preset_id);
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id)
           VALUES (?, ?, ?, ?, 'pending', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(&schedule_id)
    .bind(creator_id)
    .bind(&schedule_req.preset_id)
    .bind(preset_version)
    .bind(&schedule_req.label)
    .bind(now_ts)
    .bind(now_ts)
    .bind(work_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "auto-chain: failed to insert schedule");
        AutoChainError::Database(nexus_local_db::LocalDbError::from(e))
    })?;

    // Update the Work checkpoint to point at the new driver schedule.
    set_driver(pool, creator_id, work_id, &schedule_id, stage).await?;

    // V1.42 P0 (T3): Acquire runtime lock for this schedule.
    // Holder format: `daemon:schedule:<schedule_id>`.
    let holder = nexus_local_db::runtime_lock::schedule_holder(&schedule_id);
    let ttl = nexus_local_db::runtime_lock::ttl_from_env();
    match nexus_local_db::acquire_runtime_lock(
        pool, creator_id, work_id, &holder, ttl, true, // force_stale=true for daemon
    )
    .await
    {
        Ok(nexus_local_db::AcquireResult::Acquired { .. }) => {}
        Ok(nexus_local_db::AcquireResult::Locked {
            holder: existing, ..
        }) => {
            tracing::warn!(
                work_id = %work_id,
                schedule_id = %schedule_id,
                existing_holder = %existing,
                "runtime_lock: could not acquire for auto-chain (locked by another process)"
            );
            // Continue — auto-chain will skip if Work is locked at next tick.
        }
        Err(e) => {
            tracing::warn!(
                work_id = %work_id,
                schedule_id = %schedule_id,
                error = %e,
                "runtime_lock: failed to acquire for auto-chain"
            );
            // Non-fatal — the schedule was already enqueued.
        }
    }

    tracing::info!(
        work_id = %work_id,
        schedule_id = %schedule_id,
        stage = %stage,
        chapter = chapter.unwrap_or(0),
        "auto-chain: enqueued next step"
    );

    Ok(schedule_id)
}

/// R-V139P5-S4: Map `preset_id` to its embedded manifest version.
///
/// Must be kept in sync with `embedded-presets/*/preset.yaml` `version:` field.
/// Returns 1 as fallback for unknown preset IDs.
///
/// R-V139P5-W-4: version policy — bump the version number in both this mapping
/// AND the corresponding `preset.yaml` whenever the state machine undergoes a
/// breaking change (state additions/removals, transition edge changes, prompt
/// template modifications that alter the output contract). Non-breaking changes
/// (comments, optional fields) may keep the same version. The version is stored
/// in `creator_schedules` at enqueue time and used by the loader for compat checks.
//
// `match_same_arms`: the `novel-brainstorm | novel-write` arm intentionally
// shares its body (`1`) with the `_` catch-all. The explicit arms exist for
// discoverability (R-V150P1CRONBW-05 / qc3 W-003): a maintainer scanning this
// map sees the cron-triggered presets and knows to bump them in lockstep with
// their `preset.yaml` on a breaking change. The
// `preset_version_mapping_matches_yaml_includes_cron_presets` test enforces
// the sync, so the named arms are documentation, not a behavioural fork.
#[allow(clippy::match_same_arms)]
fn preset_version_for_id(preset_id: &str) -> i64 {
    match preset_id {
        // V1.48 P1: bumped 7 → 8 — added `open_findings_block` template var
        // to outline_chapter and draft_chapter prompt contracts (new prompt
        // input, not a breaking state-machine change, but versioned up so
        // pre-V1.48 schedules are correctly identified).
        "novel-writing" => 9,
        "research" => 2,
        "novel-review-master" => 3,
        "kb-extract" => 3,
        // V1.50 T-A P1 (R-V150P1CRONBW-05 / qc3 W-003): explicit arms for the
        // cron-triggered presets so the evaluator never silently stamps a
        // stale `preset_version` through the `_` fallback. `novel-brainstorm`
        // ships at v1 (embedded-presets/novel-brainstorm/preset.yaml).
        // `novel-write` ships at v1 (embedded-presets/novel-write/preset.yaml);
        // R-V150P1CRONBW-01 is closed in V1.51 T-A P2. Bump in lockstep with
        // the preset's `version:` field on any breaking change
        // (R-V139P5-W-4 version policy); the
        // `preset_version_mapping_matches_yaml_includes_cron_presets` test
        // enforces the sync for both presets.
        "novel-brainstorm" | "novel-write" => 1,
        // V1.47: `novel-chapter-review` replaces `reflection-loop` (renamed
        // per compass §0.1 #6). Bumped to version 1 (was already 1 as
        // `reflection-loop`); the state-machine contract is intentionally new
        // (load_chapter → review → done) but ships at v1 because no prior
        // consumer depends on the old `reflection-loop` version.
        // All other presets default to version 1
        _ => 1,
    }
}

/// Enqueue a `novel-review-master` preset run for a Work whose findings have
/// passed the master-decision SLA (V1.39 P4 T4).
///
/// This is the auto-enqueue half of the stale-findings watcher. It is
/// **only** called by the daemon's stale-findings sweep when the Work has
/// `auto_review_master_on_timeout = true`. The flag default is `false`, so
/// no schedule is created without explicit opt-in.
///
/// Unlike [`enqueue_auto_chain_schedule`], this does not touch the Work's
/// `driver_schedule_id` — `novel-review-master` is an out-of-band review
/// preset and the Work's FL-E driver is unrelated.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the schedule INSERT fails.
pub async fn enqueue_review_master_schedule(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<String, AutoChainError> {
    // R-V147P0-05 (hotfix H-1): append a per-process monotonic counter suffix
    // (mirrors `ACH_COUNTER` / R-V139P0-W-B) so two enqueues in the same
    // millisecond produce distinct PKs. Without this, the
    // `master_decision_timeout::repeated_sweeps_remain_stable` test flakes
    // when both sweeps land in the same `%Y%m%d%H%M%S%3f` granule.
    let counter = RVM_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let schedule_id = format!(
        "RVM{}{:06x}",
        chrono::Utc::now().format("%Y%m%d%H%M%S%3f"),
        counter & 0x00FF_FFFF
    );
    let now_ts = chrono::Utc::now().timestamp();
    let label = format!("auto-review-master: {work_id}");
    let preset_version = preset_version_for_id("novel-review-master");

    // SAFETY: dynamic SQL — review-master schedule insert with derived params.
    // Matches the `enqueue_auto_chain_schedule` pattern (runtime sqlx is the
    // established convention in this crate; see auto_chain.rs:354-355).
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-review-master', ?, 'pending', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(&schedule_id)
    .bind(creator_id)
    .bind(preset_version)
    .bind(&label)
    .bind(now_ts)
    .bind(now_ts)
    .bind(work_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, work_id, "stale-findings: failed to insert review-master schedule");
        AutoChainError::Database(nexus_local_db::LocalDbError::from(e))
    })?;

    tracing::info!(
        work_id,
        schedule_id = %schedule_id,
        "stale-findings: enqueued novel-review-master (opt-in)"
    );

    Ok(schedule_id)
}

/// Enqueue a cron-triggered schedule (V1.50 T-A P1).
///
/// Called by [`crate::schedule::cron_supervisor::evaluate_cron_fires`] when a
/// per-Work role cron (`brainstorm` / `write`) matches the current minute and
/// passes gating + idempotency. Inserts a pending `Schedule` linked to
/// `work_id` with the given `preset_id`.
///
/// **Out-of-band**: like [`enqueue_review_master_schedule`], this does NOT
/// touch the Work's `driver_schedule_id` — a cron fire is an independent
/// production nudge, not an FL-E stage step. The existing supervisor `tick()`
/// admits the schedule; the existing executor runs the preset; the existing
/// terminal pipeline handles completion. A cron fire therefore never disrupts
/// an in-progress FL-E chain (spec `cron-staggering.md` §5: "Cron firing does
/// not bypass `creator run`").
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the schedule INSERT fails.
pub async fn enqueue_cron_schedule(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    preset_id: &str,
    role: &str,
) -> Result<String, AutoChainError> {
    // CRON prefix + timestamp + per-process counter (collision-resistant,
    // mirrors `ACH_COUNTER` / `RVM_COUNTER`). Two roles firing in the same
    // millisecond must produce distinct PKs.
    let counter = CRON_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let schedule_id = format!(
        "CRON{}{:06x}",
        chrono::Utc::now().format("%Y%m%d%H%M%S%3f"),
        counter & 0x00FF_FFFF
    );
    let now_ts = chrono::Utc::now().timestamp();
    let label = format!("cron:{role}:{work_id}");
    let preset_version = preset_version_for_id(preset_id);

    // SAFETY: dynamic SQL — cron-triggered schedule insert with derived params.
    // Matches the `enqueue_review_master_schedule` pattern (runtime sqlx is the
    // established convention in this crate; see auto_chain.rs:354-355).
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id)
           VALUES (?, ?, ?, ?, 'pending', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(&schedule_id)
    .bind(creator_id)
    .bind(preset_id)
    .bind(preset_version)
    .bind(&label)
    .bind(now_ts)
    .bind(now_ts)
    .bind(work_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, work_id, preset_id, "cron-supervisor: failed to insert cron-triggered schedule");
        AutoChainError::Database(nexus_local_db::LocalDbError::from(e))
    })?;

    tracing::info!(
        work_id,
        preset_id,
        role,
        schedule_id = %schedule_id,
        "cron-supervisor: enqueued cron-triggered schedule"
    );

    Ok(schedule_id)
}

/// Find auto-chain-enabled Works that have a `driver_schedule_id` but whose
/// schedule is no longer running (interrupted during daemon restart).
///
/// Returns works where `auto_chain_enabled = true` and `driver_schedule_id IS NOT NULL`
/// and `auto_chain_interrupted = false` and the schedule status is not 'running'.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the database query fails.
pub async fn find_resumable_works(pool: &SqlitePool) -> Result<Vec<WorkRecord>, AutoChainError> {
    // SAFETY: dynamic SQL — complex multi-table join for boot recovery.
    // V1.42 P0: skip Works with a foreign runtime_lock_holder.
    let rows = sqlx::query(&format!(
        "SELECT {0} FROM works w
         WHERE w.auto_chain_enabled = 1
           AND w.driver_schedule_id IS NOT NULL
           AND w.auto_chain_interrupted = 0
           AND w.status != 'completed'
           AND w.runtime_lock_holder IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM creator_schedules cs
               WHERE cs.schedule_id = w.driver_schedule_id
                 AND cs.status = 'running'
           )",
        works::WORKS_COLUMNS
    ))
    .fetch_all(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    Ok(rows.iter().map(works::row_to_work_record).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_at(stage: &str, status: &str, chapter: i32, total: i32) -> WorkRecord {
        WorkRecord {
            work_id: "wrk_test".to_string(),
            creator_id: "ctr_test".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Test Novel".to_string(),
            long_term_goal: "Write a novel".to_string(),
            initial_idea: "A sci-fi thriller".to_string(),
            creative_brief: None,
            intake_status: "complete".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: "2026-06-09T10:00:00Z".to_string(),
            updated_at: "2026-06-09T10:00:00Z".to_string(),
            current_stage: stage.to_string(),
            stage_status: status.to_string(),
            work_profile: Some("novel".to_string()),
            work_ref: Some("test-novel".to_string()),
            total_planned_chapters: if total > 0 { Some(total) } else { None },
            current_chapter: chapter,
            auto_chain_enabled: true,
            driver_schedule_id: Some("sch_driver_001".to_string()),
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        }
    }

    // ── evaluate_next_step tests ──────────────────────────────────────

    #[test]
    fn intake_complete_advances_to_research() {
        let work = work_at("intake", "complete", 0, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "research".to_string(),
            }
        );
    }

    #[test]
    fn research_complete_advances_to_produce() {
        let work = work_at("research", "complete", 0, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "produce".to_string(),
            }
        );
    }

    #[test]
    fn produce_complete_advances_to_review() {
        let work = work_at("produce", "complete", 1, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "review".to_string(),
            }
        );
    }

    #[test]
    fn review_complete_advances_to_persist() {
        let work = work_at("review", "complete", 1, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "persist".to_string(),
            }
        );
    }

    #[test]
    fn persist_complete_chapter1_of_3_starts_next_chapter() {
        let work = work_at("persist", "complete", 1, 3);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::NextChapter {
                work_id: "wrk_test".to_string(),
                next_chapter: 2,
                next_volume: 1,
            }
        );
    }

    #[test]
    fn persist_complete_last_chapter_marks_work_complete() {
        let work = work_at("persist", "complete", 3, 3);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::WorkComplete {
                work_id: "wrk_test".to_string(),
            }
        );
    }

    #[test]
    fn no_chapters_marks_work_complete_after_persist() {
        let work = work_at("persist", "complete", 0, 0);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::WorkComplete {
                work_id: "wrk_test".to_string(),
            }
        );
    }

    #[test]
    fn auto_chain_disabled_no_action() {
        let mut work = work_at("research", "complete", 0, 10);
        work.auto_chain_enabled = false;
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn auto_chain_interrupted_no_action() {
        let mut work = work_at("research", "complete", 0, 10);
        work.auto_chain_interrupted = true;
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn work_already_completed_no_action() {
        let mut work = work_at("persist", "complete", 10, 10);
        work.status = "completed".to_string();
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn stage_active_no_action() {
        let work = work_at("research", "active", 0, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn intake_skipped_advances_to_research() {
        let mut work = work_at("intake", "skipped", 0, 10);
        work.intake_status = "skipped".to_string();
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "research".to_string(),
            }
        );
    }

    #[test]
    fn build_auto_chain_schedule_produce_includes_chapter() {
        let work = work_at("produce", "active", 2, 5);
        let req = build_auto_chain_schedule("produce", "ctr_test", &work, Some(2), None, None)
            .expect("produce should have a preset");
        assert_eq!(req.preset_id, "novel-writing");
        let input = req.input.expect("input should be set");
        assert_eq!(input["chapter"], 2);
        assert_eq!(input["work_id"], "wrk_test");
    }

    #[test]
    fn build_auto_chain_schedule_research() {
        let work = work_at("research", "active", 0, 5);
        let req = build_auto_chain_schedule("research", "ctr_test", &work, None, None, None)
            .expect("research should have a preset");
        assert_eq!(req.preset_id, "research");
    }

    #[test]
    fn persist_complete_chapter5_of_10_starts_chapter6() {
        let work = work_at("persist", "complete", 5, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::NextChapter {
                work_id: "wrk_test".to_string(),
                next_chapter: 6,
                next_volume: 1,
            }
        );
    }

    // ── Fix A (W-A): enqueue_auto_chain_schedule shared helper ─────────

    #[tokio::test]
    async fn enqueue_helper_success_path() {
        let db = tempfile::Builder::new()
            .prefix("auto_chain_helper_")
            .suffix(".db")
            .tempfile()
            .unwrap();
        let db_path = db.path().to_path_buf();
        std::mem::forget(db);

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let work = work_at("intake", "complete", 0, 3);
        nexus_local_db::works::create_work(&pool, &work)
            .await
            .unwrap();

        let sid = enqueue_auto_chain_schedule(
            &pool, "ctr_test", "wrk_test", "research", None, None, &work,
        )
        .await
        .unwrap();

        // Verify schedule ID format
        assert!(
            sid.starts_with("ACH"),
            "schedule ID should start with ACH: {sid}"
        );

        // Verify schedule was inserted as pending
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM creator_schedules WHERE schedule_id = ?")
                .bind(&sid)
                .fetch_optional(&pool)
                .await
                .unwrap()
                .flatten();
        assert_eq!(
            status.as_deref(),
            Some("pending"),
            "schedule should be pending"
        );

        // Verify driver_schedule_id was set on the work
        let updated = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.driver_schedule_id, Some(sid));
        assert_eq!(updated.current_stage, "research");
    }

    #[tokio::test]
    async fn enqueue_helper_error_path_no_mapping() {
        let db = tempfile::Builder::new()
            .prefix("auto_chain_helper_err_")
            .suffix(".db")
            .tempfile()
            .unwrap();
        let db_path = db.path().to_path_buf();
        std::mem::forget(db);

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let mut work = work_at("intake", "complete", 0, 3);
        work.primary_preset_id = "nonexistent-preset".to_string();
        nexus_local_db::works::create_work(&pool, &work)
            .await
            .unwrap();

        let result = enqueue_auto_chain_schedule(
            &pool,
            "ctr_test",
            "wrk_test",
            "unknown_stage_xyz",
            None,
            None,
            &work,
        )
        .await;

        assert!(result.is_err(), "should fail for unknown stage");
        let err = result.unwrap_err();
        assert!(
            matches!(err, AutoChainError::InvalidState(_)),
            "should be InvalidState: {err:?}"
        );
    }

    // ── R-V147P0-05 (hotfix H-1): RVM schedule_id PK collision regression ────

    /// Regression for R-V147P0-05: two `enqueue_review_master_schedule` calls
    /// landing in the same `%Y%m%d%H%M%S%3f` millisecond granule MUST produce
    /// distinct `schedule_id` PKs. Before the fix, the second INSERT collided
    /// on the PK and surfaced as a flake in
    /// `master_decision_timeout::repeated_sweeps_remain_stable`.
    ///
    /// The per-process `RVM_COUNTER` provides the unique suffix without adding
    /// a new crate dependency (mirrors the `ACH_COUNTER` fix, R-V139P0-W-B).
    #[tokio::test]
    async fn rvm_schedule_ids_are_unique_within_same_millisecond() {
        let db = tempfile::Builder::new()
            .prefix("rvm_pk_collision_")
            .suffix(".db")
            .tempfile()
            .unwrap();
        let db_path = db.path().to_path_buf();
        std::mem::forget(db);

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let work = work_at("review", "active", 1, 3);
        nexus_local_db::works::create_work(&pool, &work)
            .await
            .unwrap();

        // Fire two enqueues back-to-back. Even if both land in the same ms
        // granule, the counter suffix must keep the PKs distinct.
        let sid_a = enqueue_review_master_schedule(&pool, "ctr_test", "wrk_test")
            .await
            .expect("first RVM enqueue must succeed");
        let sid_b = enqueue_review_master_schedule(&pool, "ctr_test", "wrk_test")
            .await
            .expect("second RVM enqueue must succeed even in the same ms");

        assert!(
            sid_a != sid_b,
            "RVM schedule ids must be distinct; got sid_a={sid_a} sid_b={sid_b}"
        );
        assert!(
            sid_a.starts_with("RVM") && sid_b.starts_with("RVM"),
            "both ids must keep the RVM prefix: sid_a={sid_a} sid_b={sid_b}"
        );

        // Both rows must be present in the table (no PK collision).
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM creator_schedules \
             WHERE preset_id = 'novel-review-master' AND work_id = ?",
        )
        .bind("wrk_test")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            n, 2,
            "both RVM schedules must be persisted without PK collision; got count={n}"
        );
    }

    // ── V1.39 P0.5 (T6): research-stage wiring integration tests ───────

    /// AC1: research preset schedule has `fl_e_stage` = "research" and includes
    /// `creative_brief` + `inspiration_log` in the seed (same surface produce reads).
    #[test]
    fn research_schedule_seed_includes_context_for_produce() {
        let work = work_at("research", "active", 0, 5);
        let req = build_auto_chain_schedule("research", "ctr_test", &work, None, None, None)
            .expect("research should have a preset");
        assert_eq!(req.preset_id, "research");

        let seed: serde_json::Value =
            serde_json::from_str(&req.seed.expect("seed must be set")).unwrap();

        // fl_e_stage annotation
        assert_eq!(seed["fl_e_stage"], "research");
        // creative_brief and inspiration_log are the shared surface
        // that produce reads — research can enrich these.
        assert!(seed["creative_brief"].is_string());
        assert!(seed["inspiration_log"].is_string());
        assert_eq!(seed["work_id"], "wrk_test");
    }

    /// AC1: produce stage seed also carries `creative_brief` and `inspiration_log`,
    /// confirming the shared context surface between research and produce.
    #[test]
    fn produce_schedule_seed_carries_research_enrichable_fields() {
        let work = work_at("produce", "active", 1, 5);
        let req = build_auto_chain_schedule("produce", "ctr_test", &work, Some(1), None, None)
            .expect("produce should have a preset");
        assert_eq!(req.preset_id, "novel-writing");

        let input = req.input.expect("input must be set");
        assert_eq!(input["fl_e_stage"], "produce");
        // These are the same fields research enriches, confirming
        // the downstream produce stage can see research-derived material.
        assert!(input.get("creative_brief").is_some());
        assert!(input.get("inspiration_log").is_some());
    }

    /// Fix W-2: produce stage input includes `research_artifacts_dir` when
    /// the work has a `driver_schedule_id` (the research schedule that just
    /// completed). This enables AC2 and AC3 (produce sees research output).
    #[test]
    fn produce_schedule_includes_research_artifacts_dir() {
        let mut work = work_at("produce", "active", 1, 5);
        // Simulate: driver_schedule_id is the research schedule that just completed
        work.driver_schedule_id = Some("ACH20260609120000000".to_string());
        let req = build_auto_chain_schedule("produce", "ctr_test", &work, Some(1), None, None)
            .expect("produce should have a preset");

        let input = req.input.expect("input must be set");
        let rad = input
            .get("research_artifacts_dir")
            .expect("Fix W-2: produce input must include research_artifacts_dir");
        assert!(
            rad.as_str().unwrap().contains("ACH20260609120000000"),
            "research_artifacts_dir should contain the driver schedule ID: {rad}"
        );
        assert!(
            rad.as_str().unwrap().starts_with(".nexus42/references/"),
            "research_artifacts_dir should use .nexus42/references/ prefix: {rad}"
        );
    }

    /// Fix W-2 (negative): research stage does NOT include `research_artifacts_dir`.
    #[test]
    fn research_schedule_does_not_include_research_artifacts_dir() {
        let mut work = work_at("research", "active", 0, 5);
        work.driver_schedule_id = Some("SCH_prev_research".to_string());
        let req = build_auto_chain_schedule("research", "ctr_test", &work, None, None, None)
            .expect("research should have a preset");

        let input = req.input.expect("input must be set");
        assert!(
            input.get("research_artifacts_dir").is_none(),
            "research stage should NOT include research_artifacts_dir"
        );
    }

    /// AC2: full chain intake→research→produce advances correctly
    /// (verifies `evaluate_next_step` for the research-middle position).
    #[test]
    fn full_chain_intake_research_produce_advances() {
        // intake complete → advance to research
        let work = work_at("intake", "complete", 0, 3);
        assert_eq!(
            evaluate_next_step(&work),
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "research".to_string(),
            }
        );

        // research complete → advance to produce
        let work = work_at("research", "complete", 0, 3);
        assert_eq!(
            evaluate_next_step(&work),
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "produce".to_string(),
            }
        );

        // produce complete (ch1 of 3) → advance to review (not NextChapter)
        let work = work_at("produce", "complete", 1, 3);
        assert_eq!(
            evaluate_next_step(&work),
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "review".to_string(),
            }
        );
    }

    /// QC1 W-2 / R-V150P1CRONBW-05: assert `preset_version_for_id` stays in
    /// sync with embedded preset.yaml version fields, extended to cover the
    /// cron-triggered presets (`novel-brainstorm`, `novel-write`).
    #[test]
    fn preset_version_mapping_matches_yaml_includes_cron_presets() {
        use crate::preset::EMBEDDED_PRESETS;

        // R-V150P1CRONBW-05 (qc3 W-003): both cron-triggered preset ids are
        // iterated here so a future `version:` bump cannot drift silently.
        // `novel-write` ships in V1.51 T-A P2 (R-V150P1CRONBW-01 closed).
        let known_ids = [
            "novel-writing",
            "research",
            "novel-review-master",
            "kb-extract",
            "novel-brainstorm",
            "novel-write",
        ];

        for preset_id in &known_ids {
            let mapping_version = preset_version_for_id(preset_id);

            // Find the embedded preset
            let yaml_path = format!("{preset_id}/preset.yaml");
            let Some(yaml_file) = EMBEDDED_PRESETS.get_file(&yaml_path) else {
                // Only `novel-write` is expected to be deferred. Any OTHER
                // missing YAML is a real drift → panic.
                assert_eq!(
                    *preset_id, "novel-write",
                    "preset.yaml missing for '{preset_id}' at '{yaml_path}'"
                );
                assert_eq!(
                    mapping_version, 1,
                    "novel-write preset.yaml is deferred (R-V150P1CRONBW-01); \
                     preset_version_for_id must return 1 until authored"
                );
                continue;
            };
            let yaml_str = std::str::from_utf8(yaml_file.contents())
                .unwrap_or_else(|e| panic!("preset.yaml for '{preset_id}' is not UTF-8: {e}"));

            // Extract version: field from YAML
            let yaml_version = yaml_str
                .lines()
                .find_map(|line| {
                    let trimmed = line.trim();
                    trimmed.strip_prefix("version:").map(|v| {
                        v.split_whitespace()
                            .next()
                            .unwrap()
                            .trim()
                            .parse::<i64>()
                            .unwrap_or_else(|_| {
                                panic!(
                                    "non-integer version in preset.yaml for '{preset_id}': '{v}'"
                                )
                            })
                    })
                })
                .unwrap_or_else(|| panic!("no 'version:' field in preset.yaml for '{preset_id}'"));

            assert_eq!(
                mapping_version, yaml_version,
                "preset_version_for_id('{preset_id}') = {mapping_version}, but preset.yaml version = {yaml_version}. \
                 Update the match arm in preset_version_for_id() to match."
            );
        }
    }

    /// R-V150P1CRONBW-05: focused regression — the shipped `novel-brainstorm`
    /// cron preset resolves to its embedded preset.yaml `version:` field.
    /// Guards against silent drift even if someone later prunes the
    /// `known_ids` array in the sync test above.
    #[test]
    fn preset_version_for_id_novel_brainstorm_resolves() {
        use crate::preset::EMBEDDED_PRESETS;

        let mapping_version = preset_version_for_id("novel-brainstorm");

        let yaml_bytes = EMBEDDED_PRESETS
            .get_file("novel-brainstorm/preset.yaml")
            .expect("novel-brainstorm preset.yaml must ship in T-A P1");
        let yaml_str = std::str::from_utf8(yaml_bytes.contents())
            .expect("novel-brainstorm preset.yaml must be UTF-8");
        let yaml_version = yaml_str
            .lines()
            .find_map(|line| {
                line.trim().strip_prefix("version:").map(|v| {
                    v.split_whitespace()
                        .next()
                        .unwrap()
                        .trim()
                        .parse::<i64>()
                        .unwrap_or_else(|_| {
                            panic!("non-integer version in novel-brainstorm preset.yaml: '{v}'")
                        })
                })
            })
            .expect("novel-brainstorm preset.yaml must declare a version: field");

        assert_eq!(
            mapping_version, yaml_version,
            "preset_version_for_id('novel-brainstorm') = {mapping_version} but embedded YAML version = {yaml_version}; \
             update the match arm in preset_version_for_id() to match."
        );
    }

    // V1.49 P3 (R-V148P0-W1) ────────────────────────────────────────────────

    /// `load_and_parse_review_report` must reject a `work_ref` that would
    /// escape `Works/<work_ref>/` (path traversal / separators) BEFORE any
    /// filesystem access, returning `ReportLoadError::PathEscape`.
    #[test]
    fn load_and_parse_review_report_rejects_path_outside_work_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // Traversal segments in work_ref — the canonical "Works/<work_ref>/../../../etc/passwd"
        // shape from the plan, expressed via the work_ref parameter.
        let traversal = load_and_parse_review_report(tmp.path(), "../../../etc/passwd");
        assert!(
            matches!(traversal, Err(ReportLoadError::PathEscape { .. })),
            "traversal work_ref must be rejected before FS access, got {traversal:?}"
        );

        // A path separator in work_ref must also be rejected.
        let with_sep = load_and_parse_review_report(tmp.path(), "foo/bar");
        assert!(
            matches!(with_sep, Err(ReportLoadError::PathEscape { .. })),
            "work_ref containing '/' must be rejected, got {with_sep:?}"
        );

        // A clean work_ref whose report is simply absent returns Missing, NOT
        // PathEscape — proving the guard does not over-reject legitimate refs.
        let missing = load_and_parse_review_report(tmp.path(), "clean-novel");
        assert!(
            matches!(missing, Err(ReportLoadError::Missing)),
            "clean work_ref with no report must be Missing, not PathEscape; got {missing:?}"
        );
    }
}
