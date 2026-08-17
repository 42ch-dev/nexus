//! `HostEvent` mapping for `codex_codes` app-server JSON-RPC frames (AR-1
//! table).
//!
//! Converts a turn's `codex_codes::ServerMessage` frames into `HostEvent`s,
//! keeping the wire parser owned by the crate and Nexus owning only the
//! event normalization. Consumed by the Codex provider execute loop
//! (v1.168 P1 T2).

use codex_codes::messages::Notification;
use codex_codes::protocol::{Turn, TurnStatus};
use codex_codes::Error as CodexError;
use codex_codes::ServerMessage;

use crate::capability::model::{
    FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    StatusEvent, StatusLevel, TextDeltaEvent,
};
use crate::ids::{HostOperationId, HostSessionId};

/// Map a batch of codex app-server frames into host events.
///
/// One turn = one batch, ending at `TurnCompleted`. Emits at most one
/// terminal event; see the map table in
/// `.mstar/iterations/v1.168/specs/v1.168-native-host-locks.md` (AR-1).
///
/// Decode contract (PD-3): unknown methods arrive as
/// `Notification::Unknown` (crate passthrough) — skipped with a debug line;
/// a typed-decode failure on a known method never reaches this function —
/// it errors in the crate (see [`classify_stream_error`]). Approval
/// requests produce no host event here (AR-4: the execute loop
/// auto-responds from the native permission policy).
pub fn map_codex(
    messages: &[ServerMessage],
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Vec<HostEvent> {
    let mut events = Vec::new();

    for message in messages {
        match message {
            ServerMessage::Request { .. } => {
                // AR-4: approvals are answered by the execute loop, never
                // surfaced to the author.
                tracing::debug!(
                    target: "nexus_agent_host::providers::native_cli::map_codex",
                    "skipping codex server request (auto-responded by execute loop)",
                );
            }
            ServerMessage::Notification(notification) => match notification {
                Notification::TurnStarted(_) => {
                    events.push(HostEvent::OpStarted(OperationStartedEvent {
                        session_id: session_id.clone(),
                        op_id: op_id.clone(),
                    }));
                }
                // AR-1: incremental deltas.
                Notification::AgentMessageDelta(delta) => {
                    events.push(HostEvent::MessageDelta(TextDeltaEvent {
                        session_id: session_id.clone(),
                        op_id: op_id.clone(),
                        text: delta.delta.clone(),
                    }));
                }
                Notification::ReasoningDelta(delta) => {
                    events.push(HostEvent::ThoughtDelta(TextDeltaEvent {
                        session_id: session_id.clone(),
                        op_id: op_id.clone(),
                        text: delta.delta.clone(),
                    }));
                }
                Notification::ReasoningTextDelta(delta) => {
                    events.push(HostEvent::ThoughtDelta(TextDeltaEvent {
                        session_id: session_id.clone(),
                        op_id: op_id.clone(),
                        text: delta.delta.clone(),
                    }));
                }
                Notification::TurnCompleted(completed) => {
                    if let Some(terminal) =
                        turn_completed_terminal(&completed.turn, session_id, op_id)
                    {
                        events.push(terminal);
                        break;
                    }
                }
                // AR-1: non-terminal; the terminal comes from `TurnCompleted`
                // or the stream-abort backstop.
                Notification::Error(err) => events.push(HostEvent::Status(StatusEvent {
                    session_id: Some(session_id.clone()),
                    level: StatusLevel::Error,
                    message: err.error.message.clone(),
                })),
                Notification::Warning(warning) => events.push(HostEvent::Status(StatusEvent {
                    session_id: Some(session_id.clone()),
                    level: StatusLevel::Warning,
                    message: warning.message.clone(),
                })),
                Notification::GuardianWarning(warning) => {
                    events.push(HostEvent::Status(StatusEvent {
                        session_id: Some(session_id.clone()),
                        level: StatusLevel::Warning,
                        message: warning.message.clone(),
                    }));
                }
                Notification::ConfigWarning(warning) => {
                    events.push(HostEvent::Status(StatusEvent {
                        session_id: Some(session_id.clone()),
                        level: StatusLevel::Warning,
                        message: warning.summary.clone(),
                    }));
                }
                Notification::DeprecationNotice(notice) => {
                    events.push(HostEvent::Status(StatusEvent {
                        session_id: Some(session_id.clone()),
                        level: StatusLevel::Warning,
                        message: notice.summary.clone(),
                    }));
                }
                // AR-1: ItemStarted/ItemCompleted/CmdOutputDelta/
                // FileChangeOutputDelta/PlanDelta/TurnPlanUpdated/token-usage/
                // thread-lifecycle/realtime/every other modeled method — no
                // host event this iteration (structured_tool_calls stays
                // false). PD-3 row 1: unknown methods are crate passthrough.
                _ => tracing::debug!(
                    target: "nexus_agent_host::providers::native_cli::map_codex",
                    method = notification.method(),
                    "skipping codex notification",
                ),
            },
        }
    }

    events
}

/// Terminal event for a `TurnCompleted` frame, if it carries one (AR-1).
///
/// `Completed` / `Interrupted` → clean `OpFinished(EndTurn)` (an
/// interrupt-initiated stop is a clean end, not a failure); `Failed` (with
/// or without a payload) or an error-carrying `inProgress` frame → one
/// `OpFailed(provider_error)`. An unmodeled `inProgress` status is noise —
/// returns `None` so the loop keeps mapping and the stream-abort backstop
/// still supplies the one terminal.
fn turn_completed_terminal(
    turn: &Turn,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Option<HostEvent> {
    match &turn.status {
        TurnStatus::Completed | TurnStatus::Interrupted => {
            Some(HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                reason: FinishReason::EndTurn,
            }))
        }
        TurnStatus::InProgress if turn.error.is_none() => {
            tracing::debug!(
                target: "nexus_agent_host::providers::native_cli::map_codex",
                "skipping turn/completed with inProgress status",
            );
            None
        }
        TurnStatus::Failed | TurnStatus::InProgress => {
            Some(HostEvent::OpFailed(OperationFailedEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                error_category: "provider_error".to_string(),
                error_message: turn
                    .error
                    .as_ref()
                    .map_or_else(|| format!("turn {} failed", turn.id), |e| e.message.clone()),
            }))
        }
    }
}

/// Classify a `codex_codes` stream error into the one terminal `OpFailed`
/// event of the failed turn (PD-3, AR-7 category tokens).
///
/// Observed crate API (codex-codes 0.146.4, locked by the T1 fixture
/// `codex_bad_delta_params`): a known method whose `params` fail typed
/// decode surfaces as `Err(Error::Deserialization(ParseError))` from
/// `AsyncClient::next_message` — the frame is consumed (lost), so the turn
/// is failed once with `decode_error` (PD-3 row 2: no per-item skip).
/// `Ok(None)` at EOF (or `Error::ServerClosed` / `Error::ConnectionClosed`)
/// is a stream abort before `TurnCompleted`.
#[must_use]
pub fn classify_stream_error(
    error: &CodexError,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Option<OperationFailedEvent> {
    let category = match error {
        CodexError::Deserialization(_) => "decode_error",
        CodexError::ConnectionClosed | CodexError::ServerClosed => "stream_closed",
        // AR-7: io_error covers spawn/stdio failures.
        CodexError::Io(_) | CodexError::BinaryNotFound { .. } => "io_error",
        // Json originates on the outgoing serialization path (incoming
        // decode failures surface as Deserialization); everything else is a
        // provider-side failure (server exit, JSON-RPC error response, …).
        _ => "provider_error",
    };
    Some(OperationFailedEvent {
        session_id: session_id.clone(),
        op_id: op_id.clone(),
        error_category: category.to_string(),
        // AR-7: the crate error's Display, verbatim.
        error_message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::model::{FinishReason, StatusLevel};
    use uuid::Uuid;

    const TURN_STARTED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_turn_started.json"
    ));
    const AGENT_MESSAGE_DELTA: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_agent_message_delta.json"
    ));
    const REASONING_TEXT_DELTA: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_reasoning_text_delta.json"
    ));
    const REASONING_SUMMARY_DELTA: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_reasoning_summary_delta.json"
    ));
    const TURN_COMPLETED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_turn_completed.json"
    ));
    const TURN_INTERRUPTED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_turn_interrupted.json"
    ));
    const TURN_FAILED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_turn_failed.json"
    ));
    const UNKNOWN_METHOD: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_unknown_method.json"
    ));
    const WARNING: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_warning.json"
    ));
    const ERROR_NOTIFICATION: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_error_notification.json"
    ));
    const BAD_DELTA_PARAMS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_bad_delta_params.json"
    ));
    const CMD_OUTPUT_DELTA: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/codex_cmd_output_delta.json"
    ));

    fn parse(json: &str) -> ServerMessage {
        ServerMessage::from_json_str(json).expect("fixture must parse as ServerMessage")
    }

    fn ids() -> (HostSessionId, HostOperationId) {
        (
            HostSessionId(Uuid::new_v4()),
            HostOperationId(Uuid::new_v4()),
        )
    }

    fn terminal_count(events: &[HostEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, HostEvent::OpFinished(_) | HostEvent::OpFailed(_)))
            .count()
    }

    /// (a) known deltas + `TurnCompleted(Completed)` → `OpStarted`,
    /// `MessageDelta`s (incremental), `ThoughtDelta`s, exactly one terminal
    /// `OpFinished(EndTurn)`.
    #[test]
    fn maps_deltas_with_completed_terminal() {
        let (session_id, op_id) = ids();
        let messages = [
            parse(TURN_STARTED),
            parse(AGENT_MESSAGE_DELTA),
            parse(REASONING_TEXT_DELTA),
            parse(REASONING_SUMMARY_DELTA),
            parse(TURN_COMPLETED),
        ];

        let events = map_codex(&messages, &session_id, &op_id);

        assert_eq!(events.len(), 5, "started + 3 deltas + terminal: {events:?}");
        assert!(matches!(&events[0], HostEvent::OpStarted(_)));
        assert!(
            matches!(&events[1], HostEvent::MessageDelta(d) if d.text == "Hello "),
            "agentMessage delta must map to MessageDelta: {events:?}"
        );
        assert!(
            matches!(&events[2], HostEvent::ThoughtDelta(d) if d.text == "hmm, let me think"),
            "reasoning textDelta must map to ThoughtDelta: {events:?}"
        );
        assert!(
            matches!(&events[3], HostEvent::ThoughtDelta(d) if d.text == "reasoning summary"),
            "reasoning summaryTextDelta must map to ThoughtDelta: {events:?}"
        );
        assert!(
            matches!(&events[4], HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn),
            "completed turn must end cleanly: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// `TurnCompleted(Interrupted)` → clean `OpFinished(EndTurn)` (an
    /// interrupt-initiated stop is a clean end, AR-1).
    #[test]
    fn interrupted_turn_ends_cleanly() {
        let (session_id, op_id) = ids();
        let messages = [parse(TURN_INTERRUPTED)];

        let events = map_codex(&messages, &session_id, &op_id);

        assert_eq!(events.len(), 1, "only the terminal: {events:?}");
        assert!(matches!(
            &events[0],
            HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn
        ));
        assert_eq!(terminal_count(&events), 1);
    }

    /// `TurnCompleted(Failed)` or `turn.error` present → one
    /// `OpFailed(provider_error)` with the server's message (AR-1, AR-7).
    #[test]
    fn failed_turn_emits_provider_error() {
        let (session_id, op_id) = ids();
        let messages = [parse(TURN_FAILED)];

        let events = map_codex(&messages, &session_id, &op_id);

        assert_eq!(events.len(), 1, "only the terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::OpFailed(f) if f.error_category == "provider_error" && f.error_message.contains("rate limit exceeded")),
            "failed turn must carry the server error: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// (b) unknown method → `Notification::Unknown` (crate passthrough,
    /// PD-3 row 1) → skip, still exactly one terminal.
    #[test]
    fn skips_unknown_method_notification_but_keeps_terminal() {
        let (session_id, op_id) = ids();
        let messages = [
            parse(TURN_STARTED),
            parse(UNKNOWN_METHOD),
            parse(AGENT_MESSAGE_DELTA),
            parse(TURN_COMPLETED),
        ];

        let events = map_codex(&messages, &session_id, &op_id);

        assert_eq!(events.len(), 3, "started + delta + terminal: {events:?}");
        assert!(matches!(&events[0], HostEvent::OpStarted(_)));
        assert!(
            matches!(&events[1], HostEvent::MessageDelta(d) if d.text == "Hello "),
            "known delta still maps around the unknown method: {events:?}"
        );
        assert!(matches!(&events[2], HostEvent::OpFinished(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// Known but unmapped methods (e.g. command output deltas) → skip
    /// (`structured_tool_calls` stays false; AR-1 event vocabulary).
    #[test]
    fn skips_known_unmapped_methods() {
        let (session_id, op_id) = ids();
        let messages = [
            parse(TURN_STARTED),
            parse(CMD_OUTPUT_DELTA),
            parse(TURN_COMPLETED),
        ];

        let events = map_codex(&messages, &session_id, &op_id);

        assert_eq!(events.len(), 2, "started + terminal: {events:?}");
        assert!(matches!(&events[0], HostEvent::OpStarted(_)));
        assert!(matches!(&events[1], HostEvent::OpFinished(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// `Warning` → non-terminal `Status(warning)` (AR-1).
    #[test]
    fn warning_notification_is_status_warning() {
        let (session_id, op_id) = ids();
        let messages = [parse(WARNING), parse(TURN_COMPLETED)];

        let events = map_codex(&messages, &session_id, &op_id);

        assert_eq!(events.len(), 2, "warning + terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::Status(s) if s.level == StatusLevel::Warning && s.message.contains("CLI version newer than tested")),
            "warning must surface as Status(warning): {events:?}"
        );
        assert!(matches!(&events[1], HostEvent::OpFinished(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// `Error(ErrorNotification)` → non-terminal `Status(error)`; the
    /// terminal comes from `TurnCompleted` / stream abort (AR-1).
    #[test]
    fn error_notification_is_non_terminal_status_error() {
        let (session_id, op_id) = ids();
        let messages = [parse(ERROR_NOTIFICATION), parse(TURN_FAILED)];

        let events = map_codex(&messages, &session_id, &op_id);

        assert_eq!(events.len(), 2, "status + terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::Status(s) if s.level == StatusLevel::Error && s.message.contains("provider exploded")),
            "error notification must surface as Status(error): {events:?}"
        );
        assert!(matches!(&events[1], HostEvent::OpFailed(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// (c) typed-decode failure on a known method → the crate's own parse
    /// path (`ServerMessage::from_json_str`, the same one `next_message`
    /// runs) returns `Err(Error::Deserialization(ParseError))`, which
    /// classifies to one `OpFailed(decode_error)` — never a per-item skip.
    #[test]
    fn typed_decode_failure_is_op_failed_decode_error() {
        let (session_id, op_id) = ids();

        let err = ServerMessage::from_json_str(BAD_DELTA_PARAMS)
            .expect_err("malformed params on a known method must fail typed decode");

        let failed = classify_stream_error(&err, &session_id, &op_id)
            .expect("decode failure must classify to a terminal event");
        assert_eq!(failed.error_category, "decode_error");
        assert_eq!(failed.error_message, err.to_string());
        assert_eq!(terminal_count(&[HostEvent::OpFailed(failed)]), 1);
    }

    /// AR-7 category tokens for the stream-error classifier, including
    /// `stream_closed` on EOF (`Ok(None)` → `Error::ServerClosed` /
    /// `ConnectionClosed`).
    #[test]
    fn classifies_stream_errors_to_ar7_tokens() {
        let (session_id, op_id) = ids();

        let cases: Vec<(CodexError, &str)> = vec![
            (CodexError::ConnectionClosed, "stream_closed"),
            (CodexError::ServerClosed, "stream_closed"),
            (CodexError::Io(std::io::Error::other("pipe")), "io_error"),
            (
                CodexError::BinaryNotFound {
                    name: "codex".to_string(),
                },
                "io_error",
            ),
            (CodexError::Protocol("boom".to_string()), "provider_error"),
            (
                CodexError::JsonRpc {
                    code: -32000,
                    message: "server error".to_string(),
                },
                "provider_error",
            ),
            (
                CodexError::ProcessFailed(1, "exit".to_string()),
                "provider_error",
            ),
            (CodexError::Unknown("??".to_string()), "provider_error"),
        ];

        for (error, expected) in cases {
            let failed = classify_stream_error(&error, &session_id, &op_id)
                .unwrap_or_else(|| panic!("{expected} must classify: {error}"));
            assert_eq!(failed.error_category, expected, "for error {error}");
            assert_eq!(failed.error_message, error.to_string());
        }
    }
}
