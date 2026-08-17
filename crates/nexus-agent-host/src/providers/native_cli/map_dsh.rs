//! `HostEvent` mapping for the `deepseek_harness_sdk` high-level surface
//! (locks § AR-1 dsh table, AR-6, AR-7).
//!
//! Converts one `Session::run` outcome into `HostEvent`s. A successful
//! `RunResult` becomes exactly one `MessageDelta(final_response)` plus one
//! terminal `OpFinished` (AR-6: no incremental deltas on this surface); a
//! run error becomes exactly one `OpFailed` with the AR-7 category tokens.
//! The crate owns the runtime spawn, the stdio JSON-RPC wire parser, and
//! the inbox-receipt / root-idle algorithm (`Session::run`); Nexus owns
//! only the event normalization. Consumed by the dsh provider execute
//! loop (v1.168 P2 T1).

use deepseek_harness_sdk::{Error, RunResult};

use crate::capability::model::{
    FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, TextDeltaEvent,
};
use crate::ids::{HostOperationId, HostSessionId};

/// Map one successful dsh turn into host events (AR-1 dsh table, AR-6).
///
/// Exactly one `MessageDelta` carrying `final_response` (which may be
/// empty — the SDK derives it from the last `assistant/message` event and
/// never falls back to an earlier one), followed by exactly one terminal
/// `OpFinished`: `finish_reason` `"stop"` → `EndTurn`, `"length"` →
/// `MaxTokens`, absent or any other token → `EndTurn`.
#[must_use]
pub fn map_run_result(
    result: &RunResult,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Vec<HostEvent> {
    let reason = match result.finish_reason.as_deref() {
        Some("length") => FinishReason::MaxTokens,
        // "stop", absent, or an unknown token → EndTurn (AR-1 table).
        _ => FinishReason::EndTurn,
    };
    vec![
        HostEvent::MessageDelta(TextDeltaEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            text: result.final_response.clone(),
        }),
        HostEvent::OpFinished(OperationFinishedEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            reason,
        }),
    ]
}

/// Classify a `deepseek_harness_sdk` start/run error into the one terminal
/// `OpFailed` event of the failed turn (PD-3, AR-7 category tokens).
///
/// The SDK surfaces typed-decode and protocol violations as
/// `Error::SdkProtocol` (malformed `session.event` / `session.status`
/// payloads during `Session::run`, `turn/end` reason extraction failures,
/// missing server identity / message id) — the dsh decode-error surface.
/// `JsonRpc` is a JSON-RPC error response from the runtime (provider-side
/// failure); `TransportClosed` is the stdio transport dying — the stream
/// abort; `RequestTimeout` is a request that never got a response;
/// `Io` / `RuntimeNotFound` are spawn/stdio launch failures.
#[must_use]
pub fn classify_run_error(
    error: &Error,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> OperationFailedEvent {
    let category = match error {
        Error::SdkProtocol { .. } => "decode_error",
        Error::TransportClosed(_) => "stream_closed",
        Error::RequestTimeout { .. } => "timeout",
        // AR-7: io_error covers spawn/stdio failures; RuntimeNotFound is
        // the dsh analogue of claude's BinaryNotFound.
        Error::Io(_) | Error::RuntimeNotFound(_) => "io_error",
        // JsonRpc error responses and the outgoing-serialization error
        // (Json) are provider-side failures; the catch-all keeps future
        // variants provider_error.
        _ => "provider_error",
    };
    OperationFailedEvent {
        session_id: session_id.clone(),
        op_id: op_id.clone(),
        error_category: category.to_string(),
        // AR-7: the crate error's Display — capped at 512 bytes (P1 N-2
        // parity; SdkProtocol messages can embed raw payloads).
        error_message: crate::providers::native_cli::truncate_error_message(&error.to_string()),
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
            session_root: None,
        }
    }

    fn terminal_count(events: &[HostEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, HostEvent::OpFinished(_) | HostEvent::OpFailed(_)))
            .count()
    }

    /// `finish_reason` `"stop"` → exactly one `MessageDelta(final_response)`
    /// + one terminal `OpFinished(EndTurn)` (AR-1 dsh table, AR-6).
    #[test]
    fn stop_reason_maps_to_single_delta_and_end_turn() {
        let (session_id, op_id) = ids();
        let result = run_result(Some("stop"));

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
            "stop must end the turn cleanly: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// `finish_reason` `"length"` → one `MessageDelta` + one terminal
    /// `OpFinished(MaxTokens)`.
    #[test]
    fn length_reason_maps_to_max_tokens() {
        let (session_id, op_id) = ids();
        let result = run_result(Some("length"));

        let events = map_run_result(&result, &session_id, &op_id);

        assert_eq!(events.len(), 2, "one delta + one terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::MessageDelta(_)),
            "length still carries the final response: {events:?}"
        );
        assert!(
            matches!(&events[1], HostEvent::OpFinished(f) if f.reason == FinishReason::MaxTokens),
            "length must map to MaxTokens: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// Absent `finish_reason` → `OpFinished(EndTurn)` (AR-1 table).
    #[test]
    fn absent_finish_reason_maps_to_end_turn() {
        let (session_id, op_id) = ids();
        let result = run_result(None);

        let events = map_run_result(&result, &session_id, &op_id);

        assert!(matches!(
            &events[1],
            HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn
        ));
        assert_eq!(terminal_count(&events), 1);
    }

    /// An unknown `finish_reason` token → `OpFinished(EndTurn)` — never a
    /// failure (AR-1 table: absent/other → `EndTurn`).
    #[test]
    fn unknown_finish_reason_maps_to_end_turn() {
        let (session_id, op_id) = ids();
        let result = run_result(Some("tool_calls"));

        let events = map_run_result(&result, &session_id, &op_id);

        assert!(matches!(
            &events[1],
            HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn
        ));
        assert_eq!(terminal_count(&events), 1);
    }

    /// AR-6: exactly one `MessageDelta` + one terminal even when
    /// `final_response` is empty (the SDK never falls back to an earlier
    /// event; an empty turn still ends cleanly).
    #[test]
    fn empty_final_response_still_emits_delta_and_terminal() {
        let (session_id, op_id) = ids();
        let mut result = run_result(Some("stop"));
        result.final_response = String::new();

        let events = map_run_result(&result, &session_id, &op_id);

        assert_eq!(events.len(), 2, "one delta + one terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::MessageDelta(d) if d.text.is_empty()),
            "the delta is emitted with the empty final response: {events:?}"
        );
        assert!(matches!(&events[1], HostEvent::OpFinished(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// AR-7 category tokens for the run-error classifier: `SdkProtocol` →
    /// `decode_error`, `TransportClosed` → `stream_closed`,
    /// `RequestTimeout` → `timeout`, `Io` / `RuntimeNotFound` →
    /// `io_error`, `JsonRpc` / `Json` → `provider_error`.
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
                },
                "timeout",
            ),
            (Error::Io(std::io::Error::other("pipe")), "io_error"),
            (Error::RuntimeNotFound("no runtime".to_string()), "io_error"),
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
            // N-2: error_message is the crate Display, capped at 512 bytes.
            let full = error.to_string();
            let prefix = failed
                .error_message
                .strip_suffix("... (truncated)")
                .unwrap_or(&failed.error_message);
            assert!(
                full.starts_with(prefix),
                "error_message must be the (truncated) Display prefix for \
                 {error}: {failed:?}"
            );
        }
    }

    /// N-2 (P1 parity): an `SdkProtocol` message embedding a large raw
    /// payload must be capped in `error_message`, not echoed unboundedly.
    #[test]
    fn decode_error_message_truncates_raw_wire() {
        let (session_id, op_id) = ids();
        let huge_payload = "x".repeat(4096);
        let err = Error::SdkProtocol {
            message: format!("malformed session.event: {huge_payload}"),
        };

        let failed = classify_run_error(&err, &session_id, &op_id);

        assert_eq!(failed.error_category, "decode_error");
        assert!(failed.error_message.ends_with("... (truncated)"));
        assert!(
            failed.error_message.len() < 1024,
            "error_message must be bounded: {} bytes",
            failed.error_message.len()
        );
        assert_eq!(terminal_count(&[HostEvent::OpFailed(failed)]), 1);
    }
}
