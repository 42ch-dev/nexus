//! `creator works cron` — per-Work cron configuration (V1.50 T-A P0).
//!
//! Implements the per-Work cron config layer for novel-writing three-role
//! staggering. Foundation only — cron *firing* into the auto-chain is T-A P1.
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §2 / §3.
//!
//! ## Architecture (foundation slice)
//!
//! The `set/show/list` handlers read and write `works.schedule_json` directly
//! via `nexus-local-db` (the CLI already depends on it; precedent:
//! `commands/creator/soul::open_global_db`). This keeps the foundation within
//! the plan's code-touch list — no daemon handler changes — while making the
//! command functional at runtime. The daemon's cron *firing* (T-A P1) will
//! read the same `works.schedule_json` column.

use std::fmt::Write as _;
use std::str::FromStr;

use clap::Subcommand;

use crate::config::CliConfig;
use crate::errors::{CliError, Result};

// ── Default schedule table (spec §2.2) ───────────────────────────────────

/// Canonical default cron expressions per role (spec §2.1 / §2.2).
///
/// These match the novels-system reference table. When `works.schedule_json`
/// is empty/NULL, the daemon uses defaults from this table.
const DEFAULT_BRAINSTORM_CRON: &str = "0 3,9,15,21 * * *";
const DEFAULT_WRITE_CRON: &str = "0 4,10,16,22 * * *";
const DEFAULT_REVIEW_CRON: &str = "0,30 * * * *";
const DEFAULT_TZ: &str = "UTC";

// ── WorkSchedule serde model (spec §2.1) ─────────────────────────────────

/// Per-role cron entry (spec §2.1).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RoleSchedule {
    /// 5-field cron expression (author local TZ).
    pub cron: String,
    /// Per-role opt-out without removing the schedule.
    pub enabled: bool,
}

/// The three-role staggering set (spec §2.1 `roles`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RolesSchedule {
    /// `brainstorm` → `novel-brainstorm` preset.
    pub brainstorm: RoleSchedule,
    /// `write` → `novel-write` preset.
    pub write: RoleSchedule,
    /// `review` → `novel-review-master` preset.
    pub review: RoleSchedule,
}

/// Full per-Work cron configuration (spec §2.1 top-level shape).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkSchedule {
    /// IANA timezone string. Daemon converts to UTC for cron firing.
    pub tz: String,
    /// Per-role cron entries.
    pub roles: RolesSchedule,
}

impl WorkSchedule {
    /// Build the all-defaults schedule (spec §2.3: empty/NULL → defaults).
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            tz: DEFAULT_TZ.to_string(),
            roles: RolesSchedule {
                brainstorm: RoleSchedule {
                    cron: DEFAULT_BRAINSTORM_CRON.to_string(),
                    enabled: true,
                },
                write: RoleSchedule {
                    cron: DEFAULT_WRITE_CRON.to_string(),
                    enabled: true,
                },
                review: RoleSchedule {
                    cron: DEFAULT_REVIEW_CRON.to_string(),
                    enabled: true,
                },
            },
        }
    }

    /// Serialize to the `schedule_json` blob text.
    ///
    /// # Errors
    ///
    /// Returns [`CliError::Other`] only if serialization fails (cannot happen
    /// for this struct shape in practice).
    fn to_json_string(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| CliError::Other(format!("schedule_json serialization failed: {e}")))
    }
}

// ── Validation (spec §3.1; AC #5) ────────────────────────────────────────

/// Stable error codes for cron config validation (AC #5).
const ERR_INVALID_CRON: &str = "E_CRON_INVALID_EXPR";
const ERR_INVALID_TZ: &str = "E_CRON_INVALID_TZ";
/// Stable error code for the spec §3.1 "all-off" rule (R-V150P0-W2).
const ERR_ALL_DISABLED: &str = "E_CRON_ALL_ROLES_DISABLED";

/// Validate a 5-field cron expression via the `cron` crate.
///
/// The `cron` crate (zslayton) requires ≥6 fields (seconds first). Standard
/// crontab expressions are 5-field, so we prepend `0 ` (seconds=0) when the
/// input has exactly 5 fields. This preserves semantics:
/// `"0 3,9,15,21 * * *"` (min=0, hour=3/9/15/21) → `"0 0 3,9,15,21 * * *"`
/// (sec=0, min=0, hour=3/9/15/21).
///
/// # Errors
///
/// Returns [`CliError::Config`] with stable code `E_CRON_INVALID_EXPR` when the
/// expression does not parse.
pub fn validate_cron_expr(expr: &str) -> Result<()> {
    let normalized = normalize_cron_fields(expr);
    if cron::Schedule::from_str(&normalized).is_err() {
        return Err(CliError::Config(format!(
            "[{ERR_INVALID_CRON}] invalid cron expression: '{expr}'"
        )));
    }
    Ok(())
}

/// Normalize a cron expression to the `cron` crate's ≥6-field format.
///
/// 5-field input → prepend `0 ` (seconds=0). 6/7-field input is returned
/// unchanged. Whitespace-only or empty input is left as-is so the parser
/// produces a meaningful error.
fn normalize_cron_fields(expr: &str) -> String {
    let trimmed = expr.trim();
    let field_count = trimmed.split_whitespace().count();
    if field_count == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Validate an IANA timezone string via `chrono-tz`.
///
/// # Errors
///
/// Returns [`CliError::Config`] with stable code `E_CRON_INVALID_TZ` when the
/// timezone string is not a known IANA zone.
pub fn validate_tz(tz: &str) -> Result<()> {
    if chrono_tz::Tz::from_str(tz).is_err() {
        return Err(CliError::Config(format!(
            "[{ERR_INVALID_TZ}] invalid IANA timezone: '{tz}'"
        )));
    }
    Ok(())
}

// ── Resolve stored blob → effective schedule ─────────────────────────────

/// Resolve a stored `schedule_json` blob into an effective [`WorkSchedule`].
///
/// Empty/NULL/absent/unparseable → all defaults (spec §2.3). A partial blob is
/// not merged field-by-field: malformed JSON falls back to defaults so the
/// daemon never fires from a corrupt schedule.
///
/// # Errors
///
/// Does not error — falls back to defaults on any malformation.
#[must_use]
pub fn resolve_schedule(stored: Option<&str>) -> WorkSchedule {
    let Some(json) = stored.filter(|s| !s.is_empty()) else {
        return WorkSchedule::defaults();
    };
    serde_json::from_str::<WorkSchedule>(json).unwrap_or_else(|_| WorkSchedule::defaults())
}

// ── CLI set-args application ─────────────────────────────────────────────

/// Inputs from `creator works cron set` flags (spec §3.1).
//
// The bool fields are a 1:1 mirror of clap's `--no-<role>` / `--all-off`
// flags; restructuring into enums would diverge from the CLI surface, so we
// accept the bool count here.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct CronSetArgs {
    /// `--brainstorm <cron>` (None = leave unchanged from current schedule).
    pub brainstorm: Option<String>,
    /// `--write <cron>`.
    pub write: Option<String>,
    /// `--review <cron>`.
    pub review: Option<String>,
    /// `--tz <iana-tz>`.
    pub tz: Option<String>,
    /// `--no-brainstorm` (disable role).
    pub no_brainstorm: bool,
    /// `--no-write`.
    pub no_write: bool,
    /// `--no-review`.
    pub no_review: bool,
    /// `--all-off` (spec §3.1): escape hatch permitting all roles disabled.
    pub all_off: bool,
}

/// Apply `set` flags onto a base schedule, validating every cron/TZ value.
///
/// When no flags are passed, the result is the all-defaults schedule (spec
/// §3.1: `creator works cron set <work_ref>` alone resets to defaults).
///
/// # Errors
///
/// Returns [`CliError::Config`] (stable code) on invalid cron/TZ (AC #5), or
/// stable code `E_CRON_ALL_ROLES_DISABLED` when every role ends up disabled
/// and `--all-off` was not passed (spec §3.1, R-V150P0-W2).
pub fn apply_set_args(base: WorkSchedule, args: &CronSetArgs) -> Result<WorkSchedule> {
    // Validate every provided value before mutating (fail-fast, no partial write).
    if let Some(ref expr) = args.brainstorm {
        validate_cron_expr(expr)?;
    }
    if let Some(ref expr) = args.write {
        validate_cron_expr(expr)?;
    }
    if let Some(ref expr) = args.review {
        validate_cron_expr(expr)?;
    }
    if let Some(ref tz) = args.tz {
        validate_tz(tz)?;
    }

    let mut schedule = base;
    if args.brainstorm.is_some()
        || args.write.is_some()
        || args.review.is_some()
        || args.tz.is_some()
        || args.no_brainstorm
        || args.no_write
        || args.no_review
    {
        // At least one flag → patch the current schedule.
        if let Some(expr) = &args.brainstorm {
            schedule.roles.brainstorm.cron.clone_from(expr);
        }
        if let Some(expr) = &args.write {
            schedule.roles.write.cron.clone_from(expr);
        }
        if let Some(expr) = &args.review {
            schedule.roles.review.cron.clone_from(expr);
        }
        if let Some(tz) = &args.tz {
            schedule.tz.clone_from(tz);
        }
        if args.no_brainstorm {
            schedule.roles.brainstorm.enabled = false;
        }
        if args.no_write {
            schedule.roles.write.enabled = false;
        }
        if args.no_review {
            schedule.roles.review.enabled = false;
        }
    } else {
        // No flags at all → reset to defaults (spec §3.1).
        schedule = WorkSchedule::defaults();
        // R-V150P0-W1: honor an explicitly-resolved TZ (env/default fold from
        // `handle_set`) when resetting, so NEXUS_TZ / UTC land correctly. The
        // raw CLI args have `tz=None` on a reset, so this only fires when the
        // caller folded a concrete TZ into `effective_args`.
        if let Some(tz) = &args.tz {
            schedule.tz.clone_from(tz);
        }
    }

    // R-V150P0-W2: spec §3.1 — at least one role must remain enabled unless the
    // author explicitly passes `--all-off`. Reject empty schedules with a
    // stable error code so the foundation contract is enforced at config time.
    let any_enabled = schedule.roles.brainstorm.enabled
        || schedule.roles.write.enabled
        || schedule.roles.review.enabled;
    if !any_enabled && !args.all_off {
        return Err(CliError::Config(format!(
            "[{ERR_ALL_DISABLED}] disabling all three roles leaves an empty schedule; \
             pass --all-off to allow pausing every role"
        )));
    }

    Ok(schedule)
}

/// Resolve the TZ to persist for a `set` operation (R-V150P0-W1).
///
/// Pure merge semantic, unit-tested independently of `handle_set`:
///
/// - `args_tz` = the `--tz` flag value. When `Some`, it wins (explicit override).
/// - `env` = the `NEXUS_TZ` env value. Callers pass this **only on the reset
///   path**; on the patch path callers pass `None` so `base.tz` is preserved.
///
/// Returns `Some(tz)` when there is a TZ intent to write, or `None` when the
/// caller should preserve `base.tz` (the patch-without-`--tz` case). The reset
/// caller folds the `DEFAULT_TZ` (`UTC`) fallback itself when this returns
/// `None`.
fn resolve_tz(args_tz: Option<&str>, env: Option<&str>) -> Option<String> {
    args_tz
        .map(str::to_string)
        .or_else(|| env.map(str::to_string))
}

// ── Rendering (spec §3.2 / §3.3) ─────────────────────────────────────────

/// SAFETY label for infallible `writeln!`/`write!` into a `String` (no IO).
const WRITE_OK: &str = "write to String is infallible";

/// Render the `show` output (spec §3.2): local + UTC firing display.
///
/// Pure over `(work_ref, schedule)`. Disabled roles render as `disabled`.
#[must_use]
pub fn render_show(work_ref: &str, schedule: &WorkSchedule) -> String {
    let tz_display = tz_offset_display(&schedule.tz);
    let mut out = String::new();
    writeln!(out, "Work: {work_ref}").expect(WRITE_OK);
    write!(out, "TZ:   {} ({tz_display})\n\n", schedule.tz).expect(WRITE_OK);
    out.push_str("Role        Cron                 Enabled\n");
    writeln!(
        out,
        "brainstorm  {}  {}",
        maybe_disabled(&schedule.roles.brainstorm),
        schedule.roles.brainstorm.enabled
    )
    .expect(WRITE_OK);
    writeln!(
        out,
        "write       {}  {}",
        maybe_disabled(&schedule.roles.write),
        schedule.roles.write.enabled
    )
    .expect(WRITE_OK);
    write!(
        out,
        "review      {}  {}",
        maybe_disabled(&schedule.roles.review),
        schedule.roles.review.enabled
    )
    .expect(WRITE_OK);
    out
}

/// Render `disabled` in place of the cron when a role is disabled, else the cron.
fn maybe_disabled(role: &RoleSchedule) -> String {
    if role.enabled {
        role.cron.clone()
    } else {
        "disabled".to_string()
    }
}

/// Render the `list` output (spec §3.3) across workspace Works.
///
/// Defaults are shown as the canonical cron expression (spec §3.3 last line).
#[must_use]
pub fn render_list(rows: &[ListRow]) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "{:<24} {:<18} {:<18} {:<18} REVIEW",
        "WORK_REF", "TZ", "BRAINSTORM", "WRITE"
    )
    .expect(WRITE_OK);
    for row in rows {
        let ref_display = row.work_ref.clone().unwrap_or_else(|| row.work_id.clone());
        let s = &row.schedule;
        writeln!(
            out,
            "{:<24} {:<18} {:<18} {:<18} {}",
            truncate(&ref_display, 24),
            truncate(&s.tz, 18),
            truncate(&role_label(&s.roles.brainstorm), 18),
            truncate(&role_label(&s.roles.write), 18),
            role_label(&s.roles.review),
        )
        .expect(WRITE_OK);
    }
    out
}

/// One row for the `list` surface.
#[derive(Debug, Clone)]
pub struct ListRow {
    /// Human slug (None when the Work has no `work_ref` set).
    pub work_ref: Option<String>,
    /// Primary key (`wrk_...`); used as display fallback.
    pub work_id: String,
    /// Effective resolved schedule (defaults merged in).
    pub schedule: WorkSchedule,
}

/// Label for a role cell: disabled → `disabled`; enabled → the cron expression.
/// `resolve_schedule` already folds defaults in, so an unset Work shows the
/// canonical default cron here (spec §3.3 last line).
fn role_label(role: &RoleSchedule) -> String {
    if role.enabled {
        role.cron.clone()
    } else {
        "disabled".to_string()
    }
}

/// Best-effort UTC offset display for a TZ (e.g. `UTC+08:00`).
///
/// Falls back to `UTC offset unknown` when the TZ can't be resolved (should
/// not happen post-validation, but render must never panic). chrono's offset
/// `Display` already includes sign and HH:MM (e.g. `+08:00`).
fn tz_offset_display(tz: &str) -> String {
    let Ok(zone) = chrono_tz::Tz::from_str(tz) else {
        return "UTC offset unknown".to_string();
    };
    let now_local = chrono::Utc::now().with_timezone(&zone);
    let offset = now_local.offset();
    format!("UTC{offset}")
}

/// Truncate a string to `max` chars, appending `…` if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

// ── Clap subcommand definitions (spec §3) ────────────────────────────────

/// `creator works cron` subcommand group (spec §3).
#[derive(Debug, Subcommand)]
pub enum CronCommand {
    /// Set per-Work cron configuration (spec §3.1).
    Set {
        /// Work reference (`work_ref` slug) or `work_id` (`wrk_...`).
        work_ref: String,
        /// Brainstorm cron expression (5-field).
        #[arg(long)]
        brainstorm: Option<String>,
        /// Write cron expression (5-field).
        #[arg(long)]
        write: Option<String>,
        /// Review cron expression (5-field).
        #[arg(long)]
        review: Option<String>,
        /// IANA timezone (default: `NEXUS_TZ` env, fallback `UTC`).
        #[arg(long)]
        tz: Option<String>,
        /// Disable the brainstorm role (`enabled: false`).
        #[arg(long, default_value_t = false)]
        no_brainstorm: bool,
        /// Disable the write role.
        #[arg(long, default_value_t = false)]
        no_write: bool,
        /// Disable the review role.
        #[arg(long, default_value_t = false)]
        no_review: bool,
        /// Permit disabling all three roles at once (spec §3.1 "all-off" rule).
        #[arg(long, default_value_t = false)]
        all_off: bool,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show the resolved schedule with local + UTC firing times (spec §3.2).
    Show {
        /// Work reference (`work_ref` slug) or `work_id` (`wrk_...`).
        work_ref: String,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List cron config across all Works in the active workspace (spec §3.3).
    List {
        /// Maximum number of Works to return (default 100; R-V150P0-W4).
        #[arg(long)]
        limit: Option<u32>,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

// ── Dispatcher + handlers ────────────────────────────────────────────────

/// Dispatch `creator works cron` subcommands.
///
/// Opens the local `state.db` directly (foundation slice — no daemon endpoint
/// changes; the daemon firing layer arrives in T-A P1).
///
/// # Errors
///
/// Returns [`CliError`] on DB open failure, work resolution failure, or
/// validation failure (AC #5).
pub async fn handle_cron(cmd: CronCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .clone()
        .ok_or(CliError::CreatorNotSelected)?;
    let workspace_slug = config.workspace_slug_for_creator(&creator_id).to_string();

    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;

    match cmd {
        CronCommand::Set {
            work_ref,
            brainstorm,
            write,
            review,
            tz,
            no_brainstorm,
            no_write,
            no_review,
            all_off,
            json,
        } => {
            handle_set(
                &pool,
                &creator_id,
                &workspace_slug,
                &work_ref,
                &CronSetArgs {
                    brainstorm,
                    write,
                    review,
                    tz,
                    no_brainstorm,
                    no_write,
                    no_review,
                    all_off,
                },
                json,
            )
            .await
        }
        CronCommand::Show { work_ref, json } => {
            handle_show(&pool, &creator_id, &workspace_slug, &work_ref, json).await
        }
        CronCommand::List { limit, json } => {
            handle_list(&pool, &creator_id, &workspace_slug, limit, json).await
        }
    }
}

/// `creator works cron set` — persist the per-Work cron config (spec §3.1).
async fn handle_set(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    work_ref: &str,
    args: &CronSetArgs,
    json: bool,
) -> Result<()> {
    let work_id = resolve_work_id(pool, creator_id, workspace_slug, work_ref).await?;

    // Base = current stored schedule (or defaults if unset).
    let stored = nexus_local_db::works::get_schedule_json(pool, &work_id).await?;
    let base = resolve_schedule(stored.as_deref());

    // R-V150P0-W1: TZ merge semantics. Only fold env/default TZ on the reset
    // path (no flags) or when `--tz` is passed. A role-only patch without
    // `--tz` must preserve the previously-configured `base.tz` — folding the
    // env/default here used to silently clobber it.
    let is_reset = args.brainstorm.is_none()
        && args.write.is_none()
        && args.review.is_none()
        && args.tz.is_none()
        && !args.no_brainstorm
        && !args.no_write
        && !args.no_review;
    let env_for_resolve = if is_reset {
        std::env::var("NEXUS_TZ").ok()
    } else {
        None
    };
    let resolved_tz = resolve_tz(args.tz.as_deref(), env_for_resolve.as_deref());
    let mut effective_args = args.clone();
    effective_args.tz = match resolved_tz {
        Some(tz) => Some(tz),
        None if is_reset => Some(DEFAULT_TZ.to_string()),
        None => None,
    };

    let schedule = apply_set_args(base, &effective_args)?;
    let blob = schedule.to_json_string()?;
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::works::set_schedule_json(pool, &work_id, &blob, &now).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&schedule)?);
    } else {
        println!("Cron schedule set for Work {work_id} (ref: {work_ref})");
        println!("{}", render_show(work_ref, &schedule));
    }
    Ok(())
}

/// `creator works cron show` — render the resolved schedule (spec §3.2).
async fn handle_show(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    work_ref: &str,
    json: bool,
) -> Result<()> {
    let work_id = resolve_work_id(pool, creator_id, workspace_slug, work_ref).await?;
    let stored = nexus_local_db::works::get_schedule_json(pool, &work_id).await?;
    let schedule = resolve_schedule(stored.as_deref());

    if json {
        println!("{}", serde_json::to_string_pretty(&schedule)?);
    } else {
        println!("{}", render_show(work_ref, &schedule));
    }
    Ok(())
}

/// `creator works cron list` — list cron across workspace Works (spec §3.3).
async fn handle_list(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    limit: Option<u32>,
    json: bool,
) -> Result<()> {
    let rows_db =
        nexus_local_db::works::list_works_schedule(pool, creator_id, workspace_slug, limit)
            .await?;
    let rows: Vec<ListRow> = rows_db
        .into_iter()
        .map(|r| ListRow {
            work_ref: r.work_ref,
            work_id: r.work_id,
            schedule: resolve_schedule(r.schedule_json.as_deref()),
        })
        .collect();

    if json {
        let json_rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "work_ref": r.work_ref,
                    "schedule": r.schedule,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "works": json_rows }))?
        );
    } else {
        print!("{}", render_list(&rows));
    }
    Ok(())
}

/// Resolve `<work_ref>` / `<work_id>` to a concrete `work_id`, with a clear
/// error when no Work matches in the active workspace.
async fn resolve_work_id(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    ref_or_id: &str,
) -> Result<String> {
    nexus_local_db::works::resolve_work_id_by_ref_or_id(pool, creator_id, workspace_slug, ref_or_id)
        .await?
        .ok_or_else(|| {
            CliError::Config(format!(
                "No Work matches '{ref_or_id}' in workspace '{workspace_slug}'. \
                 Use `nexus42 creator works list` to see available Works."
            ))
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn validate_cron_accepts_spec_defaults() {
        // All three spec defaults must validate (5-field).
        validate_cron_expr(DEFAULT_BRAINSTORM_CRON).unwrap();
        validate_cron_expr(DEFAULT_WRITE_CRON).unwrap();
        validate_cron_expr(DEFAULT_REVIEW_CRON).unwrap();
    }

    #[test]
    fn validate_cron_accepts_six_field() {
        validate_cron_expr("0 0 3,9,15,21 * * *").unwrap();
    }

    #[test]
    fn validate_cron_rejects_garbage() {
        assert!(validate_cron_expr("not a cron").is_err());
        assert!(validate_cron_expr("99 99 99 99 99").is_err());
    }

    #[test]
    fn validate_cron_rejects_empty() {
        assert!(validate_cron_expr("").is_err());
    }

    #[test]
    fn validate_tz_accepts_iana_zones() {
        validate_tz("Asia/Shanghai").unwrap();
        validate_tz("UTC").unwrap();
        validate_tz("America/New_York").unwrap();
    }

    #[test]
    fn validate_tz_rejects_invalid() {
        assert!(validate_tz("Mars/Olympus").is_err());
        assert!(validate_tz("not-a-zone").is_err());
    }

    #[test]
    fn validate_errors_carry_stable_codes() {
        let err = validate_cron_expr("garbage").unwrap_err();
        let msg = error_message(&err);
        assert!(
            msg.contains(ERR_INVALID_CRON),
            "cron error must carry stable code {ERR_INVALID_CRON}: {msg}"
        );
        let err = validate_tz("Mars/Olympus").unwrap_err();
        let msg = error_message(&err);
        assert!(
            msg.contains(ERR_INVALID_TZ),
            "tz error must carry stable code {ERR_INVALID_TZ}: {msg}"
        );
    }

    #[test]
    fn resolve_schedule_none_gives_defaults() {
        let s = resolve_schedule(None);
        assert_eq!(s, WorkSchedule::defaults());
    }

    #[test]
    fn resolve_schedule_empty_gives_defaults() {
        let s = resolve_schedule(Some(""));
        assert_eq!(s, WorkSchedule::defaults());
    }

    #[test]
    fn resolve_schedule_malformed_gives_defaults() {
        let s = resolve_schedule(Some("{not json"));
        assert_eq!(s, WorkSchedule::defaults());
    }

    #[test]
    fn resolve_schedule_valid_round_trips() {
        let blob = r#"{"tz":"Asia/Shanghai","roles":{"brainstorm":{"cron":"0 9 * * *","enabled":true},"write":{"cron":"0 10 * * *","enabled":false},"review":{"cron":"0,30 * * * *","enabled":true}}}"#;
        let s = resolve_schedule(Some(blob));
        assert_eq!(s.tz, "Asia/Shanghai");
        assert_eq!(s.roles.brainstorm.cron, "0 9 * * *");
        assert!(s.roles.brainstorm.enabled);
        assert!(!s.roles.write.enabled);
    }

    #[test]
    fn apply_set_args_no_flags_resets_to_defaults() {
        let base = WorkSchedule::defaults();
        let out = apply_set_args(base, &CronSetArgs::default()).unwrap();
        assert_eq!(out, WorkSchedule::defaults());
    }

    #[test]
    fn apply_set_args_brainstorm_flag_validates_and_paches() {
        let base = WorkSchedule::defaults();
        let args = CronSetArgs {
            brainstorm: Some("0 9 * * *".to_string()),
            ..Default::default()
        };
        let out = apply_set_args(base, &args).unwrap();
        assert_eq!(out.roles.brainstorm.cron, "0 9 * * *");
        // Other roles keep defaults.
        assert_eq!(out.roles.write.cron, DEFAULT_WRITE_CRON);
    }

    #[test]
    fn apply_set_args_invalid_cron_fails_before_mutation() {
        let base = WorkSchedule::defaults();
        let args = CronSetArgs {
            brainstorm: Some("garbage".to_string()),
            ..Default::default()
        };
        assert!(apply_set_args(base, &args).is_err());
    }

    #[test]
    fn apply_set_args_invalid_tz_fails() {
        let base = WorkSchedule::defaults();
        let args = CronSetArgs {
            tz: Some("Mars/Olympus".to_string()),
            ..Default::default()
        };
        assert!(apply_set_args(base, &args).is_err());
    }

    #[test]
    fn apply_set_args_no_flags_disable_roles() {
        let base = WorkSchedule::defaults();
        let args = CronSetArgs {
            no_review: true,
            ..Default::default()
        };
        let out = apply_set_args(base, &args).unwrap();
        assert!(!out.roles.review.enabled);
        assert!(out.roles.brainstorm.enabled);
    }

    #[test]
    fn render_show_marks_disabled_roles() {
        let mut s = WorkSchedule::defaults();
        s.roles.write.enabled = false;
        let out = render_show("my-work", &s);
        assert!(out.contains("Work: my-work"));
        assert!(out.contains("disabled"));
        assert!(out.contains("Asia") || out.contains("UTC")); // tz line present
    }

    #[test]
    fn render_list_shows_header_and_rows() {
        let rows = vec![ListRow {
            work_ref: Some("my-work".to_string()),
            work_id: "wrk_001".to_string(),
            schedule: WorkSchedule::defaults(),
        }];
        let out = render_list(&rows);
        assert!(out.contains("WORK_REF"));
        assert!(out.contains("my-work"));
        assert!(out.contains(DEFAULT_BRAINSTORM_CRON));
    }

    #[test]
    fn render_list_uses_disabled_label() {
        let mut s = WorkSchedule::defaults();
        s.roles.brainstorm.enabled = false;
        let rows = vec![ListRow {
            work_ref: Some("my-work".to_string()),
            work_id: "wrk_002".to_string(),
            schedule: s,
        }];
        let out = render_list(&rows);
        assert!(out.contains("disabled"));
    }

    #[test]
    fn normalize_five_field_prepends_seconds() {
        assert_eq!(
            normalize_cron_fields("0 3,9,15,21 * * *"),
            "0 0 3,9,15,21 * * *"
        );
    }

    #[test]
    fn normalize_six_field_unchanged() {
        assert_eq!(normalize_cron_fields("0 0 3 * * *"), "0 0 3 * * *");
    }

    // ── R-V150P0-W1: TZ preservation on role-only patch ──────────────────

    #[test]
    fn resolve_tz_explicit_arg_wins() {
        assert_eq!(
            resolve_tz(Some("Asia/Shanghai"), Some("UTC")),
            Some("Asia/Shanghai".to_string())
        );
    }

    #[test]
    fn resolve_tz_env_used_when_no_arg() {
        assert_eq!(
            resolve_tz(None, Some("America/New_York")),
            Some("America/New_York".to_string())
        );
    }

    #[test]
    fn resolve_tz_none_when_no_intent() {
        assert_eq!(resolve_tz(None, None), None);
    }

    #[test]
    fn set_no_review_preserves_custom_tz() {
        // Pre-store a Work with tz="Asia/Shanghai"; patch with --no-review and
        // no --tz. apply_set_args must leave base.tz unchanged (R-V150P0-W1).
        let mut base = WorkSchedule::defaults();
        base.tz = "Asia/Shanghai".to_string();
        let args = CronSetArgs {
            no_review: true,
            ..Default::default()
        };
        let out = apply_set_args(base, &args).expect("patch must apply");
        assert_eq!(
            out.tz, "Asia/Shanghai",
            "role-only patch without --tz must preserve base.tz"
        );
        assert!(!out.roles.review.enabled);
    }

    // ── R-V150P0-W2: spec §3.1 "all-off" rule ─────────────────────────────

    #[test]
    fn apply_set_args_all_off_without_flag_rejects() {
        let base = WorkSchedule::defaults();
        let args = CronSetArgs {
            no_brainstorm: true,
            no_write: true,
            no_review: true,
            all_off: false,
            ..Default::default()
        };
        let err = apply_set_args(base, &args).unwrap_err();
        let msg = error_message(&err);
        assert!(
            msg.contains(ERR_ALL_DISABLED),
            "all-disabled without --all-off must carry stable code {ERR_ALL_DISABLED}: {msg}"
        );
    }

    #[test]
    fn apply_set_args_all_off_with_flag_succeeds() {
        let base = WorkSchedule::defaults();
        let args = CronSetArgs {
            no_brainstorm: true,
            no_write: true,
            no_review: true,
            all_off: true,
            ..Default::default()
        };
        let out = apply_set_args(base, &args).expect("--all-off must permit all disabled");
        assert!(!out.roles.brainstorm.enabled);
        assert!(!out.roles.write.enabled);
        assert!(!out.roles.review.enabled);
    }

    fn error_message(err: &CliError) -> String {
        format!("{err}")
    }
}
