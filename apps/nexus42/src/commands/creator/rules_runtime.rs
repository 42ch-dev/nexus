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
//! reads finding/Work data from the typed core, and invokes the pure
//! helpers.
//!
//! Spec refs:
//! - [archived/knowledge/novel-findings-maturity.md §3 / §4](../../../../../.mstar/archived/knowledge/novel-findings-maturity.md)
//! - [novel-writing/workflow-profile.md §5.5.4](../../../../../.mstar/specs/novel-writing/workflow-profile.md)

use std::io::IsTerminal;

use crate::commands::creator::works::{active_work_id_core, FindingsCommand, RulesCommand};
use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use nexus_core::{CoreService, ListFindingsQuery, Principal, UpdateFindingRequest};

/// Handle `creator works findings …` (V1.48 P2).
///
/// Every arm runs on the typed core seam: the finding family
/// ([`CoreService::get_finding`], `list_findings`, `update_finding`,
/// `prune_findings`) for the data, and the existing guarded local library
/// ([`nexus_orchestration::rules_layers`]) for the `AGENTS.md` write.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on the mapped core refusal, missing
/// `rule_suggestion`, or filesystem write error, plus any cleanup refusal from
/// [`finish_direct`].
pub async fn handle_findings(config: &CliConfig, command: FindingsCommand) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match command {
            FindingsCommand::Accept { finding_id, json } => {
                handle_findings_accept(&core, &principal, &finding_id, json).await
            }
            FindingsCommand::Prune {
                older_than_days,
                dry_run,
                json,
            } => handle_findings_prune(&core, &principal, older_than_days, dry_run, json).await,
            FindingsCommand::List {
                work_id,
                status,
                severity,
                json,
            } => handle_findings_list(&core, &principal, work_id, status, severity, json).await,
            FindingsCommand::SetStatus {
                finding_id,
                work,
                status,
                target_executor,
                json,
            } => {
                handle_findings_set_status(
                    &core,
                    &principal,
                    &finding_id,
                    &work,
                    status.as_str(),
                    target_executor.as_deref(),
                    json,
                )
                .await
            }
        }
    }
    .await;
    // The arms return their report; nothing reaches stdout until the shared
    // seam released the writer, so an accepted/pruned/listed finding is never
    // reported ahead of a close that did not settle.
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

/// Handle `creator works rules …` (V1.48 P2).
///
/// The Work read comes from the typed core; the `AGENTS.md` reset itself stays
/// the guarded local library call it always was. The outcome returns the report
/// and the writer is released by [`finish_direct`] before any line is printed.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on the mapped core refusal, missing
/// `work_ref`, non-interactive confirmation, or filesystem write error, plus
/// any cleanup refusal from [`finish_direct`].
pub async fn handle_rules(config: &CliConfig, command: RulesCommand) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match command {
            RulesCommand::Reset {
                work_id,
                dry_run,
                yes,
                json,
            } => handle_rules_reset(&core, &principal, work_id, dry_run, yes, json).await,
        }
    }
    .await;
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

/// `creator works findings accept <finding_id>` (overlay §3.2).
///
/// Steps:
/// 1. Read the finding (creator-scoped) → must have non-empty `rule_suggestion`.
/// 2. Read the Work → resolve `work_ref` (the read also verifies Work
///    ownership before the status write).
/// 3. Resolve the operational workspace dir from CLI config.
/// 4. Append the rule suggestion to `Works/<work_ref>/AGENTS.md`
///    (idempotent on `finding_id`).
/// 5. Mark the finding `resolved` (skipped when it already is).
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on the mapped core refusal, a finding
/// without `rule_suggestion`, a Work without `work_ref`, or a filesystem write
/// error.
async fn handle_findings_accept(
    core: &CoreService,
    principal: &Principal,
    finding_id: &str,
    json: bool,
) -> Result<Option<String>> {
    // 1. Read the finding (creator-scoped, V1.48 P2).
    let finding = core
        .get_finding(principal, finding_id.to_string())
        .await
        .map_err(map_core_error)?;

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

    // 3. Resolve work_ref from the Work record (ownership included).
    let work = core
        .get_work(principal, finding.work_id.clone())
        .await
        .map_err(map_core_error)?;
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

    // 6. Mark the finding `resolved` (idempotent — skip the write when it is
    //    already resolved).
    let already_resolved = finding.status == "resolved";
    let resolved_now = if already_resolved {
        false
    } else {
        core.update_finding(
            principal,
            finding_id.to_string(),
            UpdateFindingRequest {
                status: Some("resolved".to_string()),
                ..UpdateFindingRequest::default()
            },
        )
        .await
        .map_err(map_core_error)?;
        true
    };

    Ok(Some(if json {
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
        serde_json::to_string_pretty(&body).unwrap_or_default()
    } else {
        let agents_md_rel = std::path::Path::new("Works")
            .join(work_ref)
            .join("AGENTS.md");
        let mut lines = vec![match outcome {
            nexus_orchestration::rules_layers::AppendOutcome::Appended => {
                format!(
                    "✓ Appended rule suggestion from finding {finding_id} to {rel}",
                    rel = agents_md_rel.display()
                )
            }
            nexus_orchestration::rules_layers::AppendOutcome::AlreadyPresent => {
                format!(
                    "• Finding {finding_id} already recorded in {rel} (idempotent — no change)",
                    rel = agents_md_rel.display()
                )
            }
        }];
        if resolved_now {
            lines.push(format!("✓ Marked finding {finding_id} as resolved"));
        } else if already_resolved {
            lines.push(format!("• Finding {finding_id} was already resolved"));
        }
        lines.join("\n")
    }))
}

/// `creator works findings prune [--older-than <days>] [--dry-run]`
/// (`novel-writing/quality-loop.md` §9.4).
///
/// Runs [`CoreService::prune_findings`] and reports the deleted (or, in
/// dry-run, would-be-deleted) count. `resolved` findings older than the
/// retention window are eligible; `open` and `wont_fix` are never touched. The
/// `--json` body keeps the four keys the retired prune route served.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on the mapped core refusal.
async fn handle_findings_prune(
    core: &CoreService,
    principal: &Principal,
    older_than_days: i64,
    dry_run: bool,
    json: bool,
) -> Result<Option<String>> {
    let outcome = core
        .prune_findings(principal, Some(older_than_days), dry_run)
        .await
        .map_err(map_core_error)?;

    if json {
        return Ok(Some(serde_json::to_string_pretty(&serde_json::json!({
            "count": outcome.count,
            "older_than_days": outcome.older_than_days,
            "dry_run": outcome.dry_run,
            "now_epoch": outcome.now_epoch,
        }))?));
    }

    let count = outcome.count;
    let days = outcome.older_than_days;
    Ok(Some(if outcome.dry_run {
        if count == 0 {
            format!("• No resolved findings older than {days} days to prune (dry-run).")
        } else {
            format!(
                "Dry run — {count} resolved finding(s) older than {days} day(s) \
                 would be pruned. Re-run without --dry-run to delete."
            )
        }
    } else if count == 0 {
        format!("• No resolved findings older than {days} days to prune; nothing deleted.")
    } else {
        format!("✓ Pruned {count} resolved finding(s) older than {days} day(s).")
    }))
}

/// `creator works findings list <work_id> [--status …] [--severity …] [--json]`
/// — list findings for a Work (AR-87).
///
/// `--status` accepts a single status or a comma-separated list (e.g.
/// `open,triaged`). `--json` emits the `ListFindingsResponse` DTO verbatim.
///
/// # Errors
///
/// Returns `CliError` for the typed core refusal (404 `not_found` for an
/// unknown work, `invalid input` for an unknown status/severity token).
async fn handle_findings_list(
    core: &CoreService,
    principal: &Principal,
    work_id: Option<String>,
    status: Option<String>,
    severity: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let resolved_id = match work_id {
        Some(id) => id,
        None => active_work_id_core(core, principal).await?,
    };
    let resp = core
        .list_findings(
            principal,
            resolved_id.clone(),
            ListFindingsQuery {
                status,
                severity,
                ..ListFindingsQuery::default()
            },
        )
        .await
        .map_err(map_core_error)?;
    if json {
        // `ListFindingsResponse` DTO verbatim: `items` are the findings list
        // elements and `pagination` the cursor envelope. The core carriers are
        // the same shapes the generated wrapper nests (its element/pagination
        // types are re-declared per schema file and are not constructible from
        // the core's carriers), so the body is assembled from them directly.
        return Ok(Some(serde_json::to_string_pretty(&serde_json::json!({
            "items": resp.items,
            "pagination": resp.pagination,
        }))?));
    }
    if resp.items.is_empty() {
        return Ok(Some(format!("No findings for work '{resolved_id}'.")));
    }
    let mut lines = vec![
        format!("Findings for work '{resolved_id}':\n"),
        format!(
            "{:<36} {:<10} {:<10} {:<12} TITLE",
            "FINDING_ID", "STATUS", "SEVERITY", "TARGET"
        ),
        "-".repeat(100),
    ];
    for f in &resp.items {
        lines.push(format!(
            "{:<36} {:<10} {:<10} {:<12} {}",
            f.finding_id, f.status, f.severity, f.target_executor, f.title
        ));
    }
    if resp.pagination.has_more {
        lines.push(
            "\n(truncated — more findings exist; refine with --status/--severity or use --json for the complete DTO)"
                .to_string(),
        );
    }
    lines.push(format!("\n{} finding(s)", resp.items.len()));
    Ok(Some(lines.join("\n")))
}

/// `creator works findings set-status <finding_id> --work <work_id> --status <s>
/// [--target-executor <exec>] [--json]` — set a finding's status through the
/// work-findings PATCH surface (AR-87 #2/#3).
///
/// One generic verb over one route. An illegal lifecycle transition is refused
/// with `invalid_transition` naming `from → to` (surfaced by
/// [`map_core_error`]); `--help` documents the closed transition table.
///
/// # Errors
///
/// Returns `CliError` for the typed core refusal (404 `not_found` for an
/// unknown work/finding, `invalid_transition` for an illegal status move).
async fn handle_findings_set_status(
    core: &CoreService,
    principal: &Principal,
    finding_id: &str,
    work_id: &str,
    status: &str,
    target_executor: Option<&str>,
    json: bool,
) -> Result<Option<String>> {
    // Work ownership on the work-scoped route: the core's creator-scoped
    // `update_finding` deliberately leaves that check to its caller, so the
    // read the retired route performed happens here — before any write.
    core.get_work(principal, work_id.to_string())
        .await
        .map_err(map_core_error)?;

    let resp = core
        .update_finding(
            principal,
            finding_id.to_string(),
            UpdateFindingRequest {
                status: Some(status.to_string()),
                target_executor: target_executor.map(str::to_string),
                ..UpdateFindingRequest::default()
            },
        )
        .await
        .map_err(map_core_error)?;
    Ok(Some(if json {
        serde_json::to_string_pretty(&resp)?
    } else {
        format!("Finding '{finding_id}' status set to '{}'.", resp.status)
    }))
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
/// Returns the report `handle_rules` prints once the writer settled. The
/// confirmation prompt itself (the diff it shows and its question) necessarily
/// runs inside the outcome: it decides whether the admitted writer writes at
/// all, so it cannot be deferred past the seam without a second writer
/// admission.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on the mapped core refusal, missing
/// `work_ref`, non-interactive confirmation, or filesystem write error.
// Linear 6-phase CLI dispatch (resolve → snapshot → dry-run → no-op →
// confirm → write) reported in JSON + human form. Splitting it would create a
// `too_many_arguments` helper or a single-use context struct; neither reduces
// real complexity. Phase boundaries are marked by section comments below.
#[allow(clippy::too_many_lines)]
async fn handle_rules_reset(
    core: &CoreService,
    principal: &Principal,
    work_id: Option<String>,
    dry_run: bool,
    yes: bool,
    json: bool,
) -> Result<Option<String>> {
    let resolved_work_id = match work_id {
        Some(id) => id,
        None => active_work_id_core(core, principal).await?,
    };

    // Resolve work_ref from the Work record (ownership included).
    let work = core
        .get_work(principal, resolved_work_id.clone())
        .await
        .map_err(map_core_error)?;
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
            return Ok(Some(
                serde_json::to_string_pretty(&serde_json::json!({
                    "work_id": resolved_work_id,
                    "work_ref": work_ref,
                    "agents_md_path": agents_md_path.to_string_lossy(),
                    "dry_run": true,
                    "would_change": would_change,
                    "diff": diff_value,
                }))
                .unwrap_or_default(),
            ));
        }
        return Ok(Some(if current.is_none() {
            format!(
                "• {rel} does not exist; reset would create it with the default scaffold.\n\
                 --- preview: default scaffold ---\n{body}",
                rel = agents_md_rel.display(),
                body = without_trailing_newline(&scaffold)
            )
        } else if !would_change {
            format!(
                "• {rel} already matches the default scaffold (no changes).",
                rel = agents_md_rel.display()
            )
        } else {
            format!(
                "Dry run — no files modified. Proposed reset of {rel}:\n{body}",
                rel = agents_md_rel.display(),
                body = without_trailing_newline(&diff)
            )
        }));
    }

    // ── Nothing to do: file already matches the scaffold. ──────────────
    if !would_change {
        return Ok(Some(if json {
            serde_json::to_string_pretty(&serde_json::json!({
                "work_id": resolved_work_id,
                "work_ref": work_ref,
                "agents_md_path": agents_md_path.to_string_lossy(),
                "reset": false,
                "reason": "already matches default scaffold",
            }))
            .unwrap_or_default()
        } else {
            format!(
                "• {rel} already matches the default scaffold (no changes).",
                rel = agents_md_rel.display()
            )
        }));
    }

    // ── Pending changes. Confirm unless `--yes`. ───────────────────────
    if !yes {
        if json {
            // Machine-readable mode cannot host an interactive prompt; report
            // that confirmation is required and exit without writing.
            return Ok(Some(
                serde_json::to_string_pretty(&serde_json::json!({
                    "work_id": resolved_work_id,
                    "work_ref": work_ref,
                    "agents_md_path": agents_md_path.to_string_lossy(),
                    "reset": false,
                    "confirmation_required": true,
                    "hint": "pass --yes to proceed non-interactively",
                }))
                .unwrap_or_default(),
            ));
        }
        if !confirm_reset_interactive(&agents_md_rel, &diff)? {
            return Ok(Some(format!(
                "• Reset declined; {rel} left unchanged.",
                rel = agents_md_rel.display()
            )));
        }
    }

    // ── Perform the reset. ─────────────────────────────────────────────
    nexus_orchestration::rules_layers::reset_agents_md(&agents_md_path, work_ref).map_err(|e| {
        CliError::Other(format!("Failed to reset {}: {e}", agents_md_path.display()))
    })?;

    Ok(Some(if json {
        serde_json::to_string_pretty(&serde_json::json!({
            "work_id": resolved_work_id,
            "work_ref": work_ref,
            "agents_md_path": agents_md_path.to_string_lossy(),
            "reset": true,
        }))
        .unwrap_or_default()
    } else {
        format!(
            "✓ Reset {rel} to default scaffold",
            rel = agents_md_rel.display()
        )
    }))
}

/// The body of a renderable block whose own trailing newline the caller's
/// single `println!` would otherwise double: `print!`-then-`println!` and
/// `println!` produce the same bytes only when the body carries no final
/// newline of its own.
fn without_trailing_newline(text: &str) -> &str {
    text.strip_suffix('\n').unwrap_or(text)
}

/// Human-mode confirmation: print the diff and prompt before the reset.
///
/// Its output **is** the prompt (the diff the operator decides on), so it runs
/// inside the outcome by necessity — before the write and therefore before the
/// seam settles. It is not part of the returned report, which carries only the
/// outcome the seam has already resolved.
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
