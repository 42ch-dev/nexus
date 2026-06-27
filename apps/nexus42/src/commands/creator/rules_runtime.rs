//! `creator works findings …` and `creator works rules …` handlers
//! (V1.48 P2 — `AGENTS.md` Layer 2 runtime).
//!
//! These subcommands operate on the Work's Layer 2 file
//! `Works/<work_ref>/AGENTS.md`:
//!
//! - `findings accept <finding_id>` — append a finding's `rule_suggestion`
//!   to `AGENTS.md` and mark the finding `resolved` (overlay §3.2).
//! - `rules reset [<work_id>]` — restore the default `AGENTS.md` scaffold
//!   (overlay §4).
//!
//! The file-mutation logic lives in
//! [`nexus_orchestration::rules_layers`](../../../../../nexus_orchestration/rules_layers/index.html)
//! so it is hermetically testable without a daemon. This module is the
//! thin CLI orchestration layer that resolves IDs and workspace paths,
//! calls the daemon API for finding/work data, and invokes the pure
//! helpers.
//!
//! Spec refs:
//! - [archived/knowledge/novel-findings-maturity.md §3 / §4](../../../../../.mstar/archived/knowledge/novel-findings-maturity.md)
//! - [novel-writing/workflow-profile.md §5.5.4](../../../../../.mstar/knowledge/specs/novel-writing/workflow-profile.md)

use std::io::IsTerminal;

use serde::Deserialize;

use crate::api::DaemonClient;
use crate::commands::creator::work_utils::resolve_active_work_id;
use crate::commands::creator::works::{FindingsCommand, RulesCommand};
use crate::errors::{CliError, Result};

/// Subset of the daemon `GET /v1/local/works/{work_id}` payload that this
/// module needs. Deserializing via `serde_json::Value` keeps the CLI
/// decoupled from the daemon DTO crate.
#[derive(Debug, Deserialize)]
struct WorkRefResponse {
    work_ref: Option<String>,
}

/// Subset of the daemon `GET /v1/local/findings/{finding_id}` payload.
#[derive(Debug, Deserialize)]
struct FindingResponse {
    work_id: String,
    status: String,
    #[serde(default)]
    rule_suggestion: Option<String>,
}

/// Handle `creator works findings …` (V1.48 P2).
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on daemon API failure, missing
/// `rule_suggestion`, or filesystem write error.
pub async fn handle_findings(client: &DaemonClient, command: FindingsCommand) -> Result<()> {
    match command {
        FindingsCommand::Accept { finding_id, json } => {
            handle_findings_accept(client, &finding_id, json).await
        }
        FindingsCommand::Prune {
            older_than_days,
            dry_run,
            json,
        } => handle_findings_prune(client, older_than_days, dry_run, json).await,
    }
}

/// Handle `creator works rules …` (V1.48 P2).
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on daemon API failure, missing
/// `work_ref`, or filesystem write error.
pub async fn handle_rules(client: &DaemonClient, command: RulesCommand) -> Result<()> {
    match command {
        RulesCommand::Reset {
            work_id,
            dry_run,
            yes,
            json,
        } => handle_rules_reset(client, work_id, dry_run, yes, json).await,
    }
}

/// `creator works findings accept <finding_id>` (overlay §3.2).
///
/// Steps:
/// 1. GET finding (creator-scoped) → must have non-empty `rule_suggestion`.
/// 2. GET the Work → resolve `work_ref`.
/// 3. Resolve the operational workspace dir from CLI config.
/// 4. Append the rule suggestion to `Works/<work_ref>/AGENTS.md`
///    (idempotent on `finding_id`).
/// 5. PATCH the finding `status=resolved`.
async fn handle_findings_accept(client: &DaemonClient, finding_id: &str, json: bool) -> Result<()> {
    // 1. Fetch the finding (creator-scoped endpoint, V1.48 P2).
    let path = format!("/v1/local/findings/{finding_id}");
    let finding: FindingResponse = client.get(&path).await?;

    // 2. Validate rule_suggestion is present and non-empty.
    let rule_text = finding
        .rule_suggestion
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            CliError::Config(format!(
                "Finding {finding_id} has no `rule_suggestion`; nothing to accept. \
                 Use `nexus42 creator works findings ...` to set one first."
            ))
        })?;

    // 3. Resolve work_ref from the Work record.
    let work_path = format!("/v1/local/works/{}", finding.work_id);
    let work: WorkRefResponse = client.get(&work_path).await?;
    let work_ref = work.work_ref.as_deref().ok_or_else(|| {
        CliError::Config(format!(
            "Work {} has no `work_ref`; cannot locate `AGENTS.md`. \
             Re-run `nexus42 creator bootstrap` or set work_ref.",
            finding.work_id
        ))
    })?;

    // 4. Resolve the operational workspace dir.
    let ws_dir = operational_workspace_dir_or_error()?;
    let agents_md_path = nexus_home_layout::work_agents_md_path(&ws_dir, work_ref);

    // 5. Append (idempotent on finding_id).
    let timestamp = chrono::Utc::now().to_rfc3339();
    let outcome = nexus_orchestration::rules_layers::append_rule_suggestion(
        &agents_md_path,
        work_ref,
        finding_id,
        rule_text,
        &timestamp,
    )
    .map_err(|e| {
        CliError::Other(format!(
            "Failed to append to {}: {e}",
            agents_md_path.display()
        ))
    })?;

    // 6. PATCH finding status=resolved (idempotent — skip if already resolved
    //    and the append was a no-op, to avoid a redundant round-trip).
    let already_resolved = finding.status == "resolved";
    let resolved_now = if already_resolved {
        false
    } else {
        patch_finding_resolved(client, &finding.work_id, finding_id).await?;
        true
    };

    if json {
        let appended = matches!(
            outcome,
            nexus_orchestration::rules_layers::AppendOutcome::Appended
        );
        let body = serde_json::json!({
            "finding_id": finding_id,
            "work_id": finding.work_id,
            "work_ref": work_ref,
            "agents_md_path": agents_md_path.to_string_lossy(),
            "appended": appended,
            "resolved_now": resolved_now,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        let agents_md_rel = std::path::Path::new("Works")
            .join(work_ref)
            .join("AGENTS.md");
        match outcome {
            nexus_orchestration::rules_layers::AppendOutcome::Appended => {
                println!(
                    "✓ Appended rule suggestion from finding {finding_id} to {rel}",
                    rel = agents_md_rel.display()
                );
            }
            nexus_orchestration::rules_layers::AppendOutcome::AlreadyPresent => {
                println!(
                    "• Finding {finding_id} already recorded in {rel} (idempotent — no change)",
                    rel = agents_md_rel.display()
                );
            }
        }
        if resolved_now {
            println!("✓ Marked finding {finding_id} as resolved");
        } else if already_resolved {
            println!("• Finding {finding_id} was already resolved");
        }
    }
    Ok(())
}

/// `creator works findings prune [--older-than <days>] [--dry-run]`
/// (`novel-writing/quality-loop.md` §9.4).
///
/// Calls `POST /v1/local/findings/prune` and reports the deleted (or, in
/// dry-run, would-be-deleted) count. `resolved` findings older than the
/// retention window are eligible; `open` and `wont_fix` are never touched.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on daemon API failure.
async fn handle_findings_prune(
    client: &DaemonClient,
    older_than_days: i64,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let path =
        format!("/v1/local/findings/prune?older_than_days={older_than_days}&dry_run={dry_run}");
    let resp: serde_json::Value = client.post(&path, &serde_json::json!({})).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&resp).unwrap_or_default()
        );
        return Ok(());
    }

    let count = resp
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let resp_dry_run = resp
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(dry_run);
    let days = resp
        .get("older_than_days")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(older_than_days);
    if resp_dry_run {
        if count == 0 {
            println!("• No resolved findings older than {days} days to prune (dry-run).");
        } else {
            println!(
                "Dry run — {count} resolved finding(s) older than {days} day(s) \
                 would be pruned. Re-run without --dry-run to delete."
            );
        }
    } else if count == 0 {
        println!("• No resolved findings older than {days} days to prune; nothing deleted.");
    } else {
        println!("✓ Pruned {count} resolved finding(s) older than {days} day(s).");
    }
    Ok(())
}

/// Resolve the operational workspace dir or return a helpful error.
fn operational_workspace_dir_or_error() -> Result<std::path::PathBuf> {
    super::works::operational_workspace_dir_from_config_public().ok_or_else(|| {
        CliError::Config(
            "Could not resolve the operational workspace directory from CLI config. \
             Run `nexus42 creator workspace init` and ensure an active creator/workspace \
             is set."
                .to_string(),
        )
    })
}

// T4: rules reset handler below.

/// `creator works rules reset [<work_id>]` (overlay §4).
///
/// Restores `Works/<work_ref>/AGENTS.md` to the default scaffold. Does NOT
/// delete the Work or any chapter artifacts.
///
/// # Flags (V1.48 P2-fix1)
///
/// - `--dry-run`: print a unified diff of the pending change and exit WITHOUT
///   writing. No confirmation prompt is shown. Takes precedence over `--yes`.
/// - `--yes` (or `-y`): skip the confirmation prompt and write immediately
///   (matches the `apt-get -y` / `pacman --noconfirm` convention).
/// - Default (neither flag): print the diff, then prompt for confirmation
///   before overwriting. Non-interactive stdin without `--yes` is an error.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on daemon API failure, missing
/// `work_ref`, non-interactive confirmation, or filesystem write error.
// Linear 6-phase CLI dispatch (resolve → snapshot → dry-run → no-op →
// confirm → write) reported in JSON + human form. Splitting it would create a
// `too_many_arguments` helper or a single-use context struct; neither reduces
// real complexity. Phase boundaries are marked by section comments below.
#[allow(clippy::too_many_lines)]
async fn handle_rules_reset(
    client: &DaemonClient,
    work_id: Option<String>,
    dry_run: bool,
    yes: bool,
    json: bool,
) -> Result<()> {
    let resolved_work_id = resolve_active_work_id(client, work_id).await?;

    // Resolve work_ref from the Work record.
    let work_path = format!("/v1/local/works/{resolved_work_id}");
    let work: WorkRefResponse = client.get(&work_path).await?;
    let work_ref = work.work_ref.as_deref().ok_or_else(|| {
        CliError::Config(format!(
            "Work {resolved_work_id} has no `work_ref`; cannot locate `AGENTS.md`."
        ))
    })?;

    let ws_dir = operational_workspace_dir_or_error()?;
    let agents_md_path = nexus_home_layout::work_agents_md_path(&ws_dir, work_ref);
    let agents_md_rel = std::path::Path::new("Works")
        .join(work_ref)
        .join("AGENTS.md");

    // Snapshot current content. `None` ⇒ the file is absent, so reset would
    // create it. A read failure for an existing-but-corrupt file is also
    // treated as "absent" so the atomic reset can still recover it.
    let current: Option<String> = std::fs::read_to_string(&agents_md_path).ok();
    let scaffold = nexus_orchestration::rules_layers::render_default_agents_md(work_ref);
    let would_change = current.as_deref().is_none_or(|c| c != scaffold);
    let diff = if would_change {
        current
            .as_deref()
            .map(|c| nexus_orchestration::rules_layers::diff_agents_md_vs_scaffold(c, work_ref))
            .unwrap_or_default()
    } else {
        String::new()
    };

    // ── `--dry-run`: preview only, never write, never prompt. ──────────
    if dry_run {
        if json {
            let diff_value = if diff.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(diff)
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "work_id": resolved_work_id,
                    "work_ref": work_ref,
                    "agents_md_path": agents_md_path.to_string_lossy(),
                    "dry_run": true,
                    "would_change": would_change,
                    "diff": diff_value,
                }))
                .unwrap_or_default()
            );
        } else if current.is_none() {
            println!(
                "• {rel} does not exist; reset would create it with the default scaffold.",
                rel = agents_md_rel.display()
            );
            println!("--- preview: default scaffold ---");
            print!("{scaffold}");
            if !scaffold.ends_with('\n') {
                println!();
            }
        } else if !would_change {
            println!(
                "• {rel} already matches the default scaffold (no changes).",
                rel = agents_md_rel.display()
            );
        } else {
            println!(
                "Dry run — no files modified. Proposed reset of {rel}:",
                rel = agents_md_rel.display()
            );
            print!("{diff}");
        }
        return Ok(());
    }

    // ── Nothing to do: file already matches the scaffold. ──────────────
    if !would_change {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "work_id": resolved_work_id,
                    "work_ref": work_ref,
                    "agents_md_path": agents_md_path.to_string_lossy(),
                    "reset": false,
                    "reason": "already matches default scaffold",
                }))
                .unwrap_or_default()
            );
        } else {
            println!(
                "• {rel} already matches the default scaffold (no changes).",
                rel = agents_md_rel.display()
            );
        }
        return Ok(());
    }

    // ── Pending changes. Confirm unless `--yes`. ───────────────────────
    if !yes {
        if json {
            // Machine-readable mode cannot host an interactive prompt; report
            // that confirmation is required and exit without writing.
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "work_id": resolved_work_id,
                    "work_ref": work_ref,
                    "agents_md_path": agents_md_path.to_string_lossy(),
                    "reset": false,
                    "confirmation_required": true,
                    "hint": "pass --yes to proceed non-interactively",
                }))
                .unwrap_or_default()
            );
            return Ok(());
        }
        if !confirm_reset_interactive(&agents_md_rel, &diff)? {
            println!(
                "• Reset declined; {rel} left unchanged.",
                rel = agents_md_rel.display()
            );
            return Ok(());
        }
    }

    // ── Perform the reset. ─────────────────────────────────────────────
    nexus_orchestration::rules_layers::reset_agents_md(&agents_md_path, work_ref).map_err(|e| {
        CliError::Other(format!("Failed to reset {}: {e}", agents_md_path.display()))
    })?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "work_id": resolved_work_id,
                "work_ref": work_ref,
                "agents_md_path": agents_md_path.to_string_lossy(),
                "reset": true,
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "✓ Reset {rel} to default scaffold",
            rel = agents_md_rel.display()
        );
    }
    Ok(())
}

/// Human-mode confirmation: print the diff and prompt before the reset.
///
/// Returns `Ok(true)` when the user confirms, `Ok(false)` when they decline.
/// Errors when stdin is not a terminal (callers should pass `--yes` for
/// non-interactive use).
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] when stdin is non-interactive or the
/// prompt itself fails.
fn confirm_reset_interactive(agents_md_rel: &std::path::Path, diff: &str) -> Result<bool> {
    print!("{diff}");
    println!("Lines marked '-' above will be DISCARDED by the reset.\n");
    if !std::io::stdin().is_terminal() {
        return Err(CliError::Config(format!(
            "Resetting {rel} requires confirmation but stdin is not a terminal. \
             Pass --yes to proceed, or --dry-run to preview.",
            rel = agents_md_rel.display()
        )));
    }
    let confirmed = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Reset {rel} to the default scaffold? This discards local edits.",
            rel = agents_md_rel.display()
        ))
        .default(false)
        .show_default(true)
        .interact_opt()
        .map_err(|e| CliError::Other(format!("confirmation prompt failed: {e}")))?;
    Ok(confirmed == Some(true))
}

/// PATCH a finding's status to `resolved` via the daemon API.
async fn patch_finding_resolved(
    client: &DaemonClient,
    work_id: &str,
    finding_id: &str,
) -> Result<()> {
    let path = format!("/v1/local/works/{work_id}/findings/{finding_id}");
    let body = serde_json::json!({ "status": "resolved" });
    client.patch::<serde_json::Value, _>(&path, &body).await?;
    Ok(())
}
