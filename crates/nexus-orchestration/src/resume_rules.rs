//! Shared resume-rule predicates for the v1.180 checkpoint slice
//! (V1.182 P1 BL-04 QC fix wave — item 4, qc1 W1).
//!
//! Single source of truth for the `resume_driven_sessions` rules that the
//! daemon (`crates/nexus-daemon-runtime/src/preset_run.rs`) and the
//! daemon-free `nexus42 ops inspect` CLI (`apps/nexus42/src/commands/ops.rs`)
//! BOTH evaluate. Before this module the same predicates lived at two
//! independent code sites (`preset_run.rs:349-405` and `ops.rs:222-329`);
//! a future rule change would silently desync the CLI verdict from boot
//! re-drive. Predicates here operate on the generic
//! `serde_json::Map<String, Value>` form (the inspection surface's public
//! data shape); the daemon adapts its `graph_flow::Context` via
//! [`context_data`].
//!
//! Rules projected (order is authoritative — matches `resume_driven_sessions`
//! plus the boot recovery filter):
//!   1. non-terminal status (`running`/`paused`/`waiting_for_input` — the
//!      recovery filter at `storage/sqlite.rs`); a terminal row (e.g.
//!      `'cancelled'` written by the schedule-cancel handler) is never a
//!      re-drive candidate, even with live join keys.
//!   2. context unreadable (corrupt blob, or parseable but unexpected
//!      `data` shape) → no verdict fabricated.
//!   3. typed failure record (`_run_status`/`_run_error` as JSON strings).
//!   4. converge/merge chain class: any non-null live join key
//!      (`_converge_arrivals_*` / `_merge_*` / `_join_wait_start_*`).
//!
//! Rule 4 of the daemon (`engine.has_runner`, boot-time in-memory state) is
//! NOT derivable from persisted data and is never part of a verdict here —
//! consumers carry it as the separate `runner_check` caveat.

use serde_json::{Map, Value};

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
#[must_use]
pub fn is_converge_merge_chain(data: &Map<String, Value>) -> bool {
    data.iter().any(|(k, v)| !v.is_null() && is_join_key(k))
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
}
