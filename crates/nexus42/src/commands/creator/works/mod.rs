//! `nexus42 creator works` — Work management and pool (DF-60 §6.2H).
//!
//! Migrated from `creator run list` / `creator run status` in V1.41.
//! Single-Work actions (start, continue, stage, resume) remain under `creator run`.

use crate::errors::Result;
use clap::Subcommand;

use crate::api::DaemonClient;
use crate::config::CliConfig;

/// Work management subcommands (DF-60 §6.2H).
#[derive(Debug, Subcommand)]
pub enum WorksCommand {
    /// List all Works for the active creator.
    ///
    /// Migrated from `creator run list` (V1.41).
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show details of a single Work.
    ///
    /// When `<work_id>` is omitted, resolves the pool `active` Work.
    /// Migrated from `creator run status` (V1.41).
    Status {
        /// Work ID (wrk_...). Omit to use pool active Work.
        work_id: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Set pool `active` row → CLI default `work_id` (DF-60 §1.1).
    ///
    /// Does NOT pause other Works. Future `creator run` commands that
    /// accept optional `--work-id` will default to this Work.
    Use {
        /// Work ID (wrk_...) to set as active
        work_id: String,
    },
    /// Manage the completion lock for a Work (DF-60 §3.2).
    CompletionLock {
        #[command(subcommand)]
        command: CompletionLockCommand,
    },
}

/// Completion lock subcommands.
#[derive(Debug, Subcommand)]
pub enum CompletionLockCommand {
    /// Release `.completion-lock.json` for a Work.
    ///
    /// After release, `creator run resume --reopen` can be used on the Work.
    Release {
        /// Work ID (wrk_...) to release the completion lock for
        work_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Dispatch `creator works` subcommands.
///
/// # Errors
///
/// Returns an error if the daemon API call fails.
pub async fn handle_works(cmd: WorksCommand, config: &CliConfig) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    match cmd {
        WorksCommand::List { status, json } => handle_list(&client, status, json).await,
        WorksCommand::Status { work_id, json } => handle_status(&client, work_id, json).await,
        WorksCommand::Use { work_id } => handle_use(&client, &work_id).await,
        WorksCommand::CompletionLock { command } => handle_completion_lock(&client, command).await,
    }
}

async fn handle_list(client: &DaemonClient, status: Option<String>, json: bool) -> Result<()> {
    // Build query via url::Url to properly encode the status filter value.
    let base = "/v1/local/works";
    let path = status.as_ref().map_or_else(
        || base.to_string(),
        |s| {
            let mut url = url::Url::parse("http://localhost").expect("valid base");
            url.set_path(base);
            url.query_pairs_mut().append_pair("status", s);
            let q = url.query().unwrap_or("");
            format!("{base}?{q}")
        },
    );

    let resp: serde_json::Value = client.get::<serde_json::Value>(&path).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        let works = resp.get("works").and_then(|v| v.as_array());
        match works {
            Some(works) if works.is_empty() => {
                println!("No works found.");
            }
            Some(works) => {
                println!(
                    "{:<36} {:30} {:12} {:12} UPDATED",
                    "WORK_ID", "TITLE", "STATUS", "INTAKE"
                );
                for w in works {
                    let id = w.get("work_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let title = w.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let ws = w.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                    let intake = w
                        .get("intake_status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let updated = w.get("updated_at").and_then(|v| v.as_str()).unwrap_or("?");
                    let display_title = if title.len() > 28 {
                        format!("{}…", &title[..28])
                    } else {
                        title.to_string()
                    };
                    println!("{id:<36} {display_title:30} {ws:12} {intake:12} {updated}");
                }
                println!("\n{} work(s)", works.len());
            }
            None => {
                println!("No works found.");
            }
        }
    }

    Ok(())
}

// Migrated from run.rs — preserved status display logic with DF-60 extensions.
#[allow(clippy::too_many_lines)]
async fn handle_status(client: &DaemonClient, work_id: Option<String>, json: bool) -> Result<()> {
    // Resolve work_id: if omitted, try to get the pool active Work.
    let resolved_id = if let Some(id) = work_id {
        id
    } else {
        // Try pool active Work endpoint.
        let resp: serde_json::Value = client
            .get::<serde_json::Value>("/v1/local/works?limit=1&status=active")
            .await?;
        resp.get("works")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|w| w.get("work_id"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                crate::errors::CliError::Config(
                    "No active Work found. Specify <work_id> or run `nexus42 creator works use <work_id>`.".to_string(),
                )
            })?
    };

    // R-V139P1-W-3: DaemonClient already enforces DEFAULT_REQUEST_TIMEOUT
    // (30s) on every request; no unbounded wait is possible.
    let resp: serde_json::Value = client
        .get::<serde_json::Value>(&format!("/v1/local/works/{resolved_id}"))
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        // V1.39 P4 T3: stale findings banner — best-effort, never
        // fails the status command.
        if let Ok(stale) = client
            .get::<serde_json::Value>("/v1/local/findings/stale")
            .await
        {
            let stale_count = stale
                .get("stale_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let threshold_secs = stale
                .get("threshold_seconds")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(96 * 60 * 60);
            if stale_count > 0 {
                let threshold_hours = threshold_secs / 3600;
                println!(
                    "⏰ {stale_count} finding(s) stale (>{threshold_hours}h) — run: nexus42 creator run schedule add --preset novel-review-master --work-id {resolved_id}"
                );
                println!();
            }
        }

        let work_status = resp
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("(not set)");
        let title = resp
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(untitled)");
        let work_profile = resp.get("work_profile").and_then(|v| v.as_str());
        let work_ref = resp
            .get("work_ref")
            .and_then(|v| v.as_str())
            .unwrap_or("(no ref)");
        let intake_status = resp
            .get("intake_status")
            .and_then(|v| v.as_str())
            .unwrap_or("(not set)");
        let current_chapter = resp
            .get("current_chapter")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let total_planned = resp
            .get("total_planned_chapters")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let chapters = resp.get("chapters").and_then(|v| v.as_array());

        // V1.38 P0 (T8): per-chapter status UX per spec §8.1.
        // For novel profile works, show chapter-centric output.
        if let (Some("novel"), Some(ch_list)) = (work_profile, chapters) {
            let finalized_count = ch_list
                .iter()
                .filter(|c| c.get("status").and_then(|v| v.as_str()) == Some("finalized"))
                .count();
            let total = ch_list.len();

            let profile_tag = " (novel)".to_string();

            if work_status == "completed" {
                let updated_at = resp
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                println!("═══════════════════════════════════════════════════════");
                println!("  \"{title}\" — Work {resolved_id}{profile_tag}");
                println!("  COMPLETED at {updated_at}");
                println!("  {total}/{total} chapters finalized.");
                println!("  No further novel-writing schedules will be enqueued.");
                println!();
                println!("  To start a new Work, run:");
                println!("    nexus42 creator run start \\");
                println!("      --init-preset novel-project-init --idea \"...\"");
                println!("═══════════════════════════════════════════════════════");
            } else {
                // Header
                println!("Work: {resolved_id} — {title}{profile_tag}");
                println!("work_ref: {work_ref}");
                println!("intake: {intake_status}");
                println!("progress: {finalized_count} / {total} chapters finalized");
                println!("current_chapter: {current_chapter}");
                println!("total_planned_chapters: {total_planned}");

                // V1.39 T7: auto-chain checkpoint fields
                let auto_chain = resp
                    .get("auto_chain_enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                let driver = resp
                    .get("driver_schedule_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none");
                let interrupted = resp
                    .get("auto_chain_interrupted")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);

                println!("auto_chain_enabled: {auto_chain}");
                println!("driver_schedule_id: {driver}");
                if interrupted {
                    println!("auto_chain_interrupted: true (use `creator run resume`)");
                }

                // V1.41: completion lock fields (DF-60 §6.2H)
                if let Some(completion_status) =
                    resp.get("novel_completion_status").and_then(|v| v.as_str())
                {
                    println!("completion_status: {completion_status}");
                }
                if let Some(locked_at) = resp.get("completion_locked_at").and_then(|v| v.as_str()) {
                    println!("completion_locked_at: {locked_at}");
                }
                if let Some(lock_holder) = resp.get("runtime_lock_holder").and_then(|v| v.as_str())
                {
                    println!("runtime_lock_holder: {lock_holder}");
                }

                // Per-chapter table
                println!();
                println!(
                    "{:<5} {:<30} {:<14} {:<14}",
                    "CH", "TITLE", "STATUS", "UPDATED"
                );
                for ch in ch_list {
                    let num = ch
                        .get("chapter_number")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    let ch_title = ch
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(untitled)");
                    let ch_status = ch.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                    let ch_updated = ch.get("updated_at").and_then(|v| v.as_str()).unwrap_or("?");
                    let display_title = if ch_title.len() > 28 {
                        format!("{}…", &ch_title[..28])
                    } else {
                        ch_title.to_string()
                    };
                    println!("{num:<5} {display_title:<30} {ch_status:<14} {ch_updated:<14}");
                }
            }
        } else {
            // Non-novel or generic work display
            println!("Work: {resolved_id} — {title}");
            println!("status: {work_status}");
            println!("work_ref: {work_ref}");
            println!("intake: {intake_status}");

            // Show all remaining key-value pairs
            let skip_keys = [
                "work_id",
                "title",
                "status",
                "work_ref",
                "intake_status",
                "chapters",
                "work_profile",
            ];
            if let Some(obj) = resp.as_object() {
                for (key, val) in obj {
                    if skip_keys.contains(&key.as_str()) {
                        continue;
                    }
                    if val.is_null() {
                        continue;
                    }
                    let label = key.replace('_', " ");
                    let val = if val.is_string() {
                        val.as_str().unwrap_or("(invalid)").to_string()
                    } else {
                        format!("{val}")
                    };
                    println!("{label:>20}: {val}");
                }
            }
        }
    }

    Ok(())
}

async fn handle_use(client: &DaemonClient, work_id: &str) -> Result<()> {
    // Verify the work exists first.
    let _work: serde_json::Value = client
        .get::<serde_json::Value>(&format!("/v1/local/works/{work_id}"))
        .await?;

    // Set pool active via the works API. The daemon handler will
    // demote any current `active` entry and promote this one.
    let body = serde_json::json!({
        "action": "set_pool_active",
        "work_id": work_id,
    });
    let resp: serde_json::Value = client
        .post::<serde_json::Value, _>("/v1/local/works/pool", &body)
        .await?;

    println!(
        "Active Work set to {work_id} ({})",
        resp.get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(untitled)")
    );

    Ok(())
}

async fn handle_completion_lock(client: &DaemonClient, cmd: CompletionLockCommand) -> Result<()> {
    match cmd {
        CompletionLockCommand::Release { work_id, json } => {
            let body = serde_json::json!({
                "action": "release_completion_lock",
                "work_id": work_id,
            });
            let resp: serde_json::Value = client
                .post::<serde_json::Value, _>(
                    &format!("/v1/local/works/{work_id}/completion-lock/release"),
                    &body,
                )
                .await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                println!("Completion lock released for Work {work_id}.");
                println!(
                    "You can now use `nexus42 creator run resume --reopen --reason \"...\" {work_id}`"
                );
            }
        }
    }

    Ok(())
}
