//! Shared resume-rule predicates for the checkpoint slice.
//!
//! Single source of truth for the recovery selection rules that the daemon
//! (`resume_driven_sessions`) and the daemon-free `nexus42 ops inspect` CLI
//! BOTH evaluate. Before this module the same predicates lived at two
//! independent code sites; a future rule change would silently desync the
//! CLI verdict from boot re-drive.
//!
//! Two classifiers live here with distinct consumers:
//!
//! 1. **[`classify_recovery`]** — the canonical v1.186 A7 recovery classifier
//!    (P0 Task 2). It consumes the authoritative v1 status plus the
//!    deserialized [`crate::run_state::RunStateV1`] (A2 durable state) and
//!    yields one of seven [`RecoveryClass`] values. Daemon boot/resume, ops
//!    detail, and ops list all call it — there is exactly ONE recovery
//!    precedence for v1 rows (first rule that fires):
//!
//!    1. Authoritative terminal status (`completed`/`failed`/`cancelled`) —
//!       never re-driven, even when the durable state is absent/corrupt.
//!    2. Unreadable row — a v1 row without a parseable/structurally valid
//!       `RunStateV1` blob (missing, corrupt, or type-invalid JSON) or with
//!       an unknown/unsupported status is corrupt/unsupported and
//!       non-replayable. This precedes interrupted evidence (rule 3).
//!    3. Interrupted evidence — v1 status `interrupted`, unresolved
//!       `cancel_requested`, a dispatching/active `in_flight` prompt, or an
//!       unfinished `step_in_flight` mark — wins over old join keys and
//!       never auto-retries.
//!    4. Durable human wait (A4) — a token-bearing `wait` record is a human
//!       wait even when old scheduler join keys exist (scheduler joins must
//!       be represented without a human wait token). Never stepped at boot.
//!    5. Waiting at a parked converge/merge chain without a human wait token
//!       → bounded join resume.
//!    6. Running/paused at a fully committed step boundary: with live chain
//!       keys → bounded join resume; without chain keys → safe boundary
//!       (reconstructable, not auto-driven outside the chain class).
//!    7. v0 rows stay `LegacyUnverified` (status diagnostic only) — the
//!       version contract is enforced at the storage/projection boundary.
//!
//! 2. **[`classify_resumability`] / [`classify_resumability_extracted`]** —
//!    the pinned v1.180 conservative four-rule cascade over the legacy
//!    `serde_json::Map<String, Value>` context shape. It remains ONLY the
//!    v0 projection: rule 1 terminal status, then context readability,
//!    typed `_run_status`/`_run_error` failure records, then converge/merge
//!    chain class. v1 rows never route through it.
//!
//! The old daemon rule 4 (`engine.has_runner`, boot-time in-memory state) is
//! NOT derivable from persisted data and is never part of a verdict here —
//! consumers carry it as the separate `runner_check` caveat.

use crate::engine::SessionStatus;
use crate::run_state::RunStateV1;
use serde_json::{Map, Value};

/// Canonical A7 recovery classification (v1.186 P0, Task 2).
///
/// Single precedence shared by daemon boot/resume ([`resume_driven_sessions`])
/// and daemon-free `nexus42 ops inspect` (`.mstar/iterations/v1.186/guides/architecture-decisions.md`
/// A7). The v1 durable status is authoritative; v0 rows are legacy/unverified.
///
/// Precedence (first rule that fires):
/// 1. Authoritative v1 terminal status (`completed`/`failed`/`cancelled`) — never re-driven.
/// 2. Unreadable row — a v1 row without a parseable state blob is corrupt/unsupported
///    and non-replayable; `load_run`/row projection surface it before this cascade too.
/// 3. Interrupted evidence **wins over old join keys** and never auto-retries: v1 status
///    `interrupted`, unresolved `cancel_requested`, a dispatching/active `in_flight`
///    prompt, or an unfinished `step_in_flight` mark are all `Interrupted` even when live
///    converge/merge join keys exist.
/// 4. Human wait (`waiting_for_input` outside a converge/merge chain) stays distinct and
///    preserves its A4 wait token; never stepped at boot.
/// 5. Shipped converge/merge parked checkpoints: non-terminal, no interrupted evidence,
///    live join keys → bounded join resume.
/// 6. Safe boundary: v1 running/paused with no in-flight markers/wait/chain keys — may
///    reconstruct; boot does not auto-drive outside the chain class today.
/// 7. v0 rows stay `LegacyUnverified` (status diagnostic only) — the conservative legacy
///    classifier below continues to project their resumability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryClass {
    /// Authoritative completed/failed/cancelled (v1).
    Terminal,
    /// Corrupt/unsupported metadata; non-replayable, never silently reinterpreted.
    Unreadable,
    /// In-flight/uncertain work; stopped, never auto-retried.
    Interrupted,
    /// `waiting_for_input` outside a chain — token preserved, not stepped at boot.
    HumanWait,
    /// Parked converge/merge chain with live join keys — bounded join resume only.
    ConvergeMerge,
    /// v1 running/paused at a fully committed step boundary — may reconstruct.
    SafeBoundary,
    /// v0 legacy row: status diagnostic only.
    LegacyUnverified,
}

/// Classify a v1 run record (authoritative status + durable state) against
/// the A7 precedence.
///
/// `gate_park_live` is the exact current-task `_gate_park_<task>` marker used
/// for bounded converge/merge resume.
///
/// `state: None` means the v1 row's state blob is missing/unparseable
/// (corrupt/unsupported) — non-replayable `Unreadable`, except that an
/// authoritative terminal status (`completed`/`failed`/`cancelled`) still
/// classifies `Terminal` first (rule 1 beats rule 2). Within the non-terminal
/// statuses, corrupt/unsupported metadata precedes interrupted evidence
/// (rule 2 beats rule 3); a durable human-wait token beats old scheduler
/// join keys (rule 4 beats rule 5).
#[must_use]
pub const fn classify_recovery(
    status: &SessionStatus,
    state: Option<&RunStateV1>,
    gate_park_live: bool,
) -> RecoveryClass {
    // 1. Authoritative v1 status wins (A2: status is the SSOT for v1 rows).
    //    Terminal lookup stays authoritative even when the durable state
    //    blob is absent/corrupt (A7 rule 1).
    match status {
        SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Cancelled => {
            return RecoveryClass::Terminal;
        }
        _ => {}
    }
    // 2. A v1 row without a parseable/structurally valid state blob is
    //    corrupt/unsupported and non-replayable — this precedes interrupted
    //    evidence, including an explicit `interrupted` status (A7 rule 2).
    let Some(state) = state else {
        return RecoveryClass::Unreadable;
    };
    // 3. Interrupted evidence wins over old join keys; never retried:
    //    explicit interrupted status, unresolved cancel, a dispatching/
    //    active in-flight prompt, or an unfinished step mark.
    if matches!(status, SessionStatus::Interrupted)
        || state.cancel_requested
        || state.in_flight.is_some()
        || state.step_in_flight.is_some()
    {
        return RecoveryClass::Interrupted;
    }
    // 4. A durable A4 human-wait token beats old scheduler join keys: a
    //    token-bearing wait is a human wait even when live converge/merge
    //    join keys exist (a scheduler join park must be represented without
    //    a human wait token).
    if state.wait.is_some() {
        return RecoveryClass::HumanWait;
    }
    // 5. Waiting without a durable wait token: a live join key names a
    //    parked converge/merge chain (bounded join resume); a tokenless
    //    bare wait stays conservatively human-wait-shaped and is never
    //    advanced at boot.
    if matches!(status, SessionStatus::WaitingForInput) {
        return if gate_park_live {
            RecoveryClass::ConvergeMerge
        } else {
            RecoveryClass::HumanWait
        };
    }
    // 6. Running/paused at a committed boundary.
    if gate_park_live {
        RecoveryClass::ConvergeMerge
    } else {
        RecoveryClass::SafeBoundary
    }
}

/// Non-terminal status set — the recovery filter at
/// `storage/sqlite.rs::list_non_terminal_sessions` (rule 1).
#[must_use]
pub fn is_non_terminal_status(status: &str) -> bool {
    matches!(status, "running" | "paused" | "waiting_for_input")
}

/// Extract the `data` map (the `{"data": {...}}` top-level context shape)
/// from a parsed context root.
#[must_use]
pub fn context_data(root: &Value) -> Option<&Map<String, Value>> {
    root.get("data").and_then(Value::as_object)
}

/// Presence of a typed failure record (rule 2): either `_run_status` or
/// `_run_error` is a JSON **string**. Mirrors `graph_flow::Context::get`:
/// `Value::Null` and non-string values are absent.
#[must_use]
pub fn typed_failure_keys_present(data: &Map<String, Value>) -> bool {
    [status_key(), error_key()]
        .iter()
        .any(|k| matches!(data.get(*k), Some(Value::String(_))))
}

/// Typed failure record extracted from `_run_status` / `_run_error`
/// (string values only, mirroring `Context::get`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedFailureRecord {
    pub run_status: Option<String>,
    pub run_error: Option<String>,
}

/// `Some` exactly when [`typed_failure_keys_present`] is `true`.
#[must_use]
pub fn typed_failure_record(data: &Map<String, Value>) -> Option<TypedFailureRecord> {
    let text_value =
        |key: &str| -> Option<String> { data.get(key).and_then(Value::as_str).map(str::to_string) };
    let record = TypedFailureRecord {
        run_status: text_value(status_key()),
        run_error: text_value(error_key()),
    };
    (record.run_status.is_some() || record.run_error.is_some()).then_some(record)
}

/// Live join keys (rule 3): non-null values under the join-tracker prefixes,
/// sorted for deterministic output. Cleared keys are `Value::Null` (never
/// removed) and are not live.
#[must_use]
pub fn live_join_keys(data: &Map<String, Value>) -> Vec<String> {
    let mut keys: Vec<String> = data
        .iter()
        .filter(|(k, v)| !v.is_null() && is_join_key(k))
        .map(|(k, _)| k.clone())
        .collect();
    keys.sort_unstable();
    keys
}

/// Chain-class predicate (rule 3 positive): at least one live join key.
///
/// Round-4 note: the routing writers (`resolve_labeled_target` /
/// `resolve_expression_target` / `record_converge_arrival`) emit
/// `_merge_*` / `_converge_arrivals_*` / `_join_wait_start_*` for ANY routed
/// target — including plain manual-wait states — so this broad test is NOT
/// authoritative that the CURRENT step parked at a scheduler gate. The
/// authoritative scheduler-park evidence is [`gate_park_live`] (the
/// state-scoped `_gate_park_{state_id}` marker the gated task writes only at
/// its genuine gate waits). This predicate remains only as the persisted
/// legacy chain-class view for v0 recovery projections.
#[must_use]
pub fn is_converge_merge_chain(data: &Map<String, Value>) -> bool {
    data.iter().any(|(k, v)| !v.is_null() && is_join_key(k))
}

/// Authoritative current-gate scheduler-park predicate (round 4, Critical 1).
///
/// Returns `true` only when the context carries a live `_gate_park_{task}`
/// marker for the task the run is CURRENTLY parked at. The gated
/// `StateCompositeTask` writes that marker exactly at its merge/converge
/// `WaitForInput` returns and nulls it on gate success/leave/deadline expiry,
/// so a genuine manual/nested wait — even one reached through labeled/
/// conditional routing with stale or broad `_merge_*` / `_converge_arrivals_*`
/// / `_join_wait_start_*` keys — never matches and keeps its fresh retained
/// A4 token (A4 precedence in `classify_recovery` rule 4 unchanged).
#[must_use]
pub fn gate_park_live(data: &Map<String, Value>, current_task_id: &str) -> bool {
    data.get(&format!("_gate_park_{current_task_id}"))
        .is_some_and(|v| !v.is_null())
}

fn is_join_key(key: &str) -> bool {
    key.starts_with("_converge_arrivals_")
        || key.starts_with("_merge_")
        || key.starts_with("_join_wait_start_")
}

const fn status_key() -> &'static str {
    "_run_status"
}

const fn error_key() -> &'static str {
    "_run_error"
}

/// Canonical resume classification (rules 1–4 projection, rule 4 excluded).
///
/// Order matches the daemon: rule 1 (terminal status) first, then context
/// unreadable, then typed failure, then chain class — a row is classified by
/// the first rule that fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeClass {
    TerminalStatus,
    ContextUnreadable,
    TypedFailure,
    NotConvergeMergeClass,
    ChainClassNoFailure,
}

/// Classify from the persisted status + context data map (detail path).
///
/// `data: None` means the context is unreadable (corrupt blob or unexpected
/// `data` shape) — never fabricate a verdict from it.
#[must_use]
pub fn classify_resumability(status: &str, data: Option<&Map<String, Value>>) -> ResumeClass {
    classify_resumability_extracted(
        status,
        data.is_none(),
        data.is_some_and(typed_failure_keys_present),
        data.is_some_and(is_converge_merge_chain),
    )
}

/// Classify from pre-extracted predicates (list path — the storage layer
/// evaluates the predicates in SQL so no `context_json` blob is loaded).
///
/// This is the single cascade: both call sites (detail + list) must produce
/// identical verdicts for the same row, so they share this function.
#[must_use]
pub fn classify_resumability_extracted(
    status: &str,
    context_unreadable: bool,
    typed_failure_present: bool,
    in_chain_class: bool,
) -> ResumeClass {
    if !is_non_terminal_status(status) {
        return ResumeClass::TerminalStatus;
    }
    if context_unreadable {
        return ResumeClass::ContextUnreadable;
    }
    if typed_failure_present {
        return ResumeClass::TypedFailure;
    }
    if !in_chain_class {
        return ResumeClass::NotConvergeMergeClass;
    }
    ResumeClass::ChainClassNoFailure
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data(v: &serde_json::Value) -> Map<String, Value> {
        v.as_object().expect("object").clone()
    }

    #[test]
    fn non_terminal_status_set_matches_recovery_filter() {
        assert!(is_non_terminal_status("running"));
        assert!(is_non_terminal_status("paused"));
        assert!(is_non_terminal_status("waiting_for_input"));
        assert!(!is_non_terminal_status("cancelled"));
        assert!(!is_non_terminal_status("completed"));
        assert!(!is_non_terminal_status("failed"));
        assert!(!is_non_terminal_status(""));
    }

    #[test]
    fn typed_failure_requires_string_values() {
        let map = data(&json!({"_run_status": "failed", "_run_error": "boom"}));
        assert!(typed_failure_keys_present(&map));
        assert_eq!(
            typed_failure_record(&map),
            Some(TypedFailureRecord {
                run_status: Some("failed".to_string()),
                run_error: Some("boom".to_string()),
            })
        );

        // Null / non-string values are absent (Context::get semantics).
        let nulls = data(&json!({"_run_status": null, "_run_error": null}));
        assert!(!typed_failure_keys_present(&nulls));
        assert_eq!(typed_failure_record(&nulls), None);

        let numbers = data(&json!({"_run_status": 5}));
        assert!(!typed_failure_keys_present(&numbers));

        let only_error = data(&json!({"_run_error": "x"}));
        assert!(typed_failure_keys_present(&only_error));
        assert_eq!(
            typed_failure_record(&only_error),
            Some(TypedFailureRecord {
                run_status: None,
                run_error: Some("x".to_string()),
            })
        );
    }

    #[test]
    fn live_keys_ignore_null_and_sort() {
        let map = data(&json!({
            "_merge_j2": ["x"],
            "_join_wait_start_j1": 1,
            "_converge_arrivals_j1": ["a"],
            "_converge_arrivals_cleared": null
        }));
        assert_eq!(
            live_join_keys(&map),
            ["_converge_arrivals_j1", "_join_wait_start_j1", "_merge_j2"]
        );
        assert!(is_converge_merge_chain(&map));

        let cleared = data(&json!({
            "_converge_arrivals_j1": null,
            "_merge_j1": null,
            "_join_wait_start_j1": null
        }));
        assert!(live_join_keys(&cleared).is_empty());
        assert!(!is_converge_merge_chain(&cleared));
    }

    // Round-4 Critical 1: the current-gate park marker is the ONLY
    // authoritative scheduler-park evidence. A genuine manual wait reached
    // through labeled/conditional routing carries broad join keys but NO
    // marker for its own state — `gate_park_live` must return false there,
    // while a parked merge/converge gate carries a live marker for the
    // CURRENT task.
    #[test]
    fn gate_park_marker_is_current_task_authoritative() {
        // Parked gate: live marker for the current task.
        let parked = data(&json!({
            "_gate_park_join": true,
            "_converge_arrivals_join": ["a"],
            "_join_wait_start_join": 1
        }));
        assert!(gate_park_live(&parked, "join"));
        // A DIFFERENT current task (e.g. the routed manual wait) is not the
        // gate — the marker names the gate state, not this task.
        assert!(!gate_park_live(&parked, "manual_wait_state"));

        // Manual wait with broad/stale join keys but no marker for its own
        // state: never classified as a scheduler park.
        let manual_with_joins = data(&json!({
            "_converge_arrivals_manual_wait_state": ["pred"],
            "_merge_manual_wait_state": ["go"],
            "_join_wait_start_join": 1
        }));
        assert!(!gate_park_live(&manual_with_joins, "manual_wait_state"));

        // Cleared marker (null) is not live.
        let cleared_marker = data(&json!({"_gate_park_join": null}));
        assert!(!gate_park_live(&cleared_marker, "join"));

        // Absent marker + no keys: not a park.
        let plain = data(&json!({"_creator_id": "c"}));
        assert!(!gate_park_live(&plain, "join"));
        // A live marker with the same key name but value present for the
        // current task is live regardless of other stale keys.
        let marked_with_stale = data(&json!({
            "_gate_park_manual_wait_state": true,
            "_converge_arrivals_other": ["x"],
            "_join_wait_start_other": 5
        }));
        assert!(gate_park_live(&marked_with_stale, "manual_wait_state"));
    }

    #[test]
    fn classification_cascade_rule_1_first() {
        let chain = data(&json!({"_merge_j1": ["x"]}));
        // Terminal status wins even with live join keys (schedule-cancel).
        assert_eq!(
            classify_resumability("cancelled", Some(&chain)),
            ResumeClass::TerminalStatus
        );
        assert_eq!(
            classify_resumability("completed", Some(&chain)),
            ResumeClass::TerminalStatus
        );
    }

    #[test]
    fn classification_cascade_matches_daemon_order() {
        let chain = data(&json!({"_merge_j1": ["x"]}));
        let failed = data(&json!({"_run_error": "boom", "_merge_j1": ["x"]}));
        let plain = data(&json!({"_creator_id": "c"}));

        assert_eq!(
            classify_resumability("running", Some(&chain)),
            ResumeClass::ChainClassNoFailure
        );
        assert_eq!(
            classify_resumability("running", Some(&failed)),
            ResumeClass::TypedFailure
        );
        assert_eq!(
            classify_resumability("running", Some(&plain)),
            ResumeClass::NotConvergeMergeClass
        );
        assert_eq!(
            classify_resumability("running", None),
            ResumeClass::ContextUnreadable
        );
    }

    #[test]
    fn extracted_classification_matches_map_classification() {
        let chain = data(&json!({"_merge_j1": ["x"]}));
        let cases = [
            ("running", Some(&chain)),
            ("cancelled", Some(&chain)),
            ("running", None),
            ("paused", Some(&data(&json!({"_run_error": "e"})))),
            ("waiting_for_input", Some(&data(&json!({"a": 1})))),
        ];
        for (status, map) in cases {
            let from_map = classify_resumability(status, map);
            let from_extracted = classify_resumability_extracted(
                status,
                map.is_none(),
                map.is_some_and(typed_failure_keys_present),
                map.is_some_and(is_converge_merge_chain),
            );
            assert_eq!(
                from_map, from_extracted,
                "map and extracted classification must agree for {status:?} / {map:?}"
            );
        }
    }

    #[test]
    fn context_data_extracts_map_only() {
        let root = json!({"data": {"_merge_j1": ["x"]}});
        let map = context_data(&root).expect("data map");
        assert_eq!(live_join_keys(map), ["_merge_j1"]);

        assert!(context_data(&json!({"data": "not-an-object"})).is_none());
        assert!(context_data(&json!({"data": []})).is_none());
        assert!(context_data(&json!({})).is_none());
    }

    // ------------------------------------------------------------------
    // v1.186 P0 Task 2 — canonical A7 recovery classification
    // ------------------------------------------------------------------

    fn state(step_in_flight: Option<&str>, cancel_requested: bool, wait: bool) -> RunStateV1 {
        RunStateV1 {
            wait: wait.then(|| crate::run_state::WaitRecord {
                wait_id: "w1".to_string(),
                task_id: "t1".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: crate::run_state::WaitKind::Manual,
            }),
            step_in_flight: step_in_flight.map(str::to_string),
            in_flight: None,
            failure: None,
            cancel_requested,
        }
    }

    #[test]
    fn a7_terminal_status_wins_over_all_evidence() {
        use crate::engine::SessionStatus::*;
        for status in [Completed, Failed, Cancelled] {
            // Even with interrupted evidence + chain keys, terminal wins.
            let s = state(Some("t"), true, false);
            assert_eq!(
                classify_recovery(&status, Some(&s), true),
                RecoveryClass::Terminal
            );
        }
    }

    #[test]
    fn a7_missing_state_blob_is_unreadable_non_replayable() {
        use crate::engine::SessionStatus::Running;
        assert_eq!(
            classify_recovery(&Running, None, true),
            RecoveryClass::Unreadable
        );
    }

    #[test]
    fn a7_interrupted_wins_over_old_join_keys_and_never_retries() {
        use crate::engine::SessionStatus::*;
        // In-flight mark + live chain keys → Interrupted, not ConvergeMerge.
        assert_eq!(
            classify_recovery(&Running, Some(&state(Some("t3"), false, false)), true),
            RecoveryClass::Interrupted
        );
        // Unresolved cancel + chain keys → Interrupted.
        assert_eq!(
            classify_recovery(&Running, Some(&state(None, true, false)), true),
            RecoveryClass::Interrupted
        );
        // Explicit interrupted status → Interrupted even with chain keys.
        assert_eq!(
            classify_recovery(&Interrupted, Some(&state(None, false, false)), true),
            RecoveryClass::Interrupted
        );
    }

    #[test]
    fn a7_human_wait_stays_distinct_and_is_never_stepped_at_boot() {
        use crate::engine::SessionStatus::WaitingForInput;
        // No live join keys → HumanWait (A4 token preserved, not advanced).
        assert_eq!(
            classify_recovery(&WaitingForInput, Some(&state(None, false, true)), false),
            RecoveryClass::HumanWait
        );
        // A durable human-wait token beats old scheduler join keys — a
        // token-bearing wait is HumanWait even in a chain-shaped context
        // (scheduler joins must be represented without a human wait token).
        assert_eq!(
            classify_recovery(&WaitingForInput, Some(&state(None, false, true)), true),
            RecoveryClass::HumanWait
        );
        // Tokenless wait at live chain keys → parked converge/merge join
        // resume (bounded re-drive only).
        assert_eq!(
            classify_recovery(&WaitingForInput, Some(&state(None, false, false)), true),
            RecoveryClass::ConvergeMerge
        );
    }

    #[test]
    fn a7_terminal_status_wins_even_when_state_absent_or_corrupt() {
        use crate::engine::SessionStatus::*;
        // A terminal row with NO durable state blob is still terminal
        // (A7 rule 1 beats rule 2) — lookup stays authoritative without a
        // live runner.
        for status in [Completed, Failed, Cancelled] {
            assert_eq!(
                classify_recovery(&status, None, false),
                RecoveryClass::Terminal,
                "terminal {status:?} with absent state must stay terminal"
            );
        }
    }

    #[test]
    fn a7_corrupt_state_precedes_interrupted_status() {
        use crate::engine::SessionStatus::Interrupted;
        // A7 rule 2 (corrupt/unsupported metadata) fires before rule 3
        // (interrupted evidence): `Interrupted` status + missing state blob
        // is Unreadable, not Interrupted.
        assert_eq!(
            classify_recovery(&Interrupted, None, false),
            RecoveryClass::Unreadable
        );
        // With a readable state blob, the interrupted status fires rule 3.
        assert_eq!(
            classify_recovery(&Interrupted, Some(&state(None, false, false)), false),
            RecoveryClass::Interrupted
        );
    }

    #[test]
    fn a7_safe_boundary_and_converge_merge_running() {
        use crate::engine::SessionStatus::{Paused, Running};
        assert_eq!(
            classify_recovery(&Running, Some(&RunStateV1::default()), false),
            RecoveryClass::SafeBoundary
        );
        assert_eq!(
            classify_recovery(&Paused, Some(&RunStateV1::default()), false),
            RecoveryClass::SafeBoundary
        );
        // Running with live chain keys → converge/merge (existing re-drive).
        assert_eq!(
            classify_recovery(&Running, Some(&RunStateV1::default()), true),
            RecoveryClass::ConvergeMerge
        );
    }
}
