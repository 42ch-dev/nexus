//! `nexus42 ops` — hidden operator group (V1.182 P1 BL-04, Task 2).
//!
//! `ops inspect [SESSION_ID] [--json]` is a **daemon-free, read-only** view
//! over the v1.180 checkpoint slice (`orchestration_sessions`): it opens the
//! workspace `state.db` with `nexus_local_db::open_pool_read_only` (no
//! migrations, no seed, no lock upgrades) and projects the
//! `resume_driven_sessions` rules 1–4 into a resumable verdict via the
//! shared `nexus_orchestration::resume_rules` cascade (rule 1 = terminal
//! status, then context readability, typed failure, chain class); rule 4's
//! in-memory half (`engine.has_runner`, boot-time state) is carried as the
//! separate `runner_check` caveat — never folded into the verdict.
//!
//! Contract: `.mstar/sdd/2026-09-03-v1.182-p1-bl04-checkpoint-resume-ux/inspect-contract.md`.
//!
//! Honesty discipline:
//! - `db_status` is the raw DB column and is never presented as
//!   authoritative run state (every save writes `'running'`; ON CONFLICT
//!   never updates it).
//! - Corrupt `context_json` → `verdict: "unknown"` / `context_unreadable`;
//!   no verdict is fabricated from unreadable data. `context_readable` is a
//!   two-class flag: `false` ONLY for corrupt bytes; valid-JSON-unexpected-
//!   shape is byte-readable → `true`, with the shape anomaly carried in the
//!   classification/explanation.
//! - The checkpoint stores POSITION ONLY — there is no completed-stages
//!   ledger, so the output never claims one.
//! - Slice boundary: read-only inspect; the CLI never implies resume can be
//!   triggered from here (re-drive happens on next daemon boot).

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_orchestration::resume_rules::{self, ResumeClass};
use nexus_orchestration::storage::{CheckpointRow, CheckpointSummary, SqliteSessionStorage};
use serde::Serialize;
use serde_json::Value;
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
#[derive(Debug, Clone, Serialize)]
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
    /// Two-class readability flag (contract §3): `Some(false)` only when
    /// `context_json` failed to parse (corrupt bytes); `Some(true)` when the
    /// JSON parses but `data` is missing/not an object (byte-readable, shape
    /// anomaly carried in `resumable`); `None` on fully readable rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    context_readable: Option<bool>,
}

/// Typed run-failure record from context `_run_status` / `_run_error`.
#[derive(Debug, Clone, Serialize)]
struct RunFailure {
    run_status: Option<String>,
    run_error: Option<String>,
}

/// Resumable projection (contract §4): verdict carries rules 1–4; the
/// boot-time runner reconstruction requirement is the separate
/// `runner_check` caveat, never part of the verdict.
#[derive(Debug, Clone, Serialize)]
struct ResumableVerdict {
    verdict: Verdict,
    rule: ResumeRule,
    runner_check: RunnerCheck,
    explanation: String,
}

/// Stable resumable verdict word (contract `verdict` field).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Verdict {
    Yes,
    No,
    Unknown,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Unknown => "unknown",
        })
    }
}

/// Stable verdict rule (contract `rule` field) — the shared
/// [`ResumeClass`] projected into the DTO.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ResumeRule {
    TerminalStatus,
    ContextUnreadable,
    TypedFailure,
    NotConvergeMergeClass,
    ChainClassNoFailure,
}

impl From<ResumeClass> for ResumeRule {
    fn from(class: ResumeClass) -> Self {
        match class {
            ResumeClass::TerminalStatus => Self::TerminalStatus,
            ResumeClass::ContextUnreadable => Self::ContextUnreadable,
            ResumeClass::TypedFailure => Self::TypedFailure,
            ResumeClass::NotConvergeMergeClass => Self::NotConvergeMergeClass,
            ResumeClass::ChainClassNoFailure => Self::ChainClassNoFailure,
        }
    }
}

/// Boot-time runner availability — NEVER part of the verdict (rule 4 is
/// in-memory daemon state, not derivable from persisted rows).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RunnerCheck {
    BootTime,
    NotApplicable,
}

/// Why a context could not be projected — corrupt bytes vs parseable-but-
/// unexpected schema (distinct honest wording, qc3 S1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnreadableKind {
    CorruptJson,
    UnexpectedShape,
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
    /// Detail view for one row (bare DTO object).
    Detail(Box<InspectDto>),
    /// List view over non-terminal rows (bounded) + honest full total.
    List { rows: Vec<InspectDto>, total: i64 },
}

impl InspectOutcome {
    fn print(&self, json: bool) {
        if json {
            let value = match self {
                Self::NoDatabase(_) => serde_json::json!({
                    "db_present": false,
                    "total": 0,
                    "rows": [],
                }),
                Self::Detail(dto) => serde_json::to_value(dto).unwrap_or(Value::Null),
                Self::List { rows, total } => serde_json::json!({
                    "db_present": true,
                    "total": total,
                    "rows": serde_json::to_value(rows).unwrap_or(Value::Null),
                }),
            };
            println!("{value}");
            return;
        }
        match self {
            Self::NoDatabase(message) => println!("{message}"),
            Self::Detail(dto) => print!("{}", render_detail(dto)),
            Self::List { rows, total } => print!("{}", render_list(rows, *total)),
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
    let (rows, total) = tokio::join!(
        storage.list_checkpoint_rows(),
        storage.count_checkpoint_rows()
    );
    let rows = rows?;
    let total = total?;
    let dtos: Vec<InspectDto> = rows.iter().map(project_summary).collect();
    Ok(InspectOutcome::List { rows: dtos, total })
}

/// Project a raw checkpoint row into the inspect DTO (contract §3–§4).
fn project(row: &CheckpointRow) -> InspectDto {
    let context: std::result::Result<Value, _> = serde_json::from_slice(&row.context_json);
    let shape = context.map_or(Err(UnreadableKind::CorruptJson), |root| {
        resume_rules::context_data(&root).map_or(Err(UnreadableKind::UnexpectedShape), |data| {
            Ok(data.clone())
        })
    });

    let (context_readable, run_failure, live_join_keys, resumable) = match &shape {
        Ok(data) => (
            None,
            resume_rules::typed_failure_record(data).map(|record| RunFailure {
                run_status: record.run_status,
                run_error: record.run_error,
            }),
            resume_rules::live_join_keys(data),
            verdict_for(
                row.status.as_str(),
                resume_rules::classify_resumability(row.status.as_str(), Some(data)),
                None,
            ),
        ),
        Err(kind) => (
            // Two-class flag (contract §3): `false` ONLY for corrupt bytes;
            // valid-JSON-unexpected-shape is byte-readable → `true`, with
            // the shape anomaly carried in the classification/explanation.
            Some(matches!(*kind, UnreadableKind::UnexpectedShape)),
            None,
            Vec::new(),
            verdict_for(
                row.status.as_str(),
                resume_rules::classify_resumability(row.status.as_str(), None),
                Some(*kind),
            ),
        ),
    };

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

/// Project a lean list row into the inspect DTO. The storage layer already
/// evaluated the resume-rule predicates in SQL (no `context_json` loaded),
/// so the verdict comes from the shared extraction cascade — identical
/// rule order and wording to detail mode.
fn project_summary(row: &CheckpointSummary) -> InspectDto {
    let run_failure = (row.run_status.is_some() || row.run_error.is_some()).then(|| RunFailure {
        run_status: row.run_status.clone(),
        run_error: row.run_error.clone(),
    });
    let live_join_keys: Vec<String> = row
        .live_join_keys
        .as_deref()
        .map(|comma| comma.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let has_live_join_keys = !live_join_keys.is_empty();
    // Same corrupt-vs-shape honesty as detail mode, from the SQL flags.
    let unreadable_kind = if !row.context_valid_json {
        Some(UnreadableKind::CorruptJson)
    } else if !row.context_data_is_object {
        Some(UnreadableKind::UnexpectedShape)
    } else {
        None
    };
    // Two-class flag (contract §3): `false` ONLY for corrupt bytes;
    // valid-JSON-unexpected-shape is byte-readable → `true`, with the
    // shape anomaly carried in the classification/explanation.
    let context_unreadable = unreadable_kind.is_some();
    let context_readable =
        unreadable_kind.map(|kind| matches!(kind, UnreadableKind::UnexpectedShape));

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
        resumable: verdict_for(
            row.status.as_str(),
            resume_rules::classify_resumability_extracted(
                row.status.as_str(),
                context_unreadable,
                row.run_status.is_some() || row.run_error.is_some(),
                has_live_join_keys,
            ),
            unreadable_kind,
        ),
        context_readable,
    }
}

/// Build the resumable verdict for a classification. `unreadable_kind` is
/// `Some` only for the `ContextUnreadable` class and selects the honest
/// wording (corrupt bytes vs unexpected schema). Rule-4 caveat:
/// `runner_check` is `boot_time` exactly when the verdict is `yes`.
fn verdict_for(
    row_status: &str,
    class: ResumeClass,
    unreadable_kind: Option<UnreadableKind>,
) -> ResumableVerdict {
    match class {
        ResumeClass::TerminalStatus => ResumableVerdict {
            verdict: Verdict::No,
            rule: ResumeRule::TerminalStatus,
            runner_check: RunnerCheck::NotApplicable,
            explanation: format!(
                "terminal status '{row_status}' — boot never re-drives non-running sessions"
            ),
        },
        ResumeClass::ContextUnreadable => ResumableVerdict {
            verdict: Verdict::Unknown,
            rule: ResumeRule::ContextUnreadable,
            runner_check: RunnerCheck::NotApplicable,
            explanation: match unreadable_kind {
                Some(UnreadableKind::CorruptJson) => {
                    "context unreadable (corrupt context_json); no verdict fabricated".to_string()
                }
                Some(UnreadableKind::UnexpectedShape) => {
                    "context readable but unexpected shape (missing or non-object 'data'); \
                     no verdict fabricated"
                        .to_string()
                }
                None => "context unreadable; no verdict fabricated".to_string(),
            },
        },
        ResumeClass::TypedFailure => ResumableVerdict {
            verdict: Verdict::No,
            rule: ResumeRule::TypedFailure,
            runner_check: RunnerCheck::NotApplicable,
            explanation: "typed failure record present; boot re-drive skips typed-failed sessions"
                .to_string(),
        },
        ResumeClass::NotConvergeMergeClass => ResumableVerdict {
            verdict: Verdict::No,
            rule: ResumeRule::NotConvergeMergeClass,
            runner_check: RunnerCheck::NotApplicable,
            explanation: "no live converge/merge join state; boot re-drive skips \
                          sessions outside the converge/merge chain class"
                .to_string(),
        },
        ResumeClass::ChainClassNoFailure => ResumableVerdict {
            verdict: Verdict::Yes,
            rule: ResumeRule::ChainClassNoFailure,
            runner_check: RunnerCheck::BootTime,
            explanation: "candidate for re-drive on next boot (converge/merge chain, \
                          no failure record); re-drive also requires the daemon to \
                          reconstruct a runner at boot (embedded presets only); \
                          user-preset sessions that fail reconstruction stay \
                          tracked-but-not-driven"
                .to_string(),
        },
    }
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
        // Human output MAY truncate the error to one line — and must mark
        // the cut (qc2 F-001); `--json` stays DTO-verbatim.
        Some(failure) => {
            let status = failure.run_status.as_deref().unwrap_or("failed");
            let first_line = failure
                .run_error
                .as_deref()
                .map_or("", |e| e.lines().next().unwrap_or(""));
            let marker = match failure.run_error.as_deref() {
                Some(e) if e.contains('\n') => " … (truncated; see --json)",
                _ => "",
            };
            let _ = writeln!(out, "run record:     {status}: {first_line}{marker}");
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
        ResumeRule::ChainClassNoFailure => "yes — candidate for re-drive on next boot (converge/merge chain, no failure record; runner reconstruction is boot-time — see runner_check)".to_string(),
        ResumeRule::TypedFailure => "no — typed failure record present (boot never re-drives; see caveat)".to_string(),
        ResumeRule::NotConvergeMergeClass => "no — no live converge/merge join state (boot skips: not in chain class)".to_string(),
        ResumeRule::TerminalStatus => "no — terminal status (boot never re-drives; see caveat)".to_string(),
        // Contract §5 wording split (qc3 S1): corrupt bytes vs parseable-but-
        // unexpected shape — the DTO explanation already distinguishes the two
        // honest wordings, so the human line mirrors the JSON explanation
        // instead of always claiming a corrupt context_json.
        ResumeRule::ContextUnreadable => format!("unknown — {}", dto.resumable.explanation),
    };
    let _ = writeln!(out, "resumable:      {verdict_line}");
    out
}

fn render_list(rows: &[InspectDto], total: i64) -> String {
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
    let shown: i64 = rows.len().try_into().unwrap_or(i64::MAX);
    if shown >= total {
        let _ = writeln!(out, "{total} checkpointed session(s).");
    } else {
        // Honest truncated total (qc3 W2): the LIMIT hides no rows silently.
        let _ = writeln!(out, "{shown} of {total}+ checkpointed session(s).");
    }
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
        assert_eq!(dto.resumable.verdict, Verdict::Yes);
        assert_eq!(dto.resumable.rule, ResumeRule::ChainClassNoFailure);
        assert_eq!(dto.resumable.runner_check, RunnerCheck::BootTime);
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
        assert_eq!(dto.resumable.verdict, Verdict::No);
        assert_eq!(dto.resumable.rule, ResumeRule::TypedFailure);
        assert_eq!(dto.resumable.runner_check, RunnerCheck::NotApplicable);
        let failure = dto.run_failure.expect("failure record");
        assert_eq!(failure.run_status, None);
        assert_eq!(failure.run_error.as_deref(), Some("boom"));
    }

    #[test]
    fn terminal_status_forces_no_even_with_live_join_keys() {
        // schedule-cancel writer (schedules.rs:1226): status='cancelled'
        // with context untouched — live join keys + no typed failure.
        let ctx = serde_json::json!({"data": {
            "_converge_arrivals_j1": ["a"],
            "_join_wait_start_j1": 1
        }})
        .to_string();
        let mut r = row(ctx.as_bytes());
        r.status = "cancelled".to_string();
        let dto = project(&r);
        assert_eq!(dto.db_status, "cancelled");
        assert_eq!(
            dto.live_join_keys,
            ["_converge_arrivals_j1", "_join_wait_start_j1"]
        );
        assert_eq!(dto.resumable.verdict, Verdict::No);
        assert_eq!(dto.resumable.rule, ResumeRule::TerminalStatus);
        assert_eq!(dto.resumable.runner_check, RunnerCheck::NotApplicable);
        assert!(dto.resumable.explanation.contains("terminal"));
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
        assert_eq!(dto.resumable.verdict, Verdict::No);
        assert_eq!(dto.resumable.rule, ResumeRule::NotConvergeMergeClass);
    }

    #[test]
    fn merge_and_wait_keys_count_as_live() {
        let ctx = serde_json::json!({"data": {"_merge_j2": ["x"]}}).to_string();
        let dto = project(&row(ctx.as_bytes()));
        assert_eq!(dto.live_join_keys, ["_merge_j2"]);
        assert_eq!(dto.resumable.verdict, Verdict::Yes);
    }

    #[test]
    fn corrupt_context_is_unknown_and_never_fabricated() {
        let dto = project(&row(b"not-json"));
        assert_eq!(dto.context_readable, Some(false));
        assert_eq!(dto.resumable.verdict, Verdict::Unknown);
        assert_eq!(dto.resumable.rule, ResumeRule::ContextUnreadable);
        assert!(dto.resumable.explanation.contains("corrupt"));
        assert!(dto.run_failure.is_none());
        assert!(dto.live_join_keys.is_empty());
    }

    #[test]
    fn shape_anomaly_is_readable_but_unexpected_shape() {
        // Parseable JSON, `data` not an object — schema-shape anomaly, not
        // byte corruption (qc3 S1). Byte-readable → `context_readable: true`;
        // the shape anomaly is carried in the classification/explanation.
        let dto = project(&row(br#"{"data": "not-an-object"}"#));
        assert_eq!(dto.context_readable, Some(true));
        assert_eq!(dto.resumable.verdict, Verdict::Unknown);
        assert_eq!(dto.resumable.rule, ResumeRule::ContextUnreadable);
        assert!(
            dto.resumable.explanation.contains("shape"),
            "shape wording: {}",
            dto.resumable.explanation
        );
        assert!(
            !dto.resumable.explanation.contains("corrupt"),
            "shape anomaly must not be labelled corrupt: {}",
            dto.resumable.explanation
        );
    }

    #[test]
    fn context_readable_serializes_two_class_flag() {
        // Fully readable row → field absent.
        let ctx = serde_json::json!({"data": {"_merge_j1": ["x"]}}).to_string();
        let readable = serde_json::to_value(project(&row(ctx.as_bytes()))).unwrap();
        assert!(readable.get("context_readable").is_none());
        assert_eq!(readable["resumable"]["verdict"], serde_json::json!("yes"));
        assert_eq!(
            readable["resumable"]["rule"],
            serde_json::json!("chain_class_no_failure")
        );
        assert_eq!(
            readable["resumable"]["runner_check"],
            serde_json::json!("boot_time")
        );

        // Corrupt bytes → `false` (bytes-level unreadable).
        let corrupt = serde_json::to_value(project(&row(b"\xff"))).unwrap();
        assert_eq!(corrupt["context_readable"], serde_json::json!(false));
        assert_eq!(
            corrupt["resumable"]["verdict"],
            serde_json::json!("unknown")
        );
        assert_eq!(
            corrupt["resumable"]["rule"],
            serde_json::json!("context_unreadable")
        );
        assert_eq!(
            corrupt["resumable"]["runner_check"],
            serde_json::json!("not_applicable")
        );

        // Valid JSON, unexpected `data` shape → `true` (byte-readable; the
        // shape anomaly lives in the classification/explanation).
        let shape = serde_json::to_value(project(&row(br#"{"data": "not-an-object"}"#))).unwrap();
        assert_eq!(shape["context_readable"], serde_json::json!(true));
        assert_eq!(shape["resumable"]["verdict"], serde_json::json!("unknown"));
        assert_eq!(
            shape["resumable"]["rule"],
            serde_json::json!("context_unreadable")
        );
        assert_eq!(
            shape["resumable"]["runner_check"],
            serde_json::json!("not_applicable")
        );
    }

    #[test]
    fn summary_shape_anomaly_is_readable_but_unexpected_shape() {
        // List-mode projection: same two-class flag from the SQL flags —
        // valid JSON with non-object `data` → `context_readable: true`.
        let summary = CheckpointSummary {
            session_id: "ses_t".to_string(),
            creator_id: "cr_t".to_string(),
            preset_id: "preset_t".to_string(),
            preset_version: 3,
            current_task_id: Some("task_9".to_string()),
            status: "running".to_string(),
            created_at: 1_756_990_000,
            updated_at: 1_756_990_300,
            context_valid_json: true,
            context_data_is_object: false,
            run_status: None,
            run_error: None,
            live_join_keys: None,
        };
        let dto = project_summary(&summary);
        assert_eq!(dto.context_readable, Some(true));
        assert_eq!(dto.resumable.verdict, Verdict::Unknown);
        assert_eq!(dto.resumable.rule, ResumeRule::ContextUnreadable);
        assert!(
            dto.resumable.explanation.contains("shape"),
            "shape wording: {}",
            dto.resumable.explanation
        );
        assert!(
            !dto.resumable.explanation.contains("corrupt"),
            "shape anomaly must not be labelled corrupt: {}",
            dto.resumable.explanation
        );
    }

    #[test]
    fn summary_corrupt_context_is_bytes_unreadable() {
        // List-mode projection: corrupt bytes → `context_readable: false`.
        let summary = CheckpointSummary {
            session_id: "ses_t".to_string(),
            creator_id: "cr_t".to_string(),
            preset_id: "preset_t".to_string(),
            preset_version: 3,
            current_task_id: Some("task_9".to_string()),
            status: "running".to_string(),
            created_at: 1_756_990_000,
            updated_at: 1_756_990_300,
            context_valid_json: false,
            context_data_is_object: false,
            run_status: None,
            run_error: None,
            live_join_keys: None,
        };
        let dto = project_summary(&summary);
        assert_eq!(dto.context_readable, Some(false));
        assert_eq!(dto.resumable.verdict, Verdict::Unknown);
        assert_eq!(dto.resumable.rule, ResumeRule::ContextUnreadable);
        assert!(dto.resumable.explanation.contains("corrupt"));
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
        assert_eq!(render_list(&[], 0), "No checkpointed sessions.\n");
    }

    #[test]
    fn list_count_line_surfaces_honest_total_when_truncated() {
        let ctx = serde_json::json!({"data": {"_merge_j1": ["x"]}}).to_string();
        let dto = project(&row(ctx.as_bytes()));
        let shown = render_list(&[dto.clone(), dto], 5);
        assert!(
            shown.contains("2 of 5+ checkpointed session(s)."),
            "truncated total must be honest: {shown}"
        );
        assert!(!shown.contains("2 checkpointed session(s)."), "{shown}");
    }

    #[test]
    fn list_count_line_plain_when_fully_shown() {
        let ctx = serde_json::json!({"data": {"_merge_j1": ["x"]}}).to_string();
        let dto = project(&row(ctx.as_bytes()));
        assert!(render_list(&[dto], 1).contains("1 checkpointed session(s)."));
    }
}
