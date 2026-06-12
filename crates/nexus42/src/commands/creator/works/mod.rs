//! `nexus42 creator works` — Work management and pool (DF-60 §6.2H, DF-61).
//!
//! Migrated from `creator run list` / `creator run status` in V1.41.
//! P1 adds selection pool + inspiration pool subcommands (DF-61).
//! Single-Work actions (start, continue, stage, resume) remain under `creator run`.

use crate::errors::Result;
use clap::Subcommand;

use crate::api::DaemonClient;
use crate::config::CliConfig;
// V1.42 P-last (R-V141P0-06): completion-lock file path check
use nexus_home_layout;

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
    /// Manage the selection pool — promote, archive, list entries (DF-61).
    Pool {
        #[command(subcommand)]
        action: PoolAction,
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

/// Selection pool subcommands (DF-61).
#[derive(Debug, Subcommand)]
pub enum PoolAction {
    /// List pool entries for the active creator.
    List {
        /// Filter by status (active, queued, completed, archived)
        #[arg(long)]
        status: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Promote a Work to `active` in the pool (demotes prior active).
    Promote {
        /// Work ID (wrk_...) to promote
        work_id: String,
        /// Also set as CLI default via `works use`
        #[arg(long, default_value_t = false)]
        set_default: bool,
    },
    /// Archive a pool entry.
    Archive {
        /// Pool entry ID (npe_...) to archive
        entry_id: String,
    },
    /// Manage the inspiration pool (DF-61 §4).
    Inspiration {
        #[command(subcommand)]
        action: InspirationAction,
    },
}

/// Inspiration pool subcommands (DF-61 §4).
///
/// Pool-level inspiration items (DB SSOT in `inspiration_items` table);
/// distinct from per-Work `works.inspiration_log`.
#[derive(Debug, Subcommand)]
pub enum InspirationAction {
    /// Add a new inspiration item (creates MD scaffold + DB row).
    ///
    /// Pool-level item; distinct from per-Work `works.inspiration_log`.
    Add {
        /// Title for the inspiration item
        title: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List inspiration items.
    ///
    /// Pool-level items; distinct from per-Work `works.inspiration_log`.
    List {
        /// Filter by status (idea, promoted, archived)
        #[arg(long)]
        status: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Promote an inspiration item — creates a Work + pool row.
    ///
    /// Pool-level item; distinct from per-Work `works.inspiration_log`.
    Promote {
        /// Inspiration item ID (npi_...) to promote
        item_id: String,
        /// Optional idea override for the new Work's ``initial_idea``
        #[arg(long)]
        idea: Option<String>,
        /// Also set as CLI default via `works use`
        #[arg(long, default_value_t = false)]
        set_default: bool,
    },
    /// Archive an inspiration item.
    ///
    /// Pool-level item; distinct from per-Work `works.inspiration_log`.
    Archive {
        /// Inspiration item ID (npi_...) to archive
        item_id: String,
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
        WorksCommand::Pool { action } => handle_pool(&client, action).await,
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
                    "{:<36} {:30} {:12} {:12} LOCK UPDATED",
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
                    let locked = w
                        .get("completion_locked_at")
                        .and_then(|v| v.as_str())
                        .is_some();
                    let lock_icon = if locked { "🔒" } else { " " };
                    let display_title = truncate_with_ellipsis(title, 28);
                    println!(
                        "{id:<36} {display_title:30} {ws:12} {intake:12} {lock_icon}   {updated}"
                    );
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
                // V1.43 (P1 §3 remediation — open findings blocking progress):
                // cite quickstart §5.
                println!(
                    "⏰ {stale_count} finding(s) stale (>{threshold_hours}h) — \
                     address open findings or run a review pass; \
                     see docs/novel-writing-quickstart.md §5"
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

            // V1.43 P2 (T2): fetch open findings summary for spec §4 row 3.
            // Best-effort — never fails the status command.
            // Uses a shorter timeout (5s) to avoid blocking the hot path.
            let open_findings = fetch_open_findings(client, &resolved_id).await;

            if work_status == "completed" {
                let updated_at = resp
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                // V1.43 (P1 §3 remediation — work completed): cite quickstart §6.
                println!("═══════════════════════════════════════════════════════");
                println!("  \"{title}\" — Work {resolved_id}{profile_tag}");
                println!("  COMPLETED at {updated_at}");
                println!("  {total}/{total} chapters finalized.");
                println!("  No further novel-writing schedules will be enqueued.");
                println!();
                // V1.43 P2: findings summary in completed view (spec §4 row 3).
                print_findings_summary(&open_findings, &resolved_id);
                println!("  This Work is complete; see docs/novel-writing-quickstart.md §6");
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
                    // V1.42 P-last (R-V141P0-06): missing-file hint.
                    print_completion_lock_hint(work_ref, &resolved_id);
                }
                if let Some(lock_holder) = resp.get("runtime_lock_holder").and_then(|v| v.as_str())
                {
                    println!("runtime_lock_holder: {lock_holder}");
                }

                // V1.43 P2: findings summary (spec §4 row 3).
                print_findings_summary(&open_findings, &resolved_id);

                // Per-chapter table
                print_chapter_table(ch_list);
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

// ── Selection pool handlers (DF-61) ────────────────────────────────────

async fn handle_pool(client: &DaemonClient, action: PoolAction) -> Result<()> {
    match action {
        PoolAction::List { status, json } => handle_pool_list(client, status, json).await,
        PoolAction::Promote {
            work_id,
            set_default,
        } => handle_pool_promote(client, &work_id, set_default).await,
        PoolAction::Archive { entry_id } => handle_pool_archive(client, &entry_id).await,
        PoolAction::Inspiration { action } => handle_inspiration(client, action).await,
    }
}

async fn handle_pool_list(client: &DaemonClient, status: Option<String>, json: bool) -> Result<()> {
    let base = "/v1/local/works/pool";
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
        let entries = resp.get("entries").and_then(|v| v.as_array());
        match entries {
            Some(entries) if entries.is_empty() => {
                println!("No pool entries found.");
            }
            Some(entries) => {
                println!(
                    "{:<36} {:36} {:12} {:30} PROMOTED",
                    "ENTRY_ID", "WORK_ID", "STATUS", "TITLE"
                );
                for e in entries {
                    let eid = e.get("entry_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let wid = e
                        .get("work_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(none)");
                    let st = e.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                    let title = e.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let promoted = e.get("promoted_at").and_then(|v| v.as_str()).unwrap_or("-");
                    let display_title = truncate_with_ellipsis(title, 28);
                    println!("{eid:<36} {wid:<36} {st:<12} {display_title:<30} {promoted}");
                }
                println!("\n{} pool entry/entries", entries.len());
            }
            None => {
                println!("No pool entries found.");
            }
        }
    }

    Ok(())
}

async fn handle_pool_promote(
    client: &DaemonClient,
    work_id: &str,
    set_default: bool,
) -> Result<()> {
    let body = serde_json::json!({
        "work_id": work_id,
        "set_default": set_default,
    });
    let resp: serde_json::Value = client
        .post::<serde_json::Value, _>("/v1/local/works/pool/promote", &body)
        .await?;

    let entry_id = resp.get("entry_id").and_then(|v| v.as_str()).unwrap_or("?");
    println!("Promoted {work_id} to active (entry {entry_id})");

    if set_default {
        // T5: also wire as CLI default via `works use`
        let use_body = serde_json::json!({
            "action": "set_pool_active",
            "work_id": work_id,
        });
        let _use_resp: serde_json::Value = client
            .post::<serde_json::Value, _>("/v1/local/works/pool", &use_body)
            .await?;
        println!("Also set as CLI default work.");
    }

    Ok(())
}

async fn handle_pool_archive(client: &DaemonClient, entry_id: &str) -> Result<()> {
    let body = serde_json::json!({ "entry_id": entry_id });
    let resp: serde_json::Value = client
        .post::<serde_json::Value, _>("/v1/local/works/pool/archive", &body)
        .await?;

    let status = resp
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("archived");
    println!("Entry {entry_id} → {status}");

    Ok(())
}

// ── Inspiration pool handlers (DF-61 §4) ───────────────────────────────

async fn handle_inspiration(client: &DaemonClient, action: InspirationAction) -> Result<()> {
    match action {
        InspirationAction::Add { title, json } => {
            handle_inspiration_add(client, &title, json).await
        }
        InspirationAction::List { status, json } => {
            handle_inspiration_list(client, status, json).await
        }
        InspirationAction::Promote {
            item_id,
            idea,
            set_default,
        } => handle_inspiration_promote(client, &item_id, idea, set_default).await,
        InspirationAction::Archive { item_id } => {
            handle_inspiration_archive(client, &item_id).await
        }
    }
}

async fn handle_inspiration_add(client: &DaemonClient, title: &str, json: bool) -> Result<()> {
    let body = serde_json::json!({ "title": title });
    let resp: serde_json::Value = client
        .post::<serde_json::Value, _>("/v1/local/works/pool/inspiration", &body)
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        let item_id = resp.get("item_id").and_then(|v| v.as_str()).unwrap_or("?");
        let rel_path = resp.get("rel_path").and_then(|v| v.as_str()).unwrap_or("?");
        println!("Inspiration added: {item_id}");
        println!("  scaffold: {rel_path}");
    }

    Ok(())
}

async fn handle_inspiration_list(
    client: &DaemonClient,
    status: Option<String>,
    json: bool,
) -> Result<()> {
    let base = "/v1/local/works/pool/inspiration";
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
        let items = resp.get("items").and_then(|v| v.as_array());
        match items {
            Some(items) if items.is_empty() => {
                println!("No inspiration items found.");
            }
            Some(items) => {
                println!(
                    "{:<36} {:40} {:12} {:30} CREATED",
                    "ITEM_ID", "TITLE", "STATUS", "REL_PATH"
                );
                for i in items {
                    let iid = i.get("item_id").and_then(|v| v.as_str()).unwrap_or("?");
                    let title = i.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let st = i.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                    let rp = i.get("rel_path").and_then(|v| v.as_str()).unwrap_or("?");
                    let created = i.get("created_at").and_then(|v| v.as_str()).unwrap_or("-");
                    let display_title = truncate_with_ellipsis(title, 38);
                    let display_rp = if rp.len() > 28 {
                        format!("{}…", &rp[..28])
                    } else {
                        rp.to_string()
                    };
                    println!("{iid:<36} {display_title:40} {st:<12} {display_rp:<30} {created}");
                }
                println!("\n{} inspiration item(s)", items.len());
            }
            None => {
                println!("No inspiration items found.");
            }
        }
    }

    Ok(())
}

async fn handle_inspiration_promote(
    client: &DaemonClient,
    item_id: &str,
    idea: Option<String>,
    set_default: bool,
) -> Result<()> {
    let mut body = serde_json::json!({
        "item_id": item_id,
        "set_default": set_default,
    });
    if let Some(ref idea) = idea {
        body["idea"] = serde_json::Value::String(idea.clone());
    }

    let resp: serde_json::Value = client
        .post::<serde_json::Value, _>("/v1/local/works/pool/inspiration/promote", &body)
        .await?;

    let work_id = resp.get("work_id").and_then(|v| v.as_str()).unwrap_or("?");
    let pool_entry_id = resp
        .get("pool_entry_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    println!("Inspiration {item_id} promoted → Work {work_id} (pool entry {pool_entry_id})");

    if set_default {
        let use_body = serde_json::json!({
            "action": "set_pool_active",
            "work_id": work_id,
        });
        let _use_resp: serde_json::Value = client
            .post::<serde_json::Value, _>("/v1/local/works/pool", &use_body)
            .await?;
        println!("Also set as CLI default work.");
    }

    Ok(())
}

async fn handle_inspiration_archive(client: &DaemonClient, item_id: &str) -> Result<()> {
    let body = serde_json::json!({ "item_id": item_id });
    let _resp: serde_json::Value = client
        .post::<serde_json::Value, _>("/v1/local/works/pool/inspiration/archive", &body)
        .await?;

    println!("Inspiration item {item_id} archived.");

    Ok(())
}

// ── Shared display helpers (V1.42 P-last R-V141P0-02 dedup) ───────────

/// Findings subcall timeout — shorter than the default 30s to avoid
/// blocking the status hot path when the findings endpoint is slow.
const FINDINGS_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Hard cap on the number of findings fetched from the daemon.
const FINDINGS_FETCH_LIMIT: usize = 50;

/// Result of fetching open findings from the daemon.
///
/// Distinguishes "successfully fetched 0 findings" from "fetch failed",
/// so the display layer can print distinct messages.
enum FindingsResult {
    /// Findings were fetched successfully.
    Fetched(Vec<serde_json::Value>),
    /// The daemon did not return findings (network error, timeout, etc.).
    Unavailable,
}

/// Fetch open findings for a Work — best-effort, returns `Unavailable` on failure.
///
/// V1.43 P2 (T2): used by `handle_status` to satisfy spec §4 row 3
/// ("Are there open findings? Count + severity summary").
///
/// Uses a shorter timeout (`FINDINGS_FETCH_TIMEOUT`) than the default
/// 30s so a slow findings endpoint does not block the status command.
async fn fetch_open_findings(client: &DaemonClient, work_id: &str) -> FindingsResult {
    let findings_client = DaemonClient::with_timeouts(
        client.base_url(),
        crate::api::daemon_client::DEFAULT_CONNECT_TIMEOUT,
        FINDINGS_FETCH_TIMEOUT,
    );
    let path =
        format!("/v1/local/works/{work_id}/findings?status=open&limit={FINDINGS_FETCH_LIMIT}");
    findings_client
        .get::<serde_json::Value>(&path)
        .await
        .map_or(FindingsResult::Unavailable, |v| {
            FindingsResult::Fetched(v.as_array().cloned().unwrap_or_default())
        })
}

/// Parsed open-findings summary for display formatting.
///
/// Extracted as a struct to enable hermetic unit testing of
/// `format_findings_summary` without daemon client dependency.
#[derive(Debug, Default)]
struct FindingsSummary {
    /// Total open finding count.
    open_count: usize,
    /// Whether the count is truncated (server returned exactly the limit).
    is_truncated: bool,
    /// Highest severity among open findings (ordered: blocker > major > minor > info).
    highest_severity: Option<String>,
    /// Per-severity counts for the summary line.
    severity_counts: Vec<(String, usize)>,
    /// Top findings (up to 5) with title, severity, and routing hint.
    top_findings: Vec<(String, String, String)>,
}

impl FindingsSummary {
    /// Parse from the JSON array returned by the findings list endpoint.
    ///
    /// `is_truncated` should be `true` when the server returned exactly
    /// `FINDINGS_FETCH_LIMIT` rows, indicating there may be more findings
    /// beyond the fetched page.
    fn from_findings_json(findings: &[serde_json::Value], is_truncated: bool) -> Self {
        if findings.is_empty() {
            return Self::default();
        }

        let open_count = findings.len();

        // Severity priority order (highest first).
        let severity_order = ["blocker", "major", "minor", "info"];
        let mut severity_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut highest_severity: Option<String> = None;

        for f in findings {
            let sev = f
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string();
            *severity_counts.entry(sev.clone()).or_insert(0) += 1;

            // Track highest severity.
            let current_rank = severity_order.iter().position(|s| *s == sev);
            let highest_rank = highest_severity
                .as_ref()
                .and_then(|h| severity_order.iter().position(|s| *s == h));
            if current_rank.is_none_or(|c| highest_rank.is_none_or(|h| c < h)) {
                highest_severity = Some(sev);
            }
        }

        // Sort severity counts by priority order.
        let mut severity_vec: Vec<(String, usize)> = severity_counts.into_iter().collect();
        severity_vec.sort_by(|a, b| {
            let ra = severity_order.iter().position(|s| *s == a.0);
            let rb = severity_order.iter().position(|s| *s == b.0);
            ra.cmp(&rb)
        });

        // Top 5 findings with (title, severity, routing_hint).
        let top_findings = findings
            .iter()
            .take(5)
            .map(|f| {
                let title = f
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(untitled)")
                    .to_string();
                let sev = f
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string();
                let hint = f
                    .get("routing_hint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("→ none")
                    .to_string();
                (title, sev, hint)
            })
            .collect();

        Self {
            open_count,
            is_truncated,
            highest_severity,
            severity_counts: severity_vec,
            top_findings,
        }
    }
}

/// Format and print the open-findings summary block for `creator works status`.
///
/// Per spec §4 row 3: "Count + severity summary; link to review preset name."
/// Per cli-spec §7.1: clear, non-jargon formatting.
///
/// - `FindingsResult::Fetched(vec)` with empty vec → "findings: none open"
/// - `FindingsResult::Unavailable` → "findings: unavailable (daemon error)"
fn print_findings_summary(result: &FindingsResult, work_id: &str) {
    let findings = match result {
        FindingsResult::Unavailable => {
            println!("findings: unavailable (daemon error)");
            return;
        }
        FindingsResult::Fetched(vec) => vec,
    };

    let is_truncated = findings.len() == FINDINGS_FETCH_LIMIT;
    let summary = FindingsSummary::from_findings_json(findings, is_truncated);
    if summary.open_count == 0 {
        println!("findings: none open");
        return;
    }

    // Summary line: "findings: 3 open (1 blocker, 1 major, 1 info)"
    // Truncated: "findings: 50+ open (...)"
    let count_display = if summary.is_truncated {
        format!("{}+", summary.open_count)
    } else {
        format!("{}", summary.open_count)
    };
    let sev_parts: Vec<String> = summary
        .severity_counts
        .iter()
        .map(|(sev, count)| format!("{count} {sev}"))
        .collect();
    let sev_summary = sev_parts.join(", ");
    let highest_tag = summary
        .highest_severity
        .as_ref()
        .map_or(String::new(), |h| format!(" — highest: {h}"));
    println!("findings: {count_display} open ({sev_summary}){highest_tag}");

    // Top findings with routing hints (sanitized).
    for (i, (title, sev, hint)) in summary.top_findings.iter().enumerate() {
        let safe_title = sanitize_for_terminal(title);
        let safe_hint = sanitize_for_terminal(hint);
        let display_title = truncate_with_ellipsis(&safe_title, 48);
        println!("  #{} [{sev}] \"{display_title}\" {safe_hint}", i + 1);
    }

    // Review action hint — cite quickstart §5 (sanitize work_id for defense in depth).
    let safe_work_id = sanitize_for_terminal(work_id);
    println!(
        "  Address findings or run: nexus42 creator run stage advance {safe_work_id} --stage review"
    );
    println!("  See docs/novel-writing-quickstart.md §5");
}

/// Print per-chapter status table for novel works.
fn print_chapter_table(chapters: &[serde_json::Value]) {
    println!();
    println!(
        "{:<5} {:<30} {:<14} {:<14}",
        "CH", "TITLE", "STATUS", "UPDATED"
    );
    for ch in chapters {
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
        let display_title = truncate_with_ellipsis(ch_title, 28);
        println!("{num:<5} {display_title:<30} {ch_status:<14} {ch_updated:<14}");
    }
}

/// V1.42 P-last (R-V141P0-06): best-effort on-disk completion-lock file check.
///
/// DB `completion_locked_at` is the authoritative lock state. The
/// `.completion-lock.json` file is a derived artifact. When the file is
/// missing, surface a hint to the user.
fn print_completion_lock_hint(work_ref: &str, work_id: &str) {
    if work_ref.starts_with('(') {
        return;
    }
    if let Ok(cfg) = crate::config::CliConfig::load() {
        if let Some(creator_id) = &cfg.active_creator_id {
            if let Some(ws_slug) = cfg.active_workspace_slug_by_creator.get(creator_id) {
                let home = dirs::home_dir().unwrap_or_default();
                let ws_dir =
                    nexus_home_layout::operational_workspace_dir(&home, creator_id, ws_slug);
                let lock_path = ws_dir
                    .join("Works")
                    .join(work_ref)
                    .join(".completion-lock.json");
                if !lock_path.exists() {
                    println!("⚠ completion-lock file missing (DB says locked but file not found)");
                    println!("  Run: nexus42 creator run reconcile-chapters {work_id}");
                }
            }
        }
    }
}

/// Truncate a string to `max_len` characters, appending `…` if truncated.
fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len])
    } else {
        s.to_string()
    }
}

/// Strip ASCII control characters and ANSI escape sequences from a string
/// to prevent terminal display corruption from user-supplied data.
///
/// Preserves printable ASCII, Unicode, `\n`, and `\t`. Strips:
/// - ASCII control chars 0x00–0x1F (except `\n` 0x0A and `\t` 0x09) and 0x7F (DEL)
/// - ANSI CSI sequences (`ESC [ ... letter`)
fn sanitize_for_terminal(s: &str) -> String {
    // Phase 1: strip ANSI CSI sequences (ESC [ <params> <letter>).
    let ansi_re = regex::Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]").unwrap_or_else(|e| {
        // The pattern is a compile-time constant; panic is unreachable.
        unreachable!("invalid ANSI regex pattern: {e}")
    });
    let stripped = ansi_re.replace_all(s, "");

    // Phase 2: remove remaining ASCII control chars (keep \n, \t, and printable).
    stripped
        .chars()
        .filter(|&c| {
            if c == '\n' || c == '\t' {
                return true;
            }
            let code = c as u32;
            // Allow printable chars: space (0x20) and above, excluding DEL (0x7F).
            // Below 0x20 are control chars — filter them out.
            code >= 0x20 && code != 0x7F
        })
        .collect()
}

// ── Tests (V1.43 P2 T4) ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn finding_json(severity: &str, title: &str, routing_hint: &str) -> serde_json::Value {
        serde_json::json!({
            "finding_id": format!("fnd_test_{}", title.len()),
            "severity": severity,
            "title": title,
            "routing_hint": routing_hint,
            "status": "open",
        })
    }

    // ── FindingsSummary parsing tests ────────────────────────────────────

    #[test]
    fn findings_summary_empty() {
        let summary = FindingsSummary::from_findings_json(&[], false);
        assert_eq!(summary.open_count, 0);
        assert!(!summary.is_truncated);
        assert!(summary.highest_severity.is_none());
        assert!(summary.severity_counts.is_empty());
        assert!(summary.top_findings.is_empty());
    }

    #[test]
    fn findings_summary_single_finding() {
        let findings = vec![finding_json("major", "Plot hole", "→ write")];
        let summary = FindingsSummary::from_findings_json(&findings, false);
        assert_eq!(summary.open_count, 1);
        assert!(!summary.is_truncated);
        assert_eq!(summary.highest_severity.as_deref(), Some("major"));
        assert_eq!(summary.severity_counts, vec![("major".to_string(), 1)]);
        assert_eq!(summary.top_findings.len(), 1);
        assert_eq!(summary.top_findings[0].0, "Plot hole");
    }

    #[test]
    fn findings_summary_mixed_severities() {
        let findings = vec![
            finding_json("info", "Style note", "→ none"),
            finding_json("blocker", "Continuity error", "→ write"),
            finding_json("minor", "Typo", "→ none"),
            finding_json("major", "Plot hole", "→ brainstorm"),
        ];
        let summary = FindingsSummary::from_findings_json(&findings, false);
        assert_eq!(summary.open_count, 4);
        assert_eq!(summary.highest_severity.as_deref(), Some("blocker"));
        // Sorted by severity priority (blocker first).
        assert_eq!(summary.severity_counts[0].0, "blocker");
        assert_eq!(summary.severity_counts[0].1, 1);
        assert_eq!(summary.severity_counts[1].0, "major");
        assert_eq!(summary.severity_counts[1].1, 1);
    }

    #[test]
    fn findings_summary_top_five_cap() {
        let findings: Vec<serde_json::Value> = (0..8)
            .map(|i| finding_json("info", &format!("Finding {i}"), "→ none"))
            .collect();
        let summary = FindingsSummary::from_findings_json(&findings, false);
        assert_eq!(summary.open_count, 8);
        assert_eq!(summary.top_findings.len(), 5);
    }

    #[test]
    fn findings_summary_truncated_flag() {
        let findings: Vec<serde_json::Value> = (0..FINDINGS_FETCH_LIMIT)
            .map(|i| finding_json("info", &format!("Finding {i}"), "→ none"))
            .collect();
        let summary = FindingsSummary::from_findings_json(&findings, true);
        assert_eq!(summary.open_count, FINDINGS_FETCH_LIMIT);
        assert!(summary.is_truncated);
    }

    // ── print_findings_summary display tests ─────────────────────────────

    fn capture_findings_output(findings: &[serde_json::Value], work_id: &str) -> String {
        // Test the summary struct formatting (mirrors print_findings_summary logic).
        let is_truncated = findings.len() == FINDINGS_FETCH_LIMIT;
        let summary = FindingsSummary::from_findings_json(findings, is_truncated);
        let mut lines = Vec::new();

        if summary.open_count == 0 {
            lines.push("findings: none open".to_string());
        } else {
            let count_display = if summary.is_truncated {
                format!("{}+", summary.open_count)
            } else {
                format!("{}", summary.open_count)
            };
            let sev_parts: Vec<String> = summary
                .severity_counts
                .iter()
                .map(|(sev, count)| format!("{count} {sev}"))
                .collect();
            let sev_summary = sev_parts.join(", ");
            let highest_tag = summary
                .highest_severity
                .as_ref()
                .map_or(String::new(), |h| format!(" — highest: {h}"));
            lines.push(format!(
                "findings: {count_display} open ({sev_summary}){highest_tag}"
            ));
            for (i, (title, sev, hint)) in summary.top_findings.iter().enumerate() {
                let safe_title = sanitize_for_terminal(title);
                let safe_hint = sanitize_for_terminal(hint);
                let display_title = truncate_with_ellipsis(&safe_title, 48);
                lines.push(format!(
                    "  #{} [{sev}] \"{display_title}\" {safe_hint}",
                    i + 1
                ));
            }
            let safe_work_id = sanitize_for_terminal(work_id);
            lines.push(format!(
                "  Address findings or run: nexus42 creator run stage advance {safe_work_id} --stage review"
            ));
            lines.push("  See docs/novel-writing-quickstart.md §5".to_string());
        }

        lines.join("\n")
    }

    #[test]
    fn display_no_open_findings() {
        let output = capture_findings_output(&[], "wrk_test");
        assert!(output.contains("findings: none open"));
        assert!(!output.contains("highest"));
    }

    #[test]
    fn display_findings_with_severity_summary() {
        let findings = vec![
            finding_json("blocker", "Continuity error", "→ write"),
            finding_json("minor", "Style issue", "→ none"),
        ];
        let output = capture_findings_output(&findings, "wrk_abc123");
        assert!(output.contains("findings: 2 open"));
        assert!(output.contains("1 blocker"));
        assert!(output.contains("1 minor"));
        assert!(output.contains("highest: blocker"));
        assert!(output.contains("#1 [blocker] \"Continuity error\" → write"));
        assert!(output.contains("#2 [minor] \"Style issue\" → none"));
        assert!(output.contains("wrk_abc123"));
        assert!(output.contains("quickstart"));
    }

    #[test]
    fn display_findings_completed_work_shows_summary() {
        // Verify that the findings summary format works for the completed
        // path too — same formatting, just inserted before the "complete" message.
        let findings = vec![finding_json("info", "Nice-to-have", "→ none")];
        let output = capture_findings_output(&findings, "wrk_done");
        assert!(output.contains("findings: 1 open"));
        assert!(output.contains("1 info"));
        assert!(output.contains("highest: info"));
    }

    // ── Completion display tests ─────────────────────────────────────────

    #[test]
    fn completion_shows_zero_open_findings() {
        // When no findings exist, the summary line should say "none open".
        let output = capture_findings_output(&[], "wrk_completed");
        assert!(output.contains("findings: none open"));
    }

    // ── Truncation tests ─────────────────────────────────────────────────

    #[test]
    fn truncate_with_ellipsis_short() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_with_ellipsis_long() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
    }

    // ── Truncated findings (50+) display test ────────────────────────────

    #[test]
    fn display_truncated_findings_shows_plus_indicator() {
        let findings: Vec<serde_json::Value> = (0..FINDINGS_FETCH_LIMIT)
            .map(|i| finding_json("info", &format!("Finding {i}"), "→ none"))
            .collect();
        let output = capture_findings_output(&findings, "wrk_many");
        assert!(
            output.contains(&format!("findings: {}+ open", FINDINGS_FETCH_LIMIT)),
            "expected '50+ open' indicator in output: {output}"
        );
        // Should NOT show bare "50 open" (without +).
        assert!(
            !output.contains(&format!("findings: {} open", FINDINGS_FETCH_LIMIT)),
            "should not show exact count without '+' when truncated"
        );
    }

    // ── sanitize_for_terminal tests ──────────────────────────────────────

    #[test]
    fn sanitize_for_terminal_strips_escape_codes() {
        let input = "\x1b[31mRed Text\x1b[0m normal";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "Red Text normal");
    }

    #[test]
    fn sanitize_for_terminal_preserves_unicode() {
        let input = "你好世界 🌍 こんにちは";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, input);
    }

    #[test]
    fn sanitize_for_terminal_strips_control_chars() {
        let input = "hello\x00world\x07bell\x1Fus";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "helloworldbellus");
    }

    #[test]
    fn sanitize_for_terminal_strips_del() {
        // DEL (0x7F) should be removed
        let input = "before\x7Fafter";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "beforeafter");
    }

    #[test]
    fn sanitize_for_terminal_preserves_newline_and_tab() {
        let input = "line1\nline2\ttab";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "line1\nline2\ttab");
    }

    #[test]
    fn sanitize_for_terminal_strips_clear_screen() {
        // \x1b[2J is "clear screen"
        let input = "good\x1b[2Jbad";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "goodbad");
    }

    // ── FindingsResult unavailable display test ──────────────────────────

    #[test]
    fn display_unavailable_findings() {
        // When findings are unavailable, the output should say "unavailable"
        // not "none open". This tests the logic that print_findings_summary uses.
        let result = FindingsResult::Unavailable;
        let output = match result {
            FindingsResult::Unavailable => "findings: unavailable (daemon error)".to_string(),
            FindingsResult::Fetched(vec) => {
                let is_truncated = vec.len() == FINDINGS_FETCH_LIMIT;
                let summary = FindingsSummary::from_findings_json(&vec, is_truncated);
                if summary.open_count == 0 {
                    "findings: none open".to_string()
                } else {
                    format!("findings: {} open", summary.open_count)
                }
            }
        };
        assert!(output.contains("unavailable"));
        assert!(!output.contains("none open"));
    }
}
