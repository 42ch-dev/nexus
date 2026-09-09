//! `nexus42 ops` — hidden operator group (V1.182 P1 BL-04, Task 2).
//!
//! `ops inspect [SESSION_ID] [--json]` is a **daemon-free, read-only** view
//! over the v1.180 checkpoint slice (`orchestration_sessions`): it opens the
//! workspace `state.db` with `nexus_local_db::open_pool_read_only` (no
//! migrations, no seed, no lock upgrades) and projects the v1.186 A7
//! seven-class recovery classifier
//! (`nexus_orchestration::resume_rules::classify_recovery` — terminal /
//! unreadable / interrupted / `human_wait` / `converge_merge` / `safe_boundary` /
//! `legacy_unverified`) into a resumable verdict via the shared
//! `nexus_orchestration::resume_rules` module; the daemon's boot-time
//! in-memory half (`engine.has_runner`, runner reconstruction) is carried as
//! the separate `runner_check` caveat — never folded into the verdict. v0
//! rows fall back to the conservative four-rule legacy cascade
//! (`classify_resumability`) — terminal status, context readability, typed
//! failure, chain class — which the daemon applies to v0/no-store rows only.
//!
//! Contract: `.mstar/sdd/2026-09-03-v1.182-p1-bl04-checkpoint-resume-ux/inspect-contract.md`.
//!
//! Honesty discipline:
//! - `db_status` is the raw DB column. For **v1 rows** (`execution_version == 1`)
//!   it is the authoritative status (A2 — written by `commit_transition`); for
//!   **v0 legacy rows** (`execution_version == 0`) it stays diagnostic only
//!   (every legacy save writes `'running'`; ON CONFLICT never updated it).
//!   Negative/forward execution versions are unsupported and surface as
//!   `recovery_class: unreadable` (never coerced to legacy or v1). The
//!   v0/v1 split is surfaced via `execution_version` plus the canonical A7
//!   `recovery_class`.
//! - Corrupt `context_json` → `verdict: "unknown"` / `context_unreadable`;
//!   no verdict is fabricated from unreadable data. `context_readable` is a
//!   two-class flag: `false` ONLY for corrupt bytes; valid-JSON-unexpected-
//!   shape is byte-readable → `true`, with the shape anomaly carried in the
//!   classification/explanation. A v1 row whose durable `run_state_json`
//!   blob is missing/unparseable (or whose execution version is unsupported)
//!   is `recovery_class: unreadable` with an explicit **non-replayable
//!   metadata** reason — corrupt v1 run metadata is not the legacy
//!   `context_unreadable` wording, even when the session `context_json` is
//!   perfectly readable (a readable context does not make v1 metadata
//!   replayable).
//! - `wait_id` is exposed ONLY when the canonical class is `human_wait`; a
//!   terminal/interrupted/unreadable row carrying stale wait bytes must not
//!   advertise a usable wait token.
//! - The checkpoint stores POSITION ONLY — there is no completed-stages
//!   ledger, so the output never claims one.
//! - Slice boundary: read-only inspect; the CLI never implies resume can be
//!   triggered from here (re-drive happens on next daemon boot).

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_orchestration::resume_rules::{self, RecoveryClass, ResumeClass};
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
    /// Raw DB status column — authoritative for v1 rows, diagnostic for v0.
    db_status: String,
    /// Execution version: `0` = legacy/unverified, `1` = v1 authoritative
    /// (A2); negative/forward versions are unsupported (unreadable).
    execution_version: i64,
    /// State revision (CAS anchor; `0` on v0 rows).
    state_revision: i64,
    /// Canonical A7 recovery class (v1.186 P0, Task 2) — shared with daemon
    /// boot/resume; always present (`unreadable` covers corrupt/unsupported
    /// rows, `legacy_unverified` covers v0 rows).
    recovery_class: RecoveryClass,
    /// Durable human-wait token (A4) for v1 `waiting_for_input` rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    wait_id: Option<String>,
    /// Legal operator actions for the canonical recovery class (shared A2
    /// projection; tri-QC P1-B).
    allowed_actions: Vec<String>,
    /// Stable machine reason for an uncertain/blocked outcome (e.g. a human
    /// wait whose frozen source can no longer be reconstructed).
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<String>,
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
/// [`ResumeClass`] projected into the DTO ([`From<ResumeClass>`]); v1.186
/// adds the A7 recovery-class rules used when the row's durable v1 state is
/// authoritative.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ResumeRule {
    TerminalStatus,
    ContextUnreadable,
    /// A7: v1 run-state metadata is corrupt/unsupported (non-replayable).
    /// Distinct from [`ResumeRule::ContextUnreadable`] — the session
    /// `context_json` may be perfectly readable while the v1 durable
    /// metadata is not.
    UnreadableMetadata,
    TypedFailure,
    NotConvergeMergeClass,
    ChainClassNoFailure,
    /// A7: interrupted/uncertain in-flight work; never auto-retried.
    Interrupted,
    /// A7: human wait; token preserved, never stepped at boot.
    HumanWait,
    /// A7: fully committed step boundary; may reconstruct, not auto-driven.
    SafeBoundary,
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

/// Parse the durable v1 run state blob from a checkpoint row.
///
/// `None` for v0 rows (no blob, legacy/unverified) and for corrupt/absent/
/// type-invalid blobs (surfaced as `recovery_class: unreadable` under A7,
/// never fabricated). Structural validation happens here — JSON that parses
/// but does not deserialize as [`RunStateV1`] (e.g. `{"cancel_requested":
/// "false"}`) returns `None` just like corrupt bytes, so list and detail
/// agree.
fn parse_run_state(bytes: Option<&[u8]>) -> Option<nexus_orchestration::run_state::RunStateV1> {
    let bytes = bytes?;
    serde_json::from_slice(bytes).ok()
}

fn descriptor_is_valid(bytes: Option<&[u8]>) -> bool {
    bytes.is_some_and(|bytes| {
        serde_json::from_slice::<nexus_orchestration::run_state::RunDescriptorV1>(bytes).is_ok()
    })
}

/// Compute the canonical A7 recovery class from the authoritative status +
/// durable state (shared by detail and list projection).
///
/// Exact execution-version contract (A7 / `load_run`): only `0` (legacy,
/// unverified) and `1` (v1 authoritative) are accepted. Negative and
/// forward versions are unsupported and non-replayable — they surface as
/// [`RecoveryClass::Unreadable`], never coerced to legacy or v1. `state`
/// is the structurally deserialized [`RunStateV1`] (`None` = absent/
/// corrupt/type-invalid); terminal status still classifies `Terminal`
/// first via the shared [`resume_rules::classify_recovery`].
fn recovery_class_for(
    execution_version: i64,
    status: &str,
    state: Option<&nexus_orchestration::run_state::RunStateV1>,
    descriptor_valid: bool,
    gate_park_live: bool,
) -> RecoveryClass {
    match execution_version {
        0 => RecoveryClass::LegacyUnverified,
        1 => {
            if !descriptor_valid {
                return RecoveryClass::Unreadable;
            }
            // A v1 row's status must be a known A2 status; anything else is
            // corrupt/ambiguous and non-replayable (mirrors `load_run`).
            let Some(status) = nexus_orchestration::engine::SessionStatus::from_db_str(status)
            else {
                return RecoveryClass::Unreadable;
            };
            resume_rules::classify_recovery(&status, state, gate_park_live)
        }
        // Negative or forward (>= 2) execution versions are unsupported.
        _ => RecoveryClass::Unreadable,
    }
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

    // Canonical A7 recovery class (v1.186 P0, Task 2): the durable v1
    // status/state are the authority for v1 rows; v0 rows are legacy.
    // Terminal status wins over absent/corrupt state via the shared
    // classifier (A7 rule 1). `parse_run_state` returns `None` for v0 rows
    // (no blob) and corrupt/absent blobs; v1 state (and its wait token) is
    // gated on EXACT execution_version == 1 — v0 stray bytes must never be
    // interpreted through v1 wait semantics (Important 4), and unsupported
    // negative/forward versions surface as `Unreadable` (Important 2).
    let run_state = parse_run_state(row.run_state_json.as_deref());
    let gate_park = shape.as_ref().is_ok_and(|data| {
        resume_rules::gate_park_live(data, row.current_task_id.as_deref().unwrap_or_default())
    });
    let recovery_class = recovery_class_for(
        row.execution_version,
        &row.status,
        run_state.as_ref(),
        descriptor_is_valid(row.run_descriptor_json.as_deref()),
        gate_park,
    );
    // Durable human-wait token (A4): exposed ONLY when the canonical class
    // is `HumanWait`. A terminal/interrupted/unreadable row carrying stale
    // wait bytes must not advertise a usable wait token (round-2 Minor).
    let wait_id = (recovery_class == RecoveryClass::HumanWait)
        .then_some(run_state.as_ref())
        .flatten()
        .and_then(|s| s.wait.as_ref().map(|w| w.wait_id.clone()));
    // Read-only frozen-source verification (A7): a human wait whose source
    // can no longer be reconstructed must not advertise continue.
    let source_ok = frozen_source_reconstructable(row.run_descriptor_json.as_deref());

    // For v1 rows the resumable verdict is projected from the canonical A7
    // class (authoritative status/state) — the legacy context-key cascade
    // stays the v0 projection only, so durable interrupted evidence is never
    // hidden. `LegacyUnverified` routes through the legacy `resumable` above.
    let resumable = match recovery_class {
        RecoveryClass::LegacyUnverified => resumable,
        class => verdict_for_recovery(class, &row.status),
    };

    InspectDto {
        session_id: row.session_id.clone(),
        creator_id: row.creator_id.clone(),
        preset_id: row.preset_id.clone(),
        preset_version: row.preset_version,
        db_status: row.status.clone(),
        execution_version: row.execution_version,
        state_revision: row.state_revision,
        recovery_class,
        wait_id,
        allowed_actions: allowed_actions_for(recovery_class, source_ok),
        reason_code: (recovery_class == RecoveryClass::HumanWait && !source_ok)
            .then(|| "reconstruction_unavailable".to_string()),
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
/// evaluated the legacy resume-rule predicates in SQL (no `context_json`
/// loaded) for the v0 verdict, and projects the v1 durable state RAW
/// (`run_state_json`). The canonical A7 recovery class is computed by the
/// SAME shared function as detail mode and the daemon
/// ([`recovery_class_for`] → [`resume_rules::classify_recovery`]) — list
/// and detail must never drift: both structurally deserialize `RunStateV1`
/// and both enforce the exact execution-version 0/1 contract, so
/// syntactically-valid-but-structurally-corrupt JSON is `Unreadable` in
/// both modes and unsupported versions are never coerced.
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

    // Canonical A7 recovery class (v1.186 P0, Task 2): the exact same
    // deserialization + version contract as detail mode. v1 state blobs are
    // validated structurally here (equivalent to `RunStateV1`
    // deserialization), so `json_valid` alone never labels corrupt evidence
    // as readable; v0 rows stay legacy/unverified; negative/forward
    // versions are unsupported/unreadable; the wait token is gated on exact
    // version 1.
    let run_state = parse_run_state(row.run_state_json.as_deref());
    let recovery_class = recovery_class_for(
        row.execution_version,
        &row.status,
        run_state.as_ref(),
        descriptor_is_valid(row.run_descriptor_json.as_deref()),
        row.gate_park_live,
    );
    // Durable human-wait token (A4): exposed ONLY when the canonical class
    // is `HumanWait` (round-2 Minor — list must agree with detail).
    let wait_id = (recovery_class == RecoveryClass::HumanWait)
        .then_some(run_state.as_ref())
        .flatten()
        .and_then(|s| s.wait.as_ref().map(|w| w.wait_id.clone()));
    // Read-only frozen-source verification (A7): a human wait whose source
    // can no longer be reconstructed must not advertise continue.
    let source_ok = frozen_source_reconstructable(row.run_descriptor_json.as_deref());

    let legacy_resumable = verdict_for(
        row.status.as_str(),
        resume_rules::classify_resumability_extracted(
            row.status.as_str(),
            context_unreadable,
            row.run_status.is_some() || row.run_error.is_some(),
            has_live_join_keys,
        ),
        unreadable_kind,
    );
    // For v1 rows the verdict is projected from the canonical A7 class
    // (authoritative status/state) — the legacy context-key cascade stays
    // the v0 projection only, so durable interrupted evidence is never
    // hidden. `LegacyUnverified` routes through the legacy cascade above.
    let resumable = match recovery_class {
        RecoveryClass::LegacyUnverified => legacy_resumable,
        class => verdict_for_recovery(class, &row.status),
    };

    InspectDto {
        session_id: row.session_id.clone(),
        creator_id: row.creator_id.clone(),
        preset_id: row.preset_id.clone(),
        preset_version: row.preset_version,
        db_status: row.status.clone(),
        execution_version: row.execution_version,
        state_revision: row.state_revision,
        recovery_class,
        wait_id,
        allowed_actions: allowed_actions_for(recovery_class, source_ok),
        reason_code: (recovery_class == RecoveryClass::HumanWait && !source_ok)
            .then(|| "reconstruction_unavailable".to_string()),
        current_task_id: row.current_task_id.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
        run_failure,
        live_join_keys,
        resumable,
        context_readable,
    }
}

/// Build the resumable verdict for a canonical A7 recovery class (v1.186
/// P0, Task 2). v1 rows carry authoritative status/state, so the verdict
/// comes from [`RecoveryClass`], never from the legacy context-key cascade.
/// For `LegacyUnverified` (v0) the caller falls back to `verdict_for`.
fn verdict_for_recovery(class: RecoveryClass, row_status: &str) -> ResumableVerdict {
    match class {
        RecoveryClass::Terminal => verdict_for(row_status, ResumeClass::TerminalStatus, None),
        RecoveryClass::Unreadable => ResumableVerdict {
            verdict: Verdict::Unknown,
            rule: ResumeRule::UnreadableMetadata,
            runner_check: RunnerCheck::NotApplicable,
            // The v1 durable run-state metadata is corrupt/unsupported (or
            // the execution version is negative/forward) — an explicit
            // non-replayable metadata reason, never the legacy
            // context_unreadable wording. The session `context_json` being
            // readable does not make v1 metadata replayable.
            explanation: "v1 run-state metadata unreadable (corrupt/unsupported \
                          or unsupported execution version) — non-replayable; \
                          no verdict fabricated"
                .to_string(),
        },
        RecoveryClass::ConvergeMerge => {
            verdict_for(row_status, ResumeClass::ChainClassNoFailure, None)
        }
        RecoveryClass::Interrupted => ResumableVerdict {
            verdict: Verdict::No,
            rule: ResumeRule::Interrupted,
            runner_check: RunnerCheck::NotApplicable,
            explanation: "interrupted/uncertain in-flight work — stopped and never auto-retried; \
                          an operator may cancel after ownership-safe cleanup"
                .to_string(),
        },
        RecoveryClass::HumanWait => ResumableVerdict {
            verdict: Verdict::No,
            rule: ResumeRule::HumanWait,
            runner_check: RunnerCheck::NotApplicable,
            explanation:
                "human wait (A4) — wait token preserved; never stepped or approved at boot"
                    .to_string(),
        },
        RecoveryClass::SafeBoundary => ResumableVerdict {
            verdict: Verdict::No,
            rule: ResumeRule::SafeBoundary,
            runner_check: RunnerCheck::NotApplicable,
            explanation: "fully committed step boundary with no in-flight work — reconstructable, \
                          but boot does not auto-drive outside the converge/merge chain class"
                .to_string(),
        },
        RecoveryClass::LegacyUnverified => {
            unreachable!(
                "LegacyUnverified routes through the legacy cascade, not verdict_for_recovery"
            )
        }
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

/// Stable human word for a [`RecoveryClass`] (same wording as detail and
/// list output; the DTO `serde` name stays the machine contract).
/// Read-only frozen-source verification (A7): the persisted descriptor's
/// source must still resolve and hash-match at its frozen version. No runner
/// is attached and no state is mutated.
fn frozen_source_reconstructable(bytes: Option<&[u8]>) -> bool {
    use nexus_orchestration::preset::{load_embedded_preset, load_preset};
    use nexus_orchestration::run_state::PresetSourceIdentity;

    let Some(bytes) = bytes else {
        return false;
    };
    let Ok(descriptor) =
        serde_json::from_slice::<nexus_orchestration::run_state::RunDescriptorV1>(bytes)
    else {
        return false;
    };
    let caps = ops_capability_registry();
    let loaded = match &descriptor.source {
        PresetSourceIdentity::Embedded { preset_id, .. } => load_embedded_preset(preset_id, &caps),
        PresetSourceIdentity::Directory { root, .. } => load_preset(root, &caps),
    };
    match loaded {
        Ok(loaded) => {
            loaded.source_identity.as_ref() == Some(&descriptor.source)
                && loaded.version == descriptor.preset_version
        }
        Err(_) => false,
    }
}

/// Capability registry for the daemon-free verification: built-ins plus the
/// user-installed capabilities under `$HOME/.nexus42/capabilities`, so a
/// user preset that requires a user capability still verifies (Greptile P1).
fn ops_capability_registry() -> nexus_orchestration::capability::CapabilityRegistry {
    use nexus_orchestration::capability::{CapabilityRegistry, CapabilityRuntimeDeps};

    let Ok(nexus_home) = crate::config::nexus_home() else {
        return CapabilityRegistry::with_builtins();
    };
    let Some(user_home) = nexus_home.parent() else {
        return CapabilityRegistry::with_builtins();
    };
    let user_caps_dir = nexus_home_layout::user_capabilities_dir(user_home);
    if !user_caps_dir.exists() {
        return CapabilityRegistry::with_builtins();
    }
    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: None,
        session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &user_caps_dir).0
}

const fn recovery_class_str(class: RecoveryClass) -> &'static str {
    match class {
        RecoveryClass::Terminal => "terminal",
        RecoveryClass::Unreadable => "unreadable",
        RecoveryClass::Interrupted => "interrupted",
        RecoveryClass::HumanWait => "human_wait",
        RecoveryClass::ConvergeMerge => "converge_merge",
        RecoveryClass::SafeBoundary => "safe_boundary",
        RecoveryClass::LegacyUnverified => "legacy_unverified",
    }
}

/// Legal operator actions for the canonical recovery class — the same
/// mapping the daemon projection uses (A2; tri-QC P1-B).
fn allowed_actions_for(class: RecoveryClass, source_reconstructable: bool) -> Vec<String> {
    let actions: &[&str] = match class {
        RecoveryClass::Terminal => &["new_run"],
        // A human wait whose frozen source no longer verifies must not
        // advertise continue (tri-QC P1-C).
        RecoveryClass::HumanWait if !source_reconstructable => &["cancel", "new_run"],
        RecoveryClass::HumanWait => &["continue", "cancel"],
        RecoveryClass::Interrupted
        | RecoveryClass::LegacyUnverified
        | RecoveryClass::Unreadable => &["cancel", "new_run"],
        RecoveryClass::SafeBoundary | RecoveryClass::ConvergeMerge => &["cancel"],
    };
    actions.iter().map(|action| (*action).to_string()).collect()
}

fn render_detail(dto: &InspectDto) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "session:        {}", dto.session_id);
    let _ = writeln!(out, "creator:        {}", dto.creator_id);
    let _ = writeln!(out, "preset:         {}", dto.preset_id);
    let _ = writeln!(out, "preset_version: {}", dto.preset_version);
    let _ = writeln!(out, "status:         {}", dto.db_status);
    let _ = writeln!(
        out,
        "recovery_class: {}",
        recovery_class_str(dto.recovery_class)
    );
    let _ = writeln!(out, "allowed_actions: {}", dto.allowed_actions.join(", "));
    if let Some(reason) = &dto.reason_code {
        let _ = writeln!(out, "reason_code:    {reason}");
    }
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
        ResumeRule::Interrupted => "no — interrupted/uncertain in-flight work (never auto-retried; cancel after ownership-safe cleanup)".to_string(),
        ResumeRule::HumanWait => "no — human wait (A4) token preserved; never stepped or approved at boot".to_string(),
        ResumeRule::SafeBoundary => "no — fully committed boundary with no in-flight work; boot does not auto-drive outside the chain class".to_string(),
        // A7 unreadable metadata (round-2): explicit non-replayable reason —
        // distinct from a corrupt/unshaped session context. Contract §5
        // wording split (qc3 S1): corrupt bytes vs parseable-but-unexpected
        // shape — the DTO explanation already distinguishes the two honest
        // wordings, so the human line mirrors the JSON explanation instead of
        // always claiming a corrupt context_json.
        ResumeRule::UnreadableMetadata | ResumeRule::ContextUnreadable => {
            format!("unknown — {}", dto.resumable.explanation)
        }
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
            "{}  {}  {}  {}  {}  {}  {}",
            row.session_id,
            row.preset_id,
            row.db_status,
            recovery_class_str(row.recovery_class),
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
            execution_version: 0,
            state_revision: 0,
            run_state_json: None,
            run_descriptor_json: None,
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
            execution_version: 0,
            state_revision: 0,
            run_state_json: None,
            created_at: 1_756_990_000,
            updated_at: 1_756_990_300,
            context_valid_json: true,
            context_data_is_object: false,
            run_status: None,
            run_error: None,
            live_join_keys: None,
            run_descriptor_json: None,
            gate_park_live: false,
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
            execution_version: 0,
            state_revision: 0,
            run_state_json: None,
            created_at: 1_756_990_000,
            updated_at: 1_756_990_300,
            context_valid_json: false,
            context_data_is_object: false,
            run_status: None,
            run_error: None,
            live_join_keys: None,
            run_descriptor_json: None,
            gate_park_live: false,
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
