//! `schedule::cron_supervisor` — daemon-side cron evaluator for the
//! novel-writing three-role staggering (V1.50 T-A P1).
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §4.
//!
//! ## Role
//!
//! On each daemon tick (1-min interval), the runtime calls
//! [`evaluate_cron_fires`] with the current UTC time. The evaluator reads every
//! Work with a non-empty `works.schedule_json`, and for each enabled role
//! (`brainstorm` / `write` / `review`) checks whether the per-Work cron fires
//! at the current minute (in the author's configured TZ). Fires that pass the
//! per-Work gating (§4.3) and the idempotency guard (§4.2) enqueue a pending
//! [`Schedule`](nexus_contracts::local::schedule::Schedule) via
//! [`crate::auto_chain::enqueue_cron_schedule`]. The existing
//! [`super::supervisor::ScheduleSupervisor::tick`] then admits it; the existing
//! executor runs it; the existing terminal pipeline handles completion.
//!
//! The cron path is **out-of-band**: it does NOT touch the Work's
//! `driver_schedule_id` (mirrors `enqueue_review_master_schedule`), so a cron
//! fire never disrupts an in-progress FL-E chain.
//!
//! ## Scope
//!
//! All three roles are evaluated: `brainstorm`, `write`, and `review`. The
//! `review` role (V1.50 T-A P2) enqueues a `novel-review-master` schedule;
//! the existing supervisor terminal pipeline then fires the T-B P1
//! review-time KB-extraction hook (`quality_loop::extract_kb_candidates_for_review`)
//! on completion.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::preset_ids::{
    NOVEL_BRAINSTORM_PRESET_ID, NOVEL_REVIEW_MASTER_PRESET_ID, NOVEL_WRITE_PRESET_ID,
};

/// Canonical role names (spec §2.1).
const ROLE_BRAINSTORM: &str = "brainstorm";
const ROLE_WRITE: &str = "write";
const ROLE_REVIEW: &str = "review";

/// Active schedule statuses that block a re-fire (spec §4.2 idempotency).
const ACTIVE_STATUS_LIST: &str = "'pending', 'running', 'paused'";

/// Summary of one cron-evaluation sweep (returned for observability / tests).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CronFireSummary {
    /// Schedules enqueued this sweep.
    pub fired: usize,
    /// Fires skipped by per-Work gating (intake / lock / completion).
    pub skipped_gated: usize,
    /// Fires skipped by the idempotency guard (active prior schedule).
    pub skipped_idempotent: usize,
    /// Works whose `schedule_json` failed to parse (skipped, logged at warn).
    pub skipped_parse_error: usize,
    /// Role entries skipped because the cron did not match the current minute
    /// or the role was disabled. (Common case — logged at debug.)
    pub skipped_no_match: usize,
    /// R-V150-WLA-07 (V1.50 P-last WL-A / cron-brainstorm-write qc1 S-001):
    /// role entries whose idempotency-check DB read returned an error.
    /// Surfaced as its own bucket (rather than folded into
    /// `skipped_idempotent`) so `total_evaluated()` reflects the work the
    /// sweep actually performed and operators can spot a flaky reads path
    /// in the sweep summary log line.
    pub skipped_idempotency_check_error: usize,
}

impl CronFireSummary {
    /// Total roles evaluated across all Works (for log lines).
    #[must_use]
    pub const fn total_evaluated(&self) -> usize {
        self.fired
            + self.skipped_gated
            + self.skipped_idempotent
            + self.skipped_parse_error
            + self.skipped_no_match
            + self.skipped_idempotency_check_error
    }
}

// ── Per-Work cron config (minimal serde mirror of spec §2.1) ───────────────
//
// The full `WorkSchedule` model lives in `nexus42::commands::creator::works::cron`
// (the CLI surface). The daemon evaluator cannot depend on the CLI crate
// (circular: nexus42 → nexus-orchestration), so this module defines a minimal
// mirror that parses the same JSON shape (spec §2.1). `Option` fields make a
// partial config robust — a missing role simply does not fire.

#[derive(Debug, Deserialize)]
struct CronConfig {
    tz: String,
    roles: CronRoles,
}

#[derive(Debug, Default, Deserialize)]
struct CronRoles {
    #[serde(default)]
    brainstorm: Option<CronRole>,
    #[serde(default)]
    write: Option<CronRole>,
    #[serde(default)]
    review: Option<CronRole>,
}

#[derive(Debug, Deserialize)]
struct CronRole {
    cron: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

const fn default_enabled() -> bool {
    // Spec §2.1: default `enabled = true`.
    true
}

/// Map a role name to its trigger preset id (spec §2.1 table).
///
/// Returns `None` for unknown role names (defensive — the evaluator only
/// iterates the three canonical roles above).
fn role_preset(role: &str) -> Option<&'static str> {
    match role {
        ROLE_BRAINSTORM => Some(NOVEL_BRAINSTORM_PRESET_ID),
        ROLE_WRITE => Some(NOVEL_WRITE_PRESET_ID),
        ROLE_REVIEW => Some(NOVEL_REVIEW_MASTER_PRESET_ID),
        _ => None,
    }
}

/// Evaluate per-Work cron schedules and enqueue fires (spec §4.1).
///
/// Called on each daemon tick with the current UTC time. Best-effort: per-Work
/// errors are logged and the sweep continues. Returns a [`CronFireSummary`]
/// for observability and hermetic tests.
///
/// # Panics
///
/// Never — all DB / parse errors are caught and logged.
pub async fn evaluate_cron_fires(pool: &SqlitePool, now: DateTime<Utc>) -> CronFireSummary {
    let rows = match nexus_local_db::works::list_works_with_schedule_json(pool).await {
        Ok(r) => r,
        Err(e) => {
            warn!(
                error = %e,
                "cron-supervisor: scan query failed; skipping sweep"
            );
            return CronFireSummary::default();
        }
    };

    let mut summary = CronFireSummary::default();
    let works_scanned = rows.len();
    for row in &rows {
        evaluate_work(pool, row, now, &mut summary).await;
    }

    info!(
        works_scanned,
        fired = summary.fired,
        skipped_gated = summary.skipped_gated,
        skipped_idempotent = summary.skipped_idempotent,
        skipped_parse_error = summary.skipped_parse_error,
        skipped_no_match = summary.skipped_no_match,
        skipped_idempotency_check_error = summary.skipped_idempotency_check_error,
        "cron-supervisor: sweep complete"
    );
    summary
}

/// Evaluate all enabled roles for one Work.
async fn evaluate_work(
    pool: &SqlitePool,
    row: &nexus_local_db::works::WorkCronRow,
    now: DateTime<Utc>,
    summary: &mut CronFireSummary,
) {
    let config: CronConfig = match serde_json::from_str(&row.schedule_json) {
        Ok(c) => c,
        Err(e) => {
            summary.skipped_parse_error += 1;
            warn!(
                work_id = %row.work_id,
                work_ref = ?row.work_ref,
                error = %e,
                "cron-supervisor: schedule_json parse failed; skipping Work"
            );
            return;
        }
    };

    let Ok(tz) = Tz::from_str(&config.tz) else {
        summary.skipped_parse_error += 1;
        warn!(
            work_id = %row.work_id,
            work_ref = ?row.work_ref,
            tz = %config.tz,
            "cron-supervisor: invalid IANA tz; skipping Work"
        );
        return;
    };

    let gate = gate_reason(row);
    for (role_name, role_opt) in [
        (ROLE_BRAINSTORM, config.roles.brainstorm.as_ref()),
        (ROLE_WRITE, config.roles.write.as_ref()),
        (ROLE_REVIEW, config.roles.review.as_ref()),
    ] {
        try_fire_role(pool, row, role_name, role_opt, tz, now, gate, summary).await;
    }
}

/// Evaluate one role for one Work: skip (no-match / disabled / gated /
/// idempotent) or enqueue. Mutates `summary` to record the outcome.
//
// 8 args is inherent to this private linear pipeline (each arg is read once in
// a flat skip-or-fire sequence). A context struct would add indirection
// without reducing real complexity — same rationale as `too_many_lines` allows
// elsewhere in this crate (supervisor.rs, auto_chain.rs).
#[allow(clippy::too_many_arguments)]
async fn try_fire_role(
    pool: &SqlitePool,
    row: &nexus_local_db::works::WorkCronRow,
    role_name: &str,
    role: Option<&CronRole>,
    tz: Tz,
    now: DateTime<Utc>,
    gate: Option<&'static str>,
    summary: &mut CronFireSummary,
) {
    let Some(role) = role else {
        summary.skipped_no_match += 1;
        return;
    };
    if !role.enabled {
        summary.skipped_no_match += 1;
        debug!(
            work_id = %row.work_id,
            role = role_name,
            "cron-supervisor: role disabled; skipping"
        );
        return;
    }
    if !cron_fires_at_minute_for_work(&row.work_id, role_name, &role.cron, tz, now) {
        summary.skipped_no_match += 1;
        return;
    }

    // Per-Work gating (spec §4.3).
    if let Some(reason) = gate {
        summary.skipped_gated += 1;
        debug!(
            work_id = %row.work_id,
            role = role_name,
            gate_reason = reason,
            "cron-supervisor: gated skip"
        );
        return;
    }

    // Idempotency guard (spec §4.2) + enqueue.
    let Some(preset_id) = role_preset(role_name) else {
        summary.skipped_no_match += 1;
        return;
    };
    match has_active_role_schedule(pool, &row.work_id, preset_id).await {
        Ok(true) => {
            summary.skipped_idempotent += 1;
            // R-V150P1CRONBW-04 (qc3 W-002): the per-skip line is redundant
            // with the `sweep complete` summary above and grows linearly with
            // active Works (e.g. ~12k info lines/hour at 100 Works on a 4×/day
            // brainstorm+write cadence). Drop to debug so it stays available
            // for diagnosis without flooding default-level logs.
            debug!(
                work_id = %row.work_id,
                role = role_name,
                preset_id,
                "cron-supervisor: prior schedule still active; skipping fire"
            );
        }
        Ok(false) => match crate::auto_chain::enqueue_cron_schedule(
            pool,
            &row.creator_id,
            &row.work_id,
            preset_id,
            role_name,
        )
        .await
        {
            Ok(schedule_id) => {
                summary.fired += 1;
                info!(
                    work_id = %row.work_id,
                    role = role_name,
                    preset_id,
                    schedule_id = %schedule_id,
                    "cron-supervisor: enqueued cron-triggered schedule"
                );
            }
            Err(e) => {
                warn!(
                    work_id = %row.work_id,
                    role = role_name,
                    preset_id,
                    error = %e,
                    "cron-supervisor: enqueue failed; skipping fire"
                );
            }
        },
        Err(e) => {
            // R-V150-WLA-07 (V1.50 P-last WL-A / cron-brainstorm-write qc1
            // S-001): previously this arm only logged at warn! and did NOT
            // touch the summary, so a Work whose idempotency check errored
            // was silently absent from CronFireSummary::total_evaluated().
            // Increment the dedicated bucket so the metric is honest.
            summary.skipped_idempotency_check_error += 1;
            warn!(
                work_id = %row.work_id,
                role = role_name,
                error = %e,
                "cron-supervisor: idempotency check failed; skipping fire (non-fatal)"
            );
        }
    }
}

/// Return the gating skip reason for a Work, or `None` if it passes all gates
/// (spec §4.3).
fn gate_reason(row: &nexus_local_db::works::WorkCronRow) -> Option<&'static str> {
    if row.intake_status != "complete" {
        return Some("intake_incomplete");
    }
    if row.runtime_lock_holder.is_some() {
        return Some("runtime_locked");
    }
    if row.completion_locked_at.is_some() {
        return Some("completion_locked");
    }
    None
}

// ── Per-Work cron `Schedule` memoisation (R-V150P1CRONBW-03 / qc3 W-001) ────
//
// `cron::Schedule::from_str` is the hot-path cost flagged by qc3: parsing a
// 5/6-field expression runs Cuckoo-filter + bitmap construction (~10–50 µs).
// For 1 000 Works × 3 roles per minute the per-tick re-parse dominated the
// scan. The cache below keys on `(work_id, role)` and stores the raw cron
// string alongside the parsed `Schedule`, so:
//   - a repeat tick with the same raw string is an O(1) lookup (no re-parse);
//   - a content change (user re-ran `creator works cron set`) is detected by
//     raw-string drift on the next lookup → re-parse + update (content-based
//     invalidation);
//   - explicit [`invalidate_cron_schedule_cache`] clears stale entries after
//     a CLI `schedule_json` write (e.g. when a role is removed entirely).

/// In-process memoisation cache for parsed cron `Schedule`s. See the section
/// doc above for the invalidation contract. Process-global because the daemon
/// calls `evaluate_cron_fires` once per tick with no per-daemon state threaded
/// in; a `OnceLock<Mutex<..>>` is the idiomatic shape for such a memoisation.
type CronScheduleCache = HashMap<(String, String), (String, cron::Schedule)>;

static CRON_SCHEDULE_CACHE: OnceLock<Mutex<CronScheduleCache>> = OnceLock::new();

/// Counts `cron::Schedule::from_str` invocations (cache misses). Used by the
/// `cron_fires_at_minute_uses_memoised_schedule` regression test to assert the
/// cache prevents re-parses. Relaxed atomic → negligible cost in production
/// (one increment per cache miss, which is rare after warm-up).
static CRON_PARSE_COUNT: AtomicU64 = AtomicU64::new(0);

fn cron_schedule_cache() -> &'static Mutex<CronScheduleCache> {
    CRON_SCHEDULE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drop every cached `(work_id, role)` entry.
///
/// Called by the CLI after a successful `set_schedule_json_tx` write so stale
/// entries do not linger after a role config is removed or rewritten. The
/// cache also self-heals on raw-string drift, so this is a memory-hygiene
/// invalidation, not a correctness requirement.
///
/// # Panics
///
/// Propagates a poisoned mutex (same discipline as `std::sync::Mutex`).
pub fn invalidate_cron_schedule_cache() {
    if let Some(mutex) = CRON_SCHEDULE_CACHE.get() {
        mutex
            .lock()
            .expect("cron schedule cache mutex poisoned")
            .clear();
    }
}

/// Does the cron fire at the minute containing `now` (in the author's TZ)?
///
/// Pure building block: parses `cron_expr` on every call. The daemon hot path
/// uses the cached [`cron_fires_at_minute_for_work`] wrapper instead (qc3
/// W-001 / R-V150P1CRONBW-03); this pure form is retained as a test-only
/// fixture that exercises [`schedule_fires_at_minute`] semantics without
/// going through the in-process cache.
///
/// The cron is evaluated in `tz` (spec §2.1: "Daemon converts to UTC for cron
/// firing" — the expression is authored in local time). The match is
/// minute-granular: returns `true` when the expression matches the
/// `(minute, hour, day, month, weekday)` tuple at `now_local` truncated to the
/// start of the minute.
///
/// Returns `false` on any parse failure (the validator at config time should
/// have caught malformed expressions; a corrupt stored blob is skipped at the
/// Work level).
#[cfg(test)]
fn cron_fires_at_minute(cron_expr: &str, tz: Tz, now: DateTime<Utc>) -> bool {
    let normalized = normalize_cron_fields(cron_expr);
    let Ok(schedule) = cron::Schedule::from_str(&normalized) else {
        return false;
    };
    schedule_fires_at_minute(&schedule, tz, now)
}

/// Minute-match check over an already-parsed `Schedule` (no allocation, no
/// parse). Extracted from `cron_fires_at_minute` so the cached path can reuse
/// the identical semantics without re-parsing.
fn schedule_fires_at_minute(schedule: &cron::Schedule, tz: Tz, now: DateTime<Utc>) -> bool {
    // Truncate `now` to the start of the minute in the author's TZ.
    let now_local = match tz.from_utc_datetime(&now.naive_utc()) {
        dt if dt.year() > 0 => dt,
        _ => return false,
    };
    let Some(minute_start) = now_local
        .with_second(0)
        .and_then(|dt| dt.with_nanosecond(0))
    else {
        return false;
    };
    // The first fire strictly after (minute_start − 1 minute) must land
    // exactly on minute_start. This is the standard "does cron match this
    // minute" check: any earlier fire would be < minute_start, any later fire
    // would be > minute_start.
    let just_before = minute_start - chrono::Duration::minutes(1);
    schedule.after(&just_before).next() == Some(minute_start)
}

/// Cached cron-fire check for a `(work_id, role)` pair (R-V150P1CRONBW-03).
///
/// Parses `cron_expr` once per `(work_id, role)` and reuses the `Schedule`
/// across ticks until the raw expression changes (content-drift re-parse) or
/// [`invalidate_cron_schedule_cache`] clears the entry. The actual minute
/// match is delegated to [`schedule_fires_at_minute`]. Returns `false` on a
/// parse failure (and stores nothing).
fn cron_fires_at_minute_for_work(
    work_id: &str,
    role: &str,
    cron_expr: &str,
    tz: Tz,
    now: DateTime<Utc>,
) -> bool {
    let normalized = normalize_cron_fields(cron_expr);
    let key = (work_id.to_string(), role.to_string());

    // Resolve the parsed Schedule under the lock and clone it out so the guard
    // is released before the (µs-scale) minute match — keeps the critical
    // section tight and avoids borrowing across the matcher. The clone is far
    // cheaper than the parse it replaces (qc3 W-001 / R-V150P1CRONBW-03):
    // cache hits stay O(1) per Work per tick.
    let schedule = {
        let mut cache = cron_schedule_cache()
            .lock()
            .expect("cron schedule cache mutex poisoned");
        // (Re)parse only on a miss or when the raw cron string has drifted
        // since the last observation (content-based invalidation).
        let needs_parse = cache
            .get(&key)
            .is_none_or(|(stored_raw, _)| stored_raw != cron_expr);
        if needs_parse {
            CRON_PARSE_COUNT.fetch_add(1, Ordering::Relaxed);
            if let Ok(parsed) = cron::Schedule::from_str(&normalized) {
                cache.insert(key.clone(), (cron_expr.to_string(), parsed));
            } else {
                // Drop any stale entry so a later valid rewrite re-parses cleanly.
                cache.remove(&key);
                return false;
            }
        }
        cache
            .get(&key)
            .expect("entry present immediately after insert/get")
            .1
            .clone()
    };
    schedule_fires_at_minute(&schedule, tz, now)
}

/// Normalize a cron expression to the `cron` crate's ≥6-field format.
///
/// 5-field input (standard crontab) → prepend `0 ` (seconds=0). 6/7-field input
/// is returned unchanged. Mirrors the CLI-side normalizer in
/// `nexus42::commands::creator::works::cron::normalize_cron_fields` so the
/// daemon and CLI interpret expressions identically (spec §2.1 / §3.1).
fn normalize_cron_fields(expr: &str) -> String {
    let trimmed = expr.trim();
    let field_count = trimmed.split_whitespace().count();
    if field_count == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Is there an active (pending/running/paused) schedule for `(work_id, preset_id)`?
///
/// Implements the spec §4.2 idempotency guard: a cron fire is skipped when a
/// prior same-role schedule for the same Work is still active.
///
/// # Errors
///
/// Returns the underlying sqlx error if the COUNT query fails. Callers log and
/// treat the error as "skip this fire" (non-fatal — the next tick retries).
async fn has_active_role_schedule(
    pool: &SqlitePool,
    work_id: &str,
    preset_id: &str,
) -> Result<bool, sqlx::Error> {
    // SAFETY: COUNT query against creator_schedules — runtime query with a
    // dynamic `IN (...)` status list (constant string, not user-controlled).
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM creator_schedules \
         WHERE work_id = ? AND preset_id = ? AND status IN ({ACTIVE_STATUS_LIST})"
    ))
    .bind(work_id)
    .bind(preset_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    // ── CronFireSummary contract (R-V150-WLA-07) ──────────────────────────

    /// R-V150-WLA-07 (V1.50 P-last WL-A / cron-brainstorm-write qc1 S-001):
    /// `skipped_idempotency_check_error` must be a separate bucket so
    /// `total_evaluated()` reflects the work the sweep actually performed.
    /// The pre-fix `Err` arm of the `has_active_role_schedule` call only
    /// logged at warn! and did not touch the summary, so a Work whose
    /// idempotency check errored was silently absent from the metric.
    /// Behavioural coverage (injecting a DB read error) is out of scope
    /// for this contract test — the integration tests in
    /// `tests/cron_supervisor.rs` cover the happy and idempotent paths,
    /// and the new bucket is surfaced in the `sweep complete` log line so
    /// operators can spot a flaky reads path in production.
    #[test]
    fn summary_total_evaluated_includes_idempotency_check_error_bucket() {
        let s = CronFireSummary {
            fired: 2,
            skipped_gated: 1,
            skipped_idempotent: 3,
            skipped_parse_error: 0,
            skipped_no_match: 10,
            skipped_idempotency_check_error: 4,
        };
        // 2 + 1 + 3 + 0 + 10 + 4 = 20 — the new bucket is counted.
        assert_eq!(s.total_evaluated(), 20);

        // Default summary has zero everywhere, including the new field.
        let default = CronFireSummary::default();
        assert_eq!(default.skipped_idempotency_check_error, 0);
        assert_eq!(default.total_evaluated(), 0);
    }

    // ── cron_fires_at_minute ───────────────────────────────────────────────

    #[test]
    fn cron_fires_every_minute_always_matches() {
        // `* * * * *` matches every minute in UTC.
        assert!(cron_fires_at_minute(
            "* * * * *",
            Tz::UTC,
            utc(2026, 6, 19, 3, 0)
        ));
        assert!(cron_fires_at_minute(
            "* * * * *",
            Tz::UTC,
            utc(2026, 6, 19, 3, 7)
        ));
    }

    #[test]
    fn cron_fires_at_specific_hour_minute_matches() {
        // `0 3 * * *` fires at 03:00 UTC.
        assert!(cron_fires_at_minute(
            "0 3 * * *",
            Tz::UTC,
            utc(2026, 6, 19, 3, 0)
        ));
        // Not at 03:01.
        assert!(!cron_fires_at_minute(
            "0 3 * * *",
            Tz::UTC,
            utc(2026, 6, 19, 3, 1)
        ));
        // Not at 04:00.
        assert!(!cron_fires_at_minute(
            "0 3 * * *",
            Tz::UTC,
            utc(2026, 6, 19, 4, 0)
        ));
    }

    #[test]
    fn cron_fires_comma_lists_match_one_slot() {
        // `0 3,9,15,21 * * *` (brainstorm default) fires at 09:00.
        assert!(cron_fires_at_minute(
            "0 3,9,15,21 * * *",
            Tz::UTC,
            utc(2026, 6, 19, 9, 0)
        ));
        // Not at 10:00.
        assert!(!cron_fires_at_minute(
            "0 3,9,15,21 * * *",
            Tz::UTC,
            utc(2026, 6, 19, 10, 0)
        ));
    }

    #[test]
    fn cron_fires_half_hour_pattern() {
        // `0,30 * * * *` (review default) fires at :00 and :30.
        assert!(cron_fires_at_minute(
            "0,30 * * * *",
            Tz::UTC,
            utc(2026, 6, 19, 14, 0)
        ));
        assert!(cron_fires_at_minute(
            "0,30 * * * *",
            Tz::UTC,
            utc(2026, 6, 19, 14, 30)
        ));
        assert!(!cron_fires_at_minute(
            "0,30 * * * *",
            Tz::UTC,
            utc(2026, 6, 19, 14, 15)
        ));
    }

    #[test]
    fn cron_fires_in_author_tz() {
        // `0 3 * * *` in Asia/Shanghai (UTC+8) fires when UTC == 19:00 prev day.
        // 03:00 CST = 2026-06-19 19:00 UTC (2026-06-18).
        assert!(cron_fires_at_minute(
            "0 3 * * *",
            Tz::Asia__Shanghai,
            utc(2026, 6, 18, 19, 0)
        ));
        // Not at 19:01 UTC.
        assert!(!cron_fires_at_minute(
            "0 3 * * *",
            Tz::Asia__Shanghai,
            utc(2026, 6, 18, 19, 1)
        ));
    }

    #[test]
    fn cron_fires_garbage_returns_false() {
        assert!(!cron_fires_at_minute(
            "not a cron",
            Tz::UTC,
            utc(2026, 6, 19, 3, 0)
        ));
        assert!(!cron_fires_at_minute("", Tz::UTC, utc(2026, 6, 19, 3, 0)));
    }

    // ── cron_fires_at_minute_for_work (R-V150P1CRONBW-03 memoisation) ───────

    /// R-V150P1CRONBW-03 (qc3 W-001): the cached path must parse the cron
    /// expression exactly once per `(work_id, role, cron_string)` triple; a
    /// content change on the same key, or a brand-new key, must trigger a
    /// fresh parse. Asserts via the `CRON_PARSE_COUNT` instrumented counter.
    #[test]
    fn cron_fires_at_minute_uses_memoised_schedule() {
        // Start from a known-empty cache + zeroed counter so the assertions
        // are independent of test-execution order within the lib binary.
        invalidate_cron_schedule_cache();
        CRON_PARSE_COUNT.store(0, Ordering::Relaxed);

        let now = utc(2026, 6, 19, 9, 0);

        // 100 calls for the same (work, role, cron) → exactly one parse.
        for _ in 0..100 {
            assert!(cron_fires_at_minute_for_work(
                "wrk_memo_a",
                "brainstorm",
                "0 9 * * *",
                Tz::UTC,
                now,
            ));
        }
        assert_eq!(
            CRON_PARSE_COUNT.load(Ordering::Relaxed),
            1,
            "100 calls for the same (work, role, cron) must parse exactly once"
        );

        // Repeat calls still hit the cache (counter unchanged).
        for _ in 0..10 {
            assert!(cron_fires_at_minute_for_work(
                "wrk_memo_a",
                "brainstorm",
                "0 9 * * *",
                Tz::UTC,
                now,
            ));
        }
        assert_eq!(CRON_PARSE_COUNT.load(Ordering::Relaxed), 1);

        // Content drift on the same (work, role) → re-parse + cache update.
        assert!(cron_fires_at_minute_for_work(
            "wrk_memo_a",
            "brainstorm",
            "0 10 * * *",
            Tz::UTC,
            utc(2026, 6, 19, 10, 0),
        ));
        assert_eq!(CRON_PARSE_COUNT.load(Ordering::Relaxed), 2);

        // Different (work, role) → separate parse.
        assert!(cron_fires_at_minute_for_work(
            "wrk_memo_b",
            "write",
            "0 9 * * *",
            Tz::UTC,
            now,
        ));
        assert_eq!(CRON_PARSE_COUNT.load(Ordering::Relaxed), 3);

        // Cached matcher still respects the minute granularity: a non-match
        // minute for the SAME (work, role, cron) must not re-parse. The entry
        // for ("wrk_memo_a", "brainstorm") currently caches "0 10 * * *"
        // (post-drift above), so 09:00 UTC is a non-match (hour 9 ≠ 10).
        assert!(!cron_fires_at_minute_for_work(
            "wrk_memo_a",
            "brainstorm",
            "0 10 * * *",
            Tz::UTC,
            utc(2026, 6, 19, 9, 0),
        ));
        assert_eq!(
            CRON_PARSE_COUNT.load(Ordering::Relaxed),
            3,
            "non-match must not re-parse the cached entry"
        );

        // Explicit invalidation clears the cache → next call re-parses.
        invalidate_cron_schedule_cache();
        assert!(cron_fires_at_minute_for_work(
            "wrk_memo_a",
            "brainstorm",
            "0 9 * * *",
            Tz::UTC,
            now,
        ));
        assert_eq!(
            CRON_PARSE_COUNT.load(Ordering::Relaxed),
            4,
            "invalidate_cron_schedule_cache must force a re-parse on next call"
        );

        // Cleanup so no entry leaks into sibling tests in the same process.
        invalidate_cron_schedule_cache();
    }

    #[test]
    fn normalize_five_field_prepends_seconds() {
        assert_eq!(normalize_cron_fields("0 3 * * *"), "0 0 3 * * *");
    }

    #[test]
    fn normalize_six_field_unchanged() {
        assert_eq!(normalize_cron_fields("0 0 3 * * *"), "0 0 3 * * *");
    }

    // ── gate_reason ─────────────────────────────────────────────────────────

    use nexus_local_db::works::WorkCronRow;

    fn cron_row(intake: &str, lock: Option<&str>, completion: Option<&str>) -> WorkCronRow {
        WorkCronRow {
            work_id: "wrk_test".to_string(),
            creator_id: "ctr_test".to_string(),
            work_ref: Some("test".to_string()),
            schedule_json: "{}".to_string(),
            intake_status: intake.to_string(),
            runtime_lock_holder: lock.map(str::to_string),
            completion_locked_at: completion.map(str::to_string),
        }
    }

    #[test]
    fn gate_passes_when_healthy() {
        assert!(gate_reason(&cron_row("complete", None, None)).is_none());
    }

    #[test]
    fn gate_blocks_intake_incomplete() {
        assert_eq!(
            gate_reason(&cron_row("active", None, None)),
            Some("intake_incomplete")
        );
    }

    #[test]
    fn gate_blocks_runtime_lock() {
        assert_eq!(
            gate_reason(&cron_row("complete", Some("daemon:schedule:x"), None)),
            Some("runtime_locked")
        );
    }

    #[test]
    fn gate_blocks_completion_locked() {
        assert_eq!(
            gate_reason(&cron_row("complete", None, Some("2026-06-19T00:00:00Z"))),
            Some("completion_locked")
        );
    }

    // ── role_preset ─────────────────────────────────────────────────────────

    #[test]
    fn role_preset_maps_all_three_roles() {
        assert_eq!(role_preset("brainstorm"), Some(NOVEL_BRAINSTORM_PRESET_ID));
        assert_eq!(role_preset("write"), Some(NOVEL_WRITE_PRESET_ID));
        assert_eq!(role_preset("review"), Some(NOVEL_REVIEW_MASTER_PRESET_ID));
        // Unknown role names are a defensive `None`.
        assert_eq!(role_preset("unknown"), None);
    }
}
