//! `creator works chronology` — per-Work auto-chronology control (V1.50 T-A P3).
//!
//! Implements the per-Work auto-chronology opt-in flag + manual advance CLI
//! surface. The daemon `auto_chronology_tick` task (5-min interval) performs
//! the automatic advance; this CLI surfaces set/show/advance.
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/auto-chronology.md` §2.2.
//!
//! ## Architecture
//!
//! `set`/`show` read and write `works.auto_chronology` directly via
//! `nexus-local-db` (foundation-slice precedent: `commands/creator/works::cron`).
//! `advance` calls `nexus_orchestration::auto_chronology::advance_manual` so the
//! outline render + atomic write + transactional seed + log entry reuse the
//! daemon-shared logic.

use std::fmt::Write as _;

use clap::Subcommand;

use crate::config::{self, CliConfig};
use crate::errors::{CliError, Result};

// ── Clap subcommand definitions (spec §2.2) ──────────────────────────────

/// `creator works chronology` subcommand group (spec §2.2).
#[derive(Debug, Subcommand)]
pub enum ChronologyCommand {
    /// Enable or disable auto-chronology for a Work (spec §2.2).
    Set {
        /// Work reference (`work_ref` slug) or `work_id` (`wrk_...`).
        work_ref: String,
        /// `--auto true` enables the daemon auto-advance; `--auto false`
        /// disables it (the default state for every shipped Work).
        #[arg(long)]
        auto: bool,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show the auto-chronology state + last advance for a Work (spec §2.2).
    Show {
        /// Work reference (`work_ref` slug) or `work_id` (`wrk_...`).
        work_ref: String,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Manually advance a Work to a volume, bypassing finish detection
    /// (spec §2.2 override path).
    Advance {
        /// Work reference (`work_ref` slug) or `work_id` (`wrk_...`).
        work_ref: String,
        /// Target volume number to advance to (e.g. `2`, `3`).
        #[arg(long)]
        volume: i32,
        /// Number of `not_started` chapter rows to seed for the new volume
        /// (optional; default 0 = placeholder outline, author seeds manually).
        #[arg(long)]
        chapters: Option<i32>,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

// ── Dispatcher + handlers ────────────────────────────────────────────────

/// Dispatch `creator works chronology` subcommands.
///
/// Opens the local `state.db` directly for `set`/`show` (foundation-slice
/// precedent: `creator works cron`). `advance` resolves the workspace root so
/// the outline/log writes land in the right `Works/<work_ref>/` tree.
///
/// # Errors
///
/// Returns [`CliError`] on DB open failure, work resolution failure, or
/// advance failure.
pub async fn handle_chronology(cmd: ChronologyCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .clone()
        .ok_or(CliError::CreatorNotSelected)?;
    let workspace_slug = config.workspace_slug_for_creator(&creator_id).to_string();

    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;

    match cmd {
        ChronologyCommand::Set {
            work_ref,
            auto,
            json,
        } => handle_set(&pool, &creator_id, &workspace_slug, &work_ref, auto, json).await,
        ChronologyCommand::Show { work_ref, json } => {
            handle_show(&pool, &creator_id, &workspace_slug, &work_ref, json).await
        }
        ChronologyCommand::Advance {
            work_ref,
            volume,
            chapters,
            json,
        } => {
            handle_advance(
                &pool,
                &creator_id,
                &workspace_slug,
                &work_ref,
                volume,
                chapters,
                json,
            )
            .await
        }
    }
}

/// `creator works chronology set` — persist the auto-chronology flag (§2.2).
async fn handle_set(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    work_ref: &str,
    auto: bool,
    json: bool,
) -> Result<()> {
    let work_id = resolve_work_id(pool, creator_id, workspace_slug, work_ref).await?;
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::works::set_auto_chronology(pool, &work_id, auto, &now).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "work_id": work_id,
                "auto_chronology": auto,
            }))?
        );
    } else {
        let state = if auto { "enabled" } else { "disabled" };
        println!("Auto-chronology {state} for Work {work_id} (ref: {work_ref})");
    }
    Ok(())
}

/// `creator works chronology show` — render the flag + last advance (§2.2).
async fn handle_show(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    work_ref: &str,
    json: bool,
) -> Result<()> {
    let work_id = resolve_work_id(pool, creator_id, workspace_slug, work_ref).await?;
    let auto = nexus_local_db::works::get_auto_chronology(pool, &work_id).await?;
    let last_advance = last_advance_label(work_ref);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "work_id": work_id,
                "auto_chronology": auto,
                "last_advance": last_advance,
            }))?
        );
    } else {
        let mut out = String::new();
        writeln!(out, "Work: {work_ref}").expect(WRITE_OK);
        writeln!(out, "auto_chronology: {auto}").expect(WRITE_OK);
        writeln!(out, "last advance:    {last_advance}").expect(WRITE_OK);
        print!("{out}");
    }
    Ok(())
}

/// `creator works chronology advance` — manual override (spec §2.2).
async fn handle_advance(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    work_ref: &str,
    volume: i32,
    chapters: Option<i32>,
    json: bool,
) -> Result<()> {
    let work_id = resolve_work_id(pool, creator_id, workspace_slug, work_ref).await?;
    let workspace_dir = config::find_workspace_root().ok_or_else(|| {
        CliError::Config(
            "Could not locate a Nexus workspace root (no `.nexus/` directory found \
             walking up from the current directory). Run `nexus42 creator works chronology \
             advance` from inside your workspace."
                .to_string(),
        )
    })?;

    let outcome = nexus_orchestration::auto_chronology::advance_manual(
        pool,
        &workspace_dir,
        &work_id,
        volume,
        chapters,
    )
    .await
    .map_err(|e| CliError::Other(format!("auto-chronology advance failed: {e}")))?;

    match outcome {
        nexus_orchestration::auto_chronology::AdvanceOutcome::Advanced {
            prev_volume,
            next_volume,
            chapters_seeded,
            ..
        } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "work_id": work_id,
                        "advanced": true,
                        "prev_volume": prev_volume,
                        "next_volume": next_volume,
                        "chapters_seeded": chapters_seeded,
                    }))?
                );
            } else {
                println!(
                    "Advanced Work {work_id} (ref: {work_ref}) to volume {next_volume} \
                     (from volume {prev_volume}); chapters seeded: {chapters_seeded}."
                );
                println!("  Outline: Works/{work_ref}/Outlines/volume-{next_volume}-outline.md");
                println!(
                    "  Fill the outline, then seed chapters if needed: \
                     `creator works chronology advance {work_ref} --volume {next_volume} \
                     --chapters <N>`."
                );
            }
        }
        nexus_orchestration::auto_chronology::AdvanceOutcome::Skipped { reason, .. } => {
            let reason_str = format!("{reason:?}");
            let note = match reason {
                nexus_orchestration::auto_chronology::SkipReason::AlreadyAdvanced => {
                    format!("volume {volume} outline already exists (idempotent skip)")
                }
                nexus_orchestration::auto_chronology::SkipReason::VolumeNotFinalized => {
                    "Work not found (no chapters / missing work)".to_string()
                }
                other => format!("skipped: {other:?}"),
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "work_id": work_id,
                        "advanced": false,
                        "reason": reason_str,
                        "note": note,
                    }))?
                );
            } else {
                println!("Did not advance Work {work_id}: {note}");
            }
        }
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// SAFETY label for infallible `writeln!`/`write!` into a `String` (no IO).
const WRITE_OK: &str = "write to String is infallible";

/// Resolve `<work_ref>` / `<work_id>` to a concrete `work_id`.
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

/// Best-effort label for the most recent chronology advance log entry.
///
/// Scans `Works/<work_ref>/Logs/chronology/` (when a workspace root is
/// discoverable) and returns the newest `<date>-advance-vol<N>.md` filename, or
/// `(none)` when there are no entries or the workspace cannot be found.
fn last_advance_label(work_ref: &str) -> String {
    let Some(ws) = config::find_workspace_root() else {
        return "(workspace not found)".to_string();
    };
    let dir = ws
        .join("Works")
        .join(work_ref)
        .join("Logs")
        .join("chronology");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return "(none)".to_string();
    };
    let mut latest: Option<String> = None;
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.contains("advance-vol") {
                let candidate = name.to_string();
                if latest.as_ref().is_some_and(|l| l >= &candidate) {
                    continue;
                }
                latest = Some(candidate);
            }
        }
    }
    latest.unwrap_or_else(|| "(none)".to_string())
}
