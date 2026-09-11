//! `HostEvent` mapping for the `deepseek_harness_sdk` high-level surface
//! (locks § AR-1 dsh table, AR-6, AR-7; v1.188 P0 SDK 0.2 cutover).
//!
//! Converts one `Session::run` outcome into `HostEvent`s. `completed` is
//! the ONLY successful finish reason: it becomes exactly one
//! `MessageDelta(final_response)` plus one terminal `OpFinished(EndTurn)`
//! (AR-6: no incremental deltas on this surface). A missing, unknown,
//! `max-tokens` or `error` finish reason is a failed turn and maps to
//! exactly one terminal `OpFailed` — never `OpFinished`. A run error
//! becomes exactly one `OpFailed` with the AR-7 category tokens. The
//! crate owns the runtime spawn, the stdio JSON-RPC wire parser, and the
//! inbox-receipt / root-idle algorithm (`Session::run`); Nexus owns only
//! the event normalization. Consumed by the dsh provider execute loop.

use deepseek_harness_sdk::{Error, RunResult};

use crate::capability::model::{
    FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, TextDeltaEvent,
};
use crate::ids::{HostOperationId, HostSessionId};

/// Map one dsh turn result into host events (AR-1 dsh table, AR-6).
///
/// `completed` is the only successful SDK 0.2 `turn/end` kind: exactly
/// one `MessageDelta` carrying `final_response` (which may be empty — the
/// SDK derives it from the last `assistant/message` event and never falls
/// back to an earlier one), followed by exactly one terminal
/// `OpFinished(EndTurn)`. Anything else — an absent `finish_reason`, an
/// unknown token, `max-tokens`, or `error` — cannot become `EndTurn`:
/// the turn failed and maps to exactly one terminal `OpFailed` with a
/// stable category token and static diagnostic text (result payloads are
/// never echoed into diagnostics).
#[must_use]
pub fn map_run_result(
    result: &RunResult,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Vec<HostEvent> {
    match result.finish_reason.as_deref() {
        Some("completed") => vec![
            HostEvent::MessageDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text: result.final_response.clone(),
            }),
            HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                reason: FinishReason::EndTurn,
            }),
        ],
        Some("max-tokens") => vec![HostEvent::OpFailed(OperationFailedEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            error_category: "max_tokens".to_string(),
            error_message: "dsh turn reached the maximum token limit".to_string(),
        })],
        _ => vec![HostEvent::OpFailed(OperationFailedEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            error_category: "provider_error".to_string(),
            error_message: "dsh turn ended without a completed finish reason".to_string(),
        })],
    }
}

/// Classify a `deepseek_harness_sdk` start/run error into the one terminal
/// `OpFailed` event of the failed turn (PD-3, AR-7 category tokens).
///
/// Every SDK 0.2 variant is matched explicitly — a newly added variant
/// fails the build instead of silently degrading into a generic bucket.
/// The SDK surfaces typed-decode and protocol violations as
/// `Error::SdkProtocol` (malformed `session.event` / `session.status`
/// payloads during `Session::run`, `turn/end` reason extraction failures,
/// missing server identity / message id); `JsonRpc` is a JSON-RPC error
/// response from the runtime; `TransportClosed` is the stdio transport
/// dying; `RequestTimeout` is a request that never got a response;
/// `Config` is a local launch-configuration rejection; `Io` /
/// `RuntimeNotFound` are spawn/stdio launch failures; `Json` is
/// (de)serialization failure.
///
/// Diagnostics are static category text plus — for `RequestTimeout` only —
/// an allowlisted wire-method identifier. The raw SDK `Display`/`Debug`,
/// stderr tails, JSON-RPC `message`/`data`, and source errors are never
/// exported: they may embed credentials, filesystem paths, or raw wire
/// payloads (v1.188 P0 safe-diagnostics contract).
#[must_use]
pub fn classify_run_error(
    error: &Error,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> OperationFailedEvent {
    let (category, message) = classify_error_parts(error);
    OperationFailedEvent {
        session_id: session_id.clone(),
        op_id: op_id.clone(),
        error_category: category.to_string(),
        error_message: message,
    }
}

/// The `(category, safe message)` pair for one SDK error, shared by the
/// turn classifier and the provider's launch/probe error mapping (v1.188
/// P0 T2): one classification site keeps the launch-class and turn-class
/// diagnostics identical. Every SDK 0.2 variant is matched explicitly — a
/// newly added variant fails the build instead of silently degrading into
/// a generic bucket. The message is static category text plus — for
/// `RequestTimeout` only — an allowlisted wire-method identifier; raw SDK
/// payloads never leave this function.
#[must_use]
pub(crate) fn classify_error_parts(error: &Error) -> (&'static str, String) {
    match error {
        Error::SdkProtocol { .. } => (
            "decode_error",
            "dsh runtime violated the wire protocol".to_string(),
        ),
        Error::TransportClosed(_) => (
            "stream_closed",
            "dsh runtime closed the transport before the turn completed".to_string(),
        ),
        // The method is the wire method name; only the allowlisted
        // identifiers the SDK client can actually time out on may appear
        // in the diagnostic.
        Error::RequestTimeout { method, .. } => (
            "timeout",
            match method.as_str() {
                known @ ("initialize" | "session/prompt" | "shutdown") => {
                    format!("dsh request timed out: {known}")
                }
                _ => "dsh request timed out".to_string(),
            },
        ),
        // AR-7: io_error covers spawn/stdio failures; RuntimeNotFound is
        // the dsh analogue of claude's BinaryNotFound. Config is a local
        // launch-configuration rejection surfaced before spawn; JsonRpc /
        // Json are provider-side failures.
        Error::Io(_) | Error::RuntimeNotFound(_) => (
            "io_error",
            "dsh runtime could not be launched or its stdio failed".to_string(),
        ),
        Error::Config(_) => (
            "provider_error",
            "dsh launch configuration was rejected".to_string(),
        ),
        Error::JsonRpc { .. } | Error::Json(_) => (
            "provider_error",
            "dsh runtime returned an error response".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use uuid::Uuid;

    use super::*;
    use crate::ids::{HostOperationId, HostSessionId};

    fn ids() -> (HostSessionId, HostOperationId) {
        (
            HostSessionId(Uuid::new_v4()),
            HostOperationId(Uuid::new_v4()),
        )
    }

    fn run_result(finish_reason: Option<&str>) -> RunResult {
        RunResult {
            session_id: "session-test".to_string(),
            final_response: "hello from dsh".to_string(),
            finish_reason: finish_reason.map(str::to_string),
            events: Vec::new(),
            notifications: Vec::new(),
        }
    }

    fn terminal_count(events: &[HostEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, HostEvent::OpFinished(_) | HostEvent::OpFailed(_)))
            .count()
    }

    /// `finish_reason` `"completed"` → exactly one
    /// `MessageDelta(final_response)` + one terminal `OpFinished(EndTurn)`
    /// (AR-1 dsh table, AR-6; SDK 0.2 `turn/end` vocabulary).
    #[test]
    fn completed_reason_maps_to_single_delta_and_end_turn() {
        let (session_id, op_id) = ids();
        let result = run_result(Some("completed"));

        let events = map_run_result(&result, &session_id, &op_id);

        assert_eq!(events.len(), 2, "one delta + one terminal: {events:?}");
        assert!(
            matches!(
                &events[0],
                HostEvent::MessageDelta(d)
                    if d.text == "hello from dsh" && d.session_id == session_id && d.op_id == op_id
            ),
            "final_response must map to exactly one MessageDelta: {events:?}"
        );
        assert!(
            matches!(&events[1], HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn),
            "completed must end the turn cleanly: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// `finish_reason` `"max-tokens"` is NOT a success (v1.188 P0): the
    /// turn maps to exactly one terminal `OpFailed(max_tokens)` — never
    /// `OpFinished`.
    #[test]
    fn max_tokens_reason_is_a_failed_turn() {
        let (session_id, op_id) = ids();
        let result = run_result(Some("max-tokens"));

        let events = map_run_result(&result, &session_id, &op_id);

        assert_eq!(events.len(), 1, "one terminal, no delta: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::OpFailed(f) if f.error_category == "max_tokens"),
            "max-tokens must fail the turn: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// An absent `finish_reason` is NOT a success (v1.188 P0): exactly one
    /// terminal `OpFailed(provider_error)` — never `OpFinished(EndTurn)`.
    #[test]
    fn missing_finish_reason_is_a_failed_turn() {
        let (session_id, op_id) = ids();
        let result = run_result(None);

        let events = map_run_result(&result, &session_id, &op_id);

        assert_eq!(events.len(), 1, "one terminal, no delta: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::OpFailed(f) if f.error_category == "provider_error"),
            "a missing finish reason must fail the turn: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// An unknown or retired `finish_reason` token (e.g. the 0.1-era
    /// `"stop"`) is NOT a success (v1.188 P0): exactly one terminal
    /// `OpFailed(provider_error)` — never `OpFinished(EndTurn)`.
    #[test]
    fn unknown_finish_reason_is_a_failed_turn() {
        let (session_id, op_id) = ids();
        let result = run_result(Some("stop"));

        let events = map_run_result(&result, &session_id, &op_id);

        assert_eq!(events.len(), 1, "one terminal, no delta: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::OpFailed(f) if f.error_category == "provider_error"),
            "an unknown finish reason must fail the turn: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// AR-7 category tokens for the run-error classifier, exhaustive over
    /// the SDK 0.2 variants: `SdkProtocol` → `decode_error`,
    /// `TransportClosed` → `stream_closed`, `RequestTimeout` → `timeout`,
    /// `Io` / `RuntimeNotFound` → `io_error`, `Config` / `JsonRpc` /
    /// `Json` → `provider_error`.
    #[tokio::test]
    async fn classifies_run_errors_to_ar7_tokens() {
        let (session_id, op_id) = ids();
        let elapsed = tokio::time::timeout(std::time::Duration::ZERO, std::future::pending::<()>())
            .await
            .expect_err("a zero-duration timeout on pending must elapse immediately");

        let cases: Vec<(Error, &str)> = vec![
            (
                Error::SdkProtocol {
                    message: "malformed session.event".to_string(),
                },
                "decode_error",
            ),
            (
                Error::TransportClosed("exit status 1".to_string()),
                "stream_closed",
            ),
            (
                Error::RequestTimeout {
                    method: "session/prompt".to_string(),
                    source: elapsed,
                    profile: Some("sdk".to_string()),
                },
                "timeout",
            ),
            (Error::Io(std::io::Error::other("pipe")), "io_error"),
            (Error::RuntimeNotFound("no runtime".to_string()), "io_error"),
            (
                Error::Config("profile must not be empty".to_string()),
                "provider_error",
            ),
            (
                Error::JsonRpc {
                    code: Some(-32000),
                    message: "server error".to_string(),
                    data: None,
                },
                "provider_error",
            ),
            (
                Error::Json(serde_json::from_str::<Value>("{").expect_err("malformed json")),
                "provider_error",
            ),
        ];

        for (error, expected) in cases {
            let failed = classify_run_error(&error, &session_id, &op_id);
            assert_eq!(failed.error_category, expected, "for error {error}");
        }
    }

    /// Safe diagnostics (v1.188 P0): `error_message` is static category
    /// text — it must never echo the raw SDK payloads (protocol violation
    /// detail, stderr tail, JSON-RPC message/data, io detail), which may
    /// embed credentials, paths, or wire content.
    #[tokio::test]
    async fn run_error_diagnostics_never_echo_raw_sdk_payloads() {
        let (session_id, op_id) = ids();
        let elapsed = tokio::time::timeout(std::time::Duration::ZERO, std::future::pending::<()>())
            .await
            .expect_err("a zero-duration timeout on pending must elapse immediately");
        let secret = "sk-live-credential-9f27";

        let cases: Vec<Error> = vec![
            Error::SdkProtocol {
                message: format!("malformed session.event: {secret}"),
            },
            Error::TransportClosed(format!("exit status 1, stderr tail: {secret}")),
            Error::RequestTimeout {
                method: "session/prompt".to_string(),
                source: elapsed,
                profile: Some(secret.to_string()),
            },
            Error::Io(std::io::Error::other(format!("spawn failed: {secret}"))),
            Error::RuntimeNotFound(format!("hint: {secret}")),
            Error::Config(format!("invalid: {secret}")),
            Error::JsonRpc {
                code: Some(-32000),
                message: format!("server error: {secret}"),
                data: Some(serde_json::json!({ "detail": secret })),
            },
        ];

        for error in cases {
            let failed = classify_run_error(&error, &session_id, &op_id);
            assert!(
                !failed.error_message.contains(secret),
                "error_message must not echo raw SDK payloads: {failed:?}"
            );
            assert!(
                !failed.error_message.is_empty(),
                "error_message carries static diagnostic text: {failed:?}"
            );
        }
    }

    /// Safe diagnostics (v1.188 P0): a `RequestTimeout` names the
    /// allowlisted wire-method identifier; an unexpected method string
    /// collapses to static text instead of being echoed.
    #[tokio::test]
    async fn request_timeout_names_only_allowlisted_methods() {
        let (session_id, op_id) = ids();
        let elapsed = || async {
            tokio::time::timeout(std::time::Duration::ZERO, std::future::pending::<()>())
                .await
                .expect_err("zero-duration timeout elapses")
        };

        let allowlisted = classify_run_error(
            &Error::RequestTimeout {
                method: "initialize".to_string(),
                source: elapsed().await,
                profile: Some("sdk".to_string()),
            },
            &session_id,
            &op_id,
        );
        assert_eq!(allowlisted.error_category, "timeout");
        assert!(
            allowlisted.error_message.contains("initialize"),
            "an allowlisted method identifier may be named: {allowlisted:?}"
        );

        let unexpected = classify_run_error(
            &Error::RequestTimeout {
                method: "session/prompt; rm -rf /".to_string(),
                source: elapsed().await,
                profile: None,
            },
            &session_id,
            &op_id,
        );
        assert_eq!(unexpected.error_category, "timeout");
        assert!(
            !unexpected.error_message.contains("rm -rf"),
            "an unexpected method string must not be echoed: {unexpected:?}"
        );
    }
}
