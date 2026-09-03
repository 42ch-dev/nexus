//! `nexus42 ops` — hidden operator group (V1.182 P1 BL-04, Task 2).
//!
//! `ops inspect [SESSION_ID] [--json]` is a **daemon-free, read-only** view
//! over the v1.180 checkpoint slice (`orchestration_sessions`): it opens the
//! workspace `state.db` with `nexus_local_db::open_pool_read_only` (no
//! migrations, no seed, no lock upgrades) and projects the
//! `resume_driven_sessions` rules 1–3 into a resumable verdict; rule 4
//! (`engine.has_runner`, boot-time in-memory state) is carried as the
//! separate `runner_check` caveat — never folded into the verdict.
//!
//! Contract: `.mstar/sdd/2026-09-03-v1.182-p1-bl04-checkpoint-resume-ux/inspect-contract.md`.
//!
//! Honesty discipline:
//! - `db_status` is the raw DB column and is never presented as
//!   authoritative run state (every save writes `'running'`; ON CONFLICT
//!   never updates it).
//! - Corrupt `context_json` → `verdict: "unknown"` / `context_unreadable`;
//!   no verdict is fabricated from unreadable data.
//! - The checkpoint stores POSITION ONLY — there is no completed-stages
//!   ledger, so the output never claims one.
//! - Slice boundary: read-only inspect; the CLI never implies resume can be
//!   triggered from here (re-drive happens on next daemon boot).

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_orchestration::storage::{CheckpointRow, SqliteSessionStorage};
use serde::Serialize;
use serde_json::{Map, Value};
use std::fmt::Write as _;
use std::sync::Arc;

/// Operator-facing daemon-free inspection commands (hidden group).
#[derive(Debug, Subcommand)]
pub enum OpsCommand {
    /// Inspect checkpointed orchestration sessions (daemon-free, read-only)
    Inspect {
        /// Session (run) id for the detail view; omit for the list view
        session_id: Option<String>,
        /// Emit the CLI-local inspect DTO verbatim (`snake_case`)
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// CLI-local inspect DTO (contract §3 — this accessor is new, so this shape
/// IS the contract; `snake_case`, field-by-field).
#[derive(Debug, Serialize)]
struct InspectDto {
    session_id: String,
    creator_id: String,
    preset_id: String,
    preset_version: i64,
    /// Raw DB status column — NOT authoritative.
    db_status: String,
    current_task_id: Option<String>,
    created_at: i64,
    updated_at: i64,
    run_failure: Option<RunFailure>,
    live_join_keys: Vec<String>,
    resumable: ResumableVerdict,
    /// Present as `false` only when `context_json` failed to parse.
    #[serde(skip_serializing_if = "Option::is_none")]
    context_readable: Option<bool>,
}

/// Typed run-failure record from context `_run_status` / `_run_error`.
#[derive(Debug, Serialize)]
struct RunFailure {
    run_status: Option<String>,
    run_error: Option<String>,
}

/// Resumable projection (contract §4): verdict carries rules 1–3; the
/// boot-time runner reconstruction requirement is the separate
/// `runner_check` caveat, never part of the verdict.
#[derive(Debug, Serialize)]
struct ResumableVerdict {
    verdict: &'static str,
    rule: &'static str,
    runner_check: &'static str,
    explanation: String,
}

/// Dispatch entry for the `ops` group.
///
/// # Errors
///
/// Returns [`CliError`] when config, the read-only pool, a query, or the
/// requested session id cannot be resolved honestly.
pub async fn run(command: OpsCommand, config: &CliConfig) -> Result<()> {
    match command {
        OpsCommand::Inspect { session_id, json } => inspect(session_id, json, config).await,
    }
}

async fn inspect(session_id: Option<String>, json: bool, config: &CliConfig) -> Result<()> {
    match inspect_inner(session_id, config).await {
        Ok(outcome) => {
            outcome.print(json);
            Ok(())
        }
        Err(err) => {
            if json {
                // Contract §6: `--json` errors print {"error": ...} on stdout,
                // exit 1. The returned Err also lands on stderr via main's
                // generic `Error: {e}` line — stdout stays machine-clean.
                println!("{}", serde_json::json!({"error": err.to_string()}));
            }
            Err(err)
        }
    }
}

enum InspectOutcome {
    /// No workspace db at all — honest empty state, exit 0.
    NoDatabase(String),
    /// Detail view for one row.
    Detail(Box<InspectDto>),
    /// List view over non-terminal rows.
    List(Vec<InspectDto>),
}

impl InspectOutcome {
    fn print(&self, json: bool) {
        if json {
            let value = match self {
                Self::NoDatabase(_) => Value::Array(Vec::new()),
                Self::Detail(dto) => serde_json::to_value(dto).unwrap_or(Value::Null),
                Self::List(rows) => serde_json::to_value(rows).unwrap_or(Value::Null),
            };
            println!("{value}");
            return;
        }
        match self {
            Self::NoDatabase(message) => println!("{message}"),
            Self::Detail(dto) => print!("{}", render_detail(dto)),
            Self::List(rows) => print!("{}", render_list(rows)),
        }
    }
}

async fn inspect_inner(session_id: Option<String>, config: &CliConfig) -> Result<InspectOutcome> {
    let db_path = crate::config::resolve_state_db_path(config)
        .map_err(|e| CliError::Config(e.to_string()))?;

    if !db_path.exists() {
        return Ok(InspectOutcome::NoDatabase(format!(
            "No workspace state database at {} — no checkpointed sessions.",
            db_path.display()
        )));
    }

    let pool = nexus_local_db::open_pool_read_only(&db_path)
        .await
        .map_err(CliError::from)?;
    let storage = SqliteSessionStorage::new(Arc::new(pool));

    if let Some(id) = session_id {
        let row = storage.get_checkpoint_row(&id).await?;
        let Some(row) = row else {
            return Err(CliError::Other(format!("No checkpointed session {id}.")));
        };
        return Ok(InspectOutcome::Detail(Box::new(project(&row))));
    }
    let rows = storage.list_checkpoint_rows().await?;
    Ok(InspectOutcome::List(rows.iter().map(project).collect()))
}

/// Project a raw checkpoint row into the inspect DTO (contract §3–§4).
fn project(row: &CheckpointRow) -> InspectDto {
    let context: std::result::Result<Value, _> = serde_json::from_slice(&row.context_json);
    let data = context
        .ok()
        .and_then(|root| root.get("data").and_then(Value::as_object).cloned());

    let (context_readable, run_failure, live_join_keys, resumable) = data.as_ref().map_or_else(
        || {
            (
                Some(false),
                None,
                Vec::new(),
                ResumableVerdict {
                    verdict: "unknown",
                    rule: "context_unreadable",
                    runner_check: "not_applicable",
                    explanation: "context unreadable (corrupt context_json); no verdict fabricated"
                        .to_string(),
                },
            )
        },
        project_readable_context,
    );

    InspectDto {
        session_id: row.session_id.clone(),
        creator_id: row.creator_id.clone(),
        preset_id: row.preset_id.clone(),
        preset_version: row.preset_version,
        db_status: row.status.clone(),
        current_task_id: row.current_task_id.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
        run_failure,
        live_join_keys,
        resumable,
        context_readable,
    }
}

fn project_readable_context(
    data: &Map<String, Value>,
) -> (Option<bool>, Option<RunFailure>, Vec<String>, ResumableVerdict) {
    let text_value = |key: &str| -> Option<String> {
        data.get(key)
            .filter(|v| !v.is_null())
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let run_status = text_value("_run_status");
    let run_error = text_value("_run_error");
    let run_failure = if run_status.is_some() || run_error.is_some() {
        Some(RunFailure {
            run_status,
            run_error,
        })
    } else {
        None
    };

    // Live join keys: written only by merge/converge gate states;
    // cleared keys are Value::Null (never removed) — non-null only.
    let mut live_join_keys: Vec<String> = data
        .iter()
        .filter(|(k, v)| {
            !v.is_null()
                && (k.starts_with("_converge_arrivals_")
                    || k.starts_with("_merge_")
                    || k.starts_with("_join_wait_start_"))
        })
        .map(|(k, _)| k.clone())
        .collect();
    live_join_keys.sort_unstable();

    let resumable = if run_failure.is_some() {
        ResumableVerdict {
            verdict: "no",
            rule: "typed_failure",
            runner_check: "not_applicable",
            explanation: "typed failure record present; boot re-drive skips typed-failed sessions"
                .to_string(),
        }
    } else if live_join_keys.is_empty() {
        ResumableVerdict {
            verdict: "no",
            rule: "not_converge_merge_class",
            runner_check: "not_applicable",
            explanation: "no live converge/merge join state; boot re-drive skips \
                          sessions outside the converge/merge chain class"
                .to_string(),
        }
    } else {
        ResumableVerdict {
            verdict: "yes",
            rule: "chain_class_no_failure",
            runner_check: "boot_time",
            explanation: "candidate for re-drive on next boot (converge/merge chain, \
                          no failure record); re-drive also requires the daemon to \
                          reconstruct a runner at boot (embedded presets only); \
                          user-preset sessions that fail reconstruction stay \
                          tracked-but-not-driven"
                .to_string(),
        }
    };

    (None, run_failure, live_join_keys, resumable)
}

/// Render unix epoch seconds as `YYYY-MM-DD HH:MM:SS UTC`.
fn render_ts(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0).map_or_else(
        || format!("{secs} (unix)"),
        |dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    )
}

fn render_detail(dto: &InspectDto) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "session:        {}", dto.session_id);
    let _ = writeln!(out, "creator:        {}", dto.creator_id);
    let _ = writeln!(out, "preset:         {}", dto.preset_id);
    let _ = writeln!(out, "preset_version: {}", dto.preset_version);
    let _ = writeln!(out, "status:         {}", dto.db_status);
    let position = dto.current_task_id.as_deref().unwrap_or("(none recorded)");
    let _ = writeln!(out, "position:       {position}");
    let _ = writeln!(out, "created_at:     {}", render_ts(dto.created_at));
    let _ = writeln!(out, "updated_at:     {}", render_ts(dto.updated_at));

    match &dto.run_failure {
        // Human output MAY truncate the error to one line; `--json` is verbatim.
        Some(failure) => {
            let status = failure.run_status.as_deref().unwrap_or("failed");
            let first_line = failure
                .run_error
                .as_deref()
                .map_or("", |e| e.lines().next().unwrap_or(""));
            let _ = writeln!(out, "run record:     {status}: {first_line}");
        }
        None => out.push_str("run record:     (no typed run record)\n"),
    }

    if dto.live_join_keys.is_empty() {
        out.push_str("join state:     (no live join keys)\n");
    } else {
        let _ = writeln!(
            out,
            "join state:     {} live join key(s): {}",
            dto.live_join_keys.len(),
            dto.live_join_keys.join(", ")
        );
    }

    let verdict_line = match dto.resumable.rule {
        "chain_class_no_failure" => "yes — candidate for re-drive on next boot (converge/merge chain, no failure record; runner reconstruction is boot-time — see runner_check)",
        "typed_failure" => "no — typed failure record present (boot never re-drives; see caveat)",
        "not_converge_merge_class" => "no — no live converge/merge join state (boot skips: not in chain class)",
        _ => "unknown — context unreadable (corrupt context_json; no verdict fabricated)",
    };
    let _ = writeln!(out, "resumable:      {verdict_line}");
    out
}

fn render_list(rows: &[InspectDto]) -> String {
    if rows.is_empty() {
        return "No checkpointed sessions.\n".to_string();
    }
    let mut out = String::new();
    for row in rows {
        let _ = writeln!(
            out,
            "{}  {}  {}  {}  {}  {}",
            row.session_id,
            row.preset_id,
            row.db_status,
            row.current_task_id.as_deref().unwrap_or("-"),
            render_ts(row.updated_at),
            row.resumable.verdict
        );
    }
    let _ = writeln!(out, "{} checkpointed session(s).", rows.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(context: &[u8]) -> CheckpointRow {
        CheckpointRow {
            session_id: "ses_t".to_string(),
            creator_id: "cr_t".to_string(),
            preset_id: "preset_t".to_string(),
            preset_version: 3,
            current_task_id: Some("task_9".to_string()),
            status: "running".to_string(),
            context_json: context.to_vec(),
            created_at: 1_756_990_000,
            updated_at: 1_756_990_300,
        }
    }

    #[test]
    fn chain_class_with_no_failure_is_yes_with_boot_time_caveat() {
        let ctx = serde_json::json!({"data": {
            "_converge_arrivals_j1": ["a"],
            "_join_wait_start_j1": 1
        }})
        .to_string();
        let dto = project(&row(ctx.as_bytes()));
        assert_eq!(dto.resumable.verdict, "yes");
        assert_eq!(dto.resumable.rule, "chain_class_no_failure");
        assert_eq!(dto.resumable.runner_check, "boot_time");
        assert!(dto.resumable.explanation.contains("boot"));
        assert_eq!(
            dto.live_join_keys,
            ["_converge_arrivals_j1", "_join_wait_start_j1"]
        );
        assert!(dto.run_failure.is_none());
        assert!(dto.context_readable.is_none());
    }

    #[test]
    fn typed_failure_record_forces_no() {
        let ctx = serde_json::json!({"data": {
            "_run_error": "boom",
            "_converge_arrivals_j1": ["a"]
        }})
        .to_string();
        let dto = project(&row(ctx.as_bytes()));
        assert_eq!(dto.resumable.verdict, "no");
        assert_eq!(dto.resumable.rule, "typed_failure");
        assert_eq!(dto.resumable.runner_check, "not_applicable");
        let failure = dto.run_failure.expect("failure record");
        assert_eq!(failure.run_status, None);
        assert_eq!(failure.run_error.as_deref(), Some("boom"));
    }

    #[test]
    fn null_cleared_join_keys_are_not_live() {
        let ctx = serde_json::json!({"data": {
            "_converge_arrivals_j1": null,
            "_merge_j1": null,
            "_join_wait_start_j1": null
        }})
        .to_string();
        let dto = project(&row(ctx.as_bytes()));
        assert!(dto.live_join_keys.is_empty());
        assert_eq!(dto.resumable.verdict, "no");
        assert_eq!(dto.resumable.rule, "not_converge_merge_class");
    }

    #[test]
    fn merge_and_wait_keys_count_as_live() {
        let ctx = serde_json::json!({"data": {"_merge_j2": ["x"]}}).to_string();
        let dto = project(&row(ctx.as_bytes()));
        assert_eq!(dto.live_join_keys, ["_merge_j2"]);
        assert_eq!(dto.resumable.verdict, "yes");
    }

    #[test]
    fn corrupt_context_is_unknown_and_never_fabricated() {
        let dto = project(&row(b"not-json"));
        assert_eq!(dto.context_readable, Some(false));
        assert_eq!(dto.resumable.verdict, "unknown");
        assert_eq!(dto.resumable.rule, "context_unreadable");
        assert!(dto.run_failure.is_none());
        assert!(dto.live_join_keys.is_empty());
    }

    #[test]
    fn context_readable_field_serializes_only_when_false() {
        let ctx = serde_json::json!({"data": {}}).to_string();
        let readable = serde_json::to_value(project(&row(ctx.as_bytes()))).unwrap();
        assert!(readable.get("context_readable").is_none());

        let unreadable = serde_json::to_value(project(&row(b"\xff"))).unwrap();
        assert_eq!(unreadable["context_readable"], serde_json::json!(false));
    }

    #[test]
    fn missing_position_renders_honest_negative() {
        let mut r = row(b"{}");
        r.current_task_id = None;
        let rendered = render_detail(&project(&r));
        assert!(rendered.contains("position:       (none recorded)"));
        assert!(!rendered.contains("completed"));
    }

    #[test]
    fn empty_list_renders_honest_empty_state() {
        assert_eq!(render_list(&[]), "No checkpointed sessions.\n");
    }
}
