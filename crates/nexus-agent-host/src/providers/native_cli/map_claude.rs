//! `HostEvent` mapping for `claude_codes` stream-json frames (AR-1 table).
//!
//! Converts a turn's `claude_codes::ClaudeOutput` frames into `HostEvent`s,
//! keeping the wire parser owned by the crate and Nexus owning only the
//! event normalization. Consumed by the Claude provider execute loop
//! (v1.168 P1 T3).

use claude_codes::io::{ContentBlock, ResultMessage, ResultSubtype};
use claude_codes::ClaudeOutput;
use claude_codes::Error as ClaudeError;

use crate::capability::model::{
    FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, StatusEvent,
    StatusLevel, TextDeltaEvent,
};
use crate::ids::{HostOperationId, HostSessionId};

/// Map a batch of Claude stream-json frames into host events.
///
/// One turn = one batch, ending at the CLI's `Result` frame. Emits at most
/// one terminal event; see the map table in
/// `.mstar/iterations/v1.168/specs/v1.168-native-host-locks.md` (AR-1).
///
/// Decode contract (PD-3 + AR-1 correction): unknown **nested** variants the
/// crate downgrades (`ContentBlock::Unknown`, `ResultSubtype::Unknown`, …)
/// are skipped with a debug line; an unknown **top-level** `type` tag never
/// reaches this function — it fails typed decode in the crate (see
/// [`classify_stream_error`]). Frames that produce no host event are
/// skipped with a debug line, never fatal.
pub fn map_claude(
    outputs: &[ClaudeOutput],
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Vec<HostEvent> {
    let mut events = Vec::new();

    for output in outputs {
        match output {
            ClaudeOutput::Assistant(assistant) => {
                for block in &assistant.message.content {
                    match block {
                        // AR-1: whole block, no token deltas this iteration.
                        ContentBlock::Text(text) => {
                            events.push(HostEvent::MessageDelta(TextDeltaEvent {
                                session_id: session_id.clone(),
                                op_id: op_id.clone(),
                                text: text.text.clone(),
                            }));
                        }
                        ContentBlock::Thinking(thinking) => {
                            events.push(HostEvent::ThoughtDelta(TextDeltaEvent {
                                session_id: session_id.clone(),
                                op_id: op_id.clone(),
                                text: thinking.thinking.clone(),
                            }));
                        }
                        // AR-1: ToolUse and any other/unknown block carry no
                        // host event this iteration (structured_tool_calls
                        // stays false; unknown blocks are forward-compat
                        // noise) — skip + debug.
                        other => tracing::debug!(
                            target: "nexus_agent_host::providers::native_cli::map_claude",
                            block_type = other.block_type(),
                            "skipping claude content block",
                        ),
                    }
                }
            }
            ClaudeOutput::Result(result) => {
                let success = matches!(result.subtype, ResultSubtype::Success) && !result.is_error;
                if success {
                    events.push(HostEvent::OpFinished(OperationFinishedEvent {
                        session_id: session_id.clone(),
                        op_id: op_id.clone(),
                        reason: FinishReason::EndTurn,
                    }));
                    break;
                }
                if matches!(result.subtype, ResultSubtype::Unknown(_)) {
                    // AR-1 correction: a crate-downgraded unknown subtype is
                    // nested unknown noise — skip, do not fail the turn here.
                    // A later `Result` frame (or the stream-abort backstop)
                    // still supplies the one terminal.
                    tracing::debug!(
                        target: "nexus_agent_host::providers::native_cli::map_claude",
                        subtype = result.subtype.as_str(),
                        "skipping claude result frame with unknown subtype",
                    );
                    continue;
                }
                // Known error subtype (error_max_turns, error_during_execution,
                // …) or success-subtype-with-error-flag → one terminal failure.
                events.push(HostEvent::OpFailed(OperationFailedEvent {
                    session_id: session_id.clone(),
                    op_id: op_id.clone(),
                    error_category: "provider_error".to_string(),
                    error_message: result_error_message(result),
                }));
                break;
            }
            // AR-1: non-terminal warning; the terminal decision stays with
            // the `Result` frame / stream abort.
            ClaudeOutput::Error(err) => events.push(HostEvent::Status(StatusEvent {
                session_id: Some(session_id.clone()),
                level: StatusLevel::Warning,
                message: format!(
                    "claude api error: {}: {}",
                    err.error.error_type, err.error.message
                ),
            })),
            ClaudeOutput::RateLimitEvent(evt) => events.push(HostEvent::Status(StatusEvent {
                session_id: Some(session_id.clone()),
                level: StatusLevel::Warning,
                message: format!("claude rate limit: {}", evt.rate_limit_info.status),
            })),
            // AR-1: System / User / TranscriptResult / ControlRequest /
            // ControlResponse / StreamEvent (cannot arrive without
            // --include-partial-messages) / ToolProgress / CommandLifecycle /
            // AuthStatus / ToolUseSummary / PromptSuggestion /
            // ConversationReset / internal transcript events — skipped, no
            // author-visible content on this surface.
            _ => tracing::debug!(
                target: "nexus_agent_host::providers::native_cli::map_claude",
                frame = output.message_type(),
                "skipping claude frame",
            ),
        }
    }

    events
}

/// Error text for a failed claude `Result` frame: the CLI's `errors` list,
/// else the `result` string, else the subtype token.
fn result_error_message(result: &ResultMessage) -> String {
    if !result.errors.is_empty() {
        result.errors.join("\n")
    } else if let Some(result_text) = &result.result {
        result_text.clone()
    } else {
        result.subtype.as_str().to_string()
    }
}

/// Classify a `claude_codes` stream error into the one terminal `OpFailed`
/// event of the failed turn (PD-3, AR-7 category tokens).
///
/// Observed crate API (claude-codes 2.1.232, locked by the T1 fixture
/// `unknown_top_level_type`): `AsyncClient::receive` returns
/// `Err(Error::Deserialization(ParseError))` for a frame that fails typed
/// decode, with the connection left usable — the error is per-frame, not
/// fatal to the stream. The frame content is consumed (lost), so the turn is
/// failed once with `decode_error` (PD-3 row 2: no per-item skip).
/// `Err(Error::ConnectionClosed)` is EOF — a stream abort before `Result`.
#[must_use]
pub fn classify_stream_error(
    error: &ClaudeError,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Option<OperationFailedEvent> {
    let category = match error {
        ClaudeError::Deserialization(_) => "decode_error",
        ClaudeError::ConnectionClosed => "stream_closed",
        ClaudeError::Timeout => "timeout",
        // AR-7: io_error covers spawn/stdio failures.
        ClaudeError::Io(_) | ClaudeError::BinaryNotFound { .. } => "io_error",
        // Json originates on the outgoing serialization path (incoming
        // decode failures surface as Deserialization); everything else is a
        // provider-side failure.
        _ => "provider_error",
    };
    Some(OperationFailedEvent {
        session_id: session_id.clone(),
        op_id: op_id.clone(),
        error_category: category.to_string(),
        // AR-7: the crate error's Display — capped so the raw wire line the
        // Deserialization Display embeds is not echoed unboundedly (N-2).
        error_message: crate::providers::native_cli::truncate_error_message(&error.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::model::{FinishReason, StatusLevel};
    use claude_codes::io::ResultSubtype;
    use uuid::Uuid;

    const ASSISTANT_TEXT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_assistant_text.json"
    ));
    const ASSISTANT_UNKNOWN_BLOCK: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_assistant_unknown_block.json"
    ));
    const RESULT_SUCCESS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_result_success.json"
    ));
    const RESULT_UNKNOWN_SUBTYPE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_result_unknown_subtype.json"
    ));
    const RESULT_ERROR: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_result_error.json"
    ));
    const UNKNOWN_TOP_LEVEL_TYPE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_unknown_top_level_type.json"
    ));
    const ERROR_FRAME: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_error_frame.json"
    ));
    const RATE_LIMIT_EVENT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/claude_rate_limit_event.json"
    ));

    fn parse(json: &str) -> ClaudeOutput {
        ClaudeOutput::parse_json_tolerant(json).expect("fixture must parse as ClaudeOutput")
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

    /// (a) known assistant text (plus thinking) → deltas + exactly one
    /// terminal `OpFinished(EndTurn)`.
    #[test]
    fn maps_assistant_text_and_thinking_with_success_terminal() {
        let (session_id, op_id) = ids();
        let frames = [parse(ASSISTANT_TEXT), parse(RESULT_SUCCESS)];

        let events = map_claude(&frames, &session_id, &op_id);

        assert_eq!(events.len(), 3, "two deltas + one terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::MessageDelta(d) if d.text == "Hello from claude"),
            "text block must map to MessageDelta: {events:?}"
        );
        assert!(
            matches!(&events[1], HostEvent::ThoughtDelta(d) if d.text == "let me think about this"),
            "thinking block must map to ThoughtDelta: {events:?}"
        );
        assert!(
            matches!(&events[2], HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn),
            "success result must end the turn: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// (b) unknown nested content block (`ContentBlock::Unknown`) → skip,
    /// still exactly one terminal.
    #[test]
    fn skips_unknown_content_block_but_keeps_terminal() {
        let (session_id, op_id) = ids();
        let frames = [parse(ASSISTANT_UNKNOWN_BLOCK), parse(RESULT_SUCCESS)];

        let events = map_claude(&frames, &session_id, &op_id);

        assert_eq!(events.len(), 2, "text delta + terminal only: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::MessageDelta(d) if d.text == "visible text"),
            "known text block must still map: {events:?}"
        );
        assert!(matches!(&events[1], HostEvent::OpFinished(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// (b) `ResultSubtype::Unknown` (crate-downgraded nested unknown, AR-1
    /// correction) → skip without a terminal of its own; a later `Result`
    /// frame still supplies the one terminal.
    #[test]
    fn skips_unknown_result_subtype_without_emitting_terminal() {
        let (session_id, op_id) = ids();
        let frames = [parse(RESULT_UNKNOWN_SUBTYPE), parse(RESULT_SUCCESS)];

        let events = map_claude(&frames, &session_id, &op_id);

        assert_eq!(events.len(), 1, "only the success terminal: {events:?}");
        assert!(matches!(
            &events[0],
            HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn
        ));
        assert_eq!(terminal_count(&events), 1);
    }

    /// `Error(AnthropicError)` → non-terminal `Status(warning)`; the
    /// terminal decision stays with the `Result` frame (AR-1).
    #[test]
    fn error_frame_is_non_terminal_warning() {
        let (session_id, op_id) = ids();
        let frames = [parse(ERROR_FRAME), parse(RESULT_SUCCESS)];

        let events = map_claude(&frames, &session_id, &op_id);

        assert_eq!(events.len(), 2, "warning + terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::Status(s) if s.level == StatusLevel::Warning && s.message.contains("Internal server error")),
            "Anthropic error must surface as a warning: {events:?}"
        );
        assert!(matches!(&events[1], HostEvent::OpFinished(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// `RateLimitEvent` → non-terminal `Status(warning)` (AR-1).
    #[test]
    fn rate_limit_event_is_warning() {
        let (session_id, op_id) = ids();
        let frames = [parse(RATE_LIMIT_EVENT), parse(RESULT_SUCCESS)];

        let events = map_claude(&frames, &session_id, &op_id);

        assert_eq!(events.len(), 2, "warning + terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::Status(s) if s.level == StatusLevel::Warning),
            "rate limit event must surface as a warning: {events:?}"
        );
        assert!(matches!(&events[1], HostEvent::OpFinished(_)));
        assert_eq!(terminal_count(&events), 1);
    }

    /// Known error `Result` subtype → one terminal `OpFailed(provider_error)`
    /// with the CLI's error text (AR-1, AR-7).
    #[test]
    fn error_result_fails_turn_with_provider_error() {
        let (session_id, op_id) = ids();
        let frames = [parse(RESULT_ERROR)];

        let events = map_claude(&frames, &session_id, &op_id);

        assert_eq!(events.len(), 1, "only the terminal: {events:?}");
        assert!(
            matches!(&events[0], HostEvent::OpFailed(f) if f.error_category == "provider_error" && f.error_message.contains("Bash command failed with exit code 1")),
            "error result must fail with provider_error: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// (c) unknown top-level `type` tag → typed-decode failure: the crate's
    /// own parse path (`ClaudeOutput::parse_json_tolerant`, the same one
    /// `AsyncClient::receive` uses) errors with `Error::Deserialization`,
    /// which classifies to one `OpFailed(decode_error)` — never a per-item
    /// skip.
    #[test]
    fn unknown_top_level_type_is_typed_decode_failure_not_skip() {
        let (session_id, op_id) = ids();

        let parse_err = ClaudeOutput::parse_json_tolerant(UNKNOWN_TOP_LEVEL_TYPE)
            .expect_err("unknown top-level type tag must fail typed decode");
        // Same construction `AsyncClient::receive` uses: `parse_error.into()`.
        let err = ClaudeError::Deserialization(parse_err);

        let failed = classify_stream_error(&err, &session_id, &op_id)
            .expect("decode failure must classify to a terminal event");
        assert_eq!(failed.error_category, "decode_error");
        // N-2: error_message is the crate Display, capped at 512 bytes.
        let full = err.to_string();
        let prefix = failed
            .error_message
            .strip_suffix("... (truncated)")
            .unwrap_or(&failed.error_message);
        assert!(
            full.starts_with(prefix),
            "error_message must be the (truncated) Display prefix: {failed:?}"
        );
        assert_eq!(terminal_count(&[HostEvent::OpFailed(failed)]), 1);
    }

    /// AR-7 category tokens for the stream-error classifier, including
    /// `stream_closed` on EOF (`Error::ConnectionClosed`).
    #[test]
    fn classifies_stream_errors_to_ar7_tokens() {
        let (session_id, op_id) = ids();
        let parse_err = ClaudeOutput::parse_json_tolerant(UNKNOWN_TOP_LEVEL_TYPE)
            .expect_err("fixture must fail typed decode");

        let cases: Vec<(ClaudeError, &str)> = vec![
            (ClaudeError::Deserialization(parse_err), "decode_error"),
            (ClaudeError::ConnectionClosed, "stream_closed"),
            (ClaudeError::Io(std::io::Error::other("pipe")), "io_error"),
            (
                ClaudeError::BinaryNotFound {
                    name: "claude".to_string(),
                },
                "io_error",
            ),
            (ClaudeError::Timeout, "timeout"),
            (ClaudeError::Protocol("boom".to_string()), "provider_error"),
            (ClaudeError::Unknown("??".to_string()), "provider_error"),
        ];

        for (error, expected) in cases {
            let failed = classify_stream_error(&error, &session_id, &op_id)
                .unwrap_or_else(|| panic!("{expected} must classify: {error}"));
            assert_eq!(failed.error_category, expected, "for error {error}");
            // N-2: error_message is the crate Display, capped at 512 bytes.
            let full = error.to_string();
            let prefix = failed
                .error_message
                .strip_suffix("... (truncated)")
                .unwrap_or(&failed.error_message);
            assert!(
                full.starts_with(prefix),
                "error_message must be the (truncated) Display prefix for {error}: {failed:?}"
            );
        }
    }

    /// The fixture's unknown subtype string stays a crate-downgraded
    /// `ResultSubtype::Unknown` after parse.
    #[test]
    fn unknown_subtype_fixture_is_crate_downgraded() {
        let output = parse(RESULT_UNKNOWN_SUBTYPE);
        let ClaudeOutput::Result(result) = output else {
            panic!("fixture must parse as a Result frame");
        };
        assert!(matches!(result.subtype, ResultSubtype::Unknown(_)));
    }

    /// N-2: a multi-MB raw wire line embedded in the Deserialization Display
    /// must be capped in `error_message`, not echoed unboundedly.
    #[test]
    fn decode_error_message_truncates_raw_wire() {
        let (session_id, op_id) = ids();

        let parse_err = claude_codes::ParseError {
            raw_line: "y".repeat(4096),
            raw_json: None,
            error_message: "expected value".to_string(),
        };
        let err = ClaudeError::Deserialization(parse_err);

        let failed = classify_stream_error(&err, &session_id, &op_id)
            .expect("decode failure must classify to a terminal event");
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
