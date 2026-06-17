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
//! (`brainstorm` / `write` in T-A P1) checks whether the per-Work cron fires
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
//! ## Scope (T-A P1)
//!
//! Only `brainstorm` + `write` roles are evaluated. `review` cron firing is
//! T-A P2 (non-goal per plan §3).

use std::str::FromStr;

use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::preset_ids::{NOVEL_BRAINSTORM_PRESET_ID, NOVEL_WRITE_PRESET_ID};

/// Canonical role names (spec §2.1) in scope for T-A P1.
const ROLE_BRAINSTORM: &str = "brainstorm";
const ROLE_WRITE: &str = "write";

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
    // `review` is intentionally absent — T-A P2 (plan §3 non-goal).
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
/// Returns `None` for roles out of scope for T-A P1 (e.g. `review`).
fn role_preset(role: &str) -> Option<&'static str> {
    match role {
        ROLE_BRAINSTORM => Some(NOVEL_BRAINSTORM_PRESET_ID),
        ROLE_WRITE => Some(NOVEL_WRITE_PRESET_ID),
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
    if !cron_fires_at_minute(&role.cron, tz, now) {
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
            info!(
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

/// Does `cron_expr` fire at the minute containing `now` (in the author's TZ)?
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
fn cron_fires_at_minute(cron_expr: &str, tz: Tz, now: DateTime<Utc>) -> bool {
    let normalized = normalize_cron_fields(cron_expr);
    let Ok(schedule) = cron::Schedule::from_str(&normalized) else {
        return false;
    };
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
    fn role_preset_maps_brainstorm_and_write() {
        assert_eq!(role_preset("brainstorm"), Some(NOVEL_BRAINSTORM_PRESET_ID));
        assert_eq!(role_preset("write"), Some(NOVEL_WRITE_PRESET_ID));
        // review is out of scope for T-A P1.
        assert_eq!(role_preset("review"), None);
    }
}
