//! `HostEvent` mapping for the `deepseek_harness_sdk` high-level surface
//! (v1.188 P1 message-level streaming conformance).
//!
//! Classifies SDK 0.2 tree notifications, emits complete root
//! `assistant/message` units during `Session::run`, and reconciles the
//! high-level `RunResult` with a boolean `emitted_root_text` (no cumulative
//! transcript). Consumed by the dsh provider execute loop and producer.

use deepseek_harness_sdk::api::extract_finish_reason;
use deepseek_harness_sdk::{Error, Notification, RunResult};
use serde_json::Value;

use crate::capability::model::{
    FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, TextDeltaEvent,
};
use crate::ids::{HostOperationId, HostSessionId};

/// Outcome of classifying one SDK tree notification for the dsh adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DshNotificationClass {
    /// Known non-content, nested, or forward-compatible noise.
    Ignore,
    /// One complete root assistant message with non-empty prose.
    RootText(String),
}

/// Per-root-message text bound enforced before allocation (architecture §4.3).
pub(crate) const DSCH_MAX_ROOT_MESSAGE_TEXT_BYTES: usize = 256 * 1024;

/// Classifier failure for a recognized wire envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DshProtocolFailure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DshEventTooLarge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassifyNotificationError {
    Protocol(DshProtocolFailure),
    EventTooLarge(DshEventTooLarge),
}

/// Tracks whether any root message delta was emitted during the run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunReconciliation {
    pub emitted_root_text: bool,
}

impl RunReconciliation {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            emitted_root_text: false,
        }
    }

    pub fn note_root_text_emitted(&mut self) {
        self.emitted_root_text = true;
    }
}

/// Classify one notification delivered to the `Session::run` observer.
///
/// Root equality uses the SDK session id passed to `start_session`, not the
/// host UUID. Malformed recognized envelopes fail; unknown well-formed event
/// types are ignored.
#[must_use]
pub fn classify_notification(
    notification: &Notification,
    root_session_id: &str,
) -> Result<DshNotificationClass, ClassifyNotificationError> {
    match notification.method.as_str() {
        "session.event" => {
            let event = notification
                .session_event()
                .ok_or(ClassifyNotificationError::Protocol(DshProtocolFailure))?
                .map_err(|_| ClassifyNotificationError::Protocol(DshProtocolFailure))?;
            if event.session_id != root_session_id {
                return Ok(DshNotificationClass::Ignore);
            }
            classify_root_session_event(&event.event).map_err(|failure| match failure {
                RootEventFailure::Protocol => ClassifyNotificationError::Protocol(DshProtocolFailure),
                RootEventFailure::TooLarge => ClassifyNotificationError::EventTooLarge(DshEventTooLarge),
            })
        }
        "session.status" => {
            notification
                .session_status()
                .ok_or(ClassifyNotificationError::Protocol(DshProtocolFailure))?
                .map_err(|_| ClassifyNotificationError::Protocol(DshProtocolFailure))?;
            Ok(DshNotificationClass::Ignore)
        }
        "subagent.started" => {
            notification
                .subagent_started()
                .ok_or(ClassifyNotificationError::Protocol(DshProtocolFailure))?
                .map_err(|_| ClassifyNotificationError::Protocol(DshProtocolFailure))?;
            Ok(DshNotificationClass::Ignore)
        }
        "subagent.finished" => {
            notification
                .subagent_finished()
                .ok_or(ClassifyNotificationError::Protocol(DshProtocolFailure))?
                .map_err(|_| ClassifyNotificationError::Protocol(DshProtocolFailure))?;
            Ok(DshNotificationClass::Ignore)
        }
        _ => Ok(DshNotificationClass::Ignore),
    }
}

enum RootEventFailure {
    Protocol,
    TooLarge,
}

fn classify_root_session_event(event: &Value) -> Result<DshNotificationClass, RootEventFailure> {
    if event.get("type").and_then(Value::as_str) != Some("assistant/message") {
        return Ok(DshNotificationClass::Ignore);
    }
    let len = measure_assistant_message_text(event).map_err(|_| RootEventFailure::Protocol)?;
    if len == 0 {
        return Ok(DshNotificationClass::Ignore);
    }
    if len > DSCH_MAX_ROOT_MESSAGE_TEXT_BYTES {
        return Err(RootEventFailure::TooLarge);
    }
    let text = build_assistant_message_text(event).map_err(|_| RootEventFailure::Protocol)?;
    Ok(DshNotificationClass::RootText(text))
}

fn assistant_message_content(event: &Value) -> Result<&Vec<Value>, DshProtocolFailure> {
    let content = if event.pointer("/data/message").is_some_and(Value::is_object) {
        event.pointer("/data/message/content")
    } else {
        event.pointer("/data/content")
    };
    content.and_then(Value::as_array).ok_or(DshProtocolFailure)
}

fn measure_assistant_message_text(event: &Value) -> Result<usize, DshProtocolFailure> {
    let blocks = assistant_message_content(event)?;
    let mut len = 0usize;
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        match block.get("text") {
            None | Some(Value::Null) => return Err(DshProtocolFailure),
            Some(Value::String(text)) => len += text.len(),
            Some(_) => return Err(DshProtocolFailure),
        }
    }
    Ok(len)
}

fn build_assistant_message_text(event: &Value) -> Result<String, DshProtocolFailure> {
    let blocks = assistant_message_content(event)?;
    let mut out = String::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        match block.get("text") {
            None | Some(Value::Null) => return Err(DshProtocolFailure),
            Some(Value::String(text)) => out.push_str(text),
            Some(_) => return Err(DshProtocolFailure),
        }
    }
    Ok(out)
}

/// Build terminal (and optional fallback delta) events after a successful
/// `Session::run`. Streaming deltas emitted during the run are not repeated.
#[must_use]
pub fn finalize_successful_run(
    result: &RunResult,
    reconciliation: &RunReconciliation,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> Result<Vec<HostEvent>, OperationFailedEvent> {
    let finish_reason = match extract_finish_reason(&result.events) {
        Ok(reason) => reason,
        Err(_) => {
            return Err(failed_turn(
                session_id,
                op_id,
                "decode_error",
                "dsh turn ended with a malformed finish reason",
            ));
        }
    };

    match finish_reason.as_deref() {
        Some("completed") => {
            let mut events = Vec::new();
            if !reconciliation.emitted_root_text {
                if result.final_response.is_empty() {
                    return Err(failed_turn(
                        session_id,
                        op_id,
                        "provider_error",
                        "dsh turn completed without any assistant text",
                    ));
                }
                let bytes = result.final_response.len();
                if bytes > DSCH_MAX_ROOT_MESSAGE_TEXT_BYTES {
                    return Err(operation_delivery_overflow_failure(session_id, op_id));
                }
                events.push(message_delta(
                    session_id,
                    op_id,
                    result.final_response.clone(),
                ));
            }
            events.push(HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                reason: FinishReason::EndTurn,
            }));
            Ok(events)
        }
        Some("max-tokens") => Err(failed_turn(
            session_id,
            op_id,
            "max_tokens",
            "dsh turn reached the maximum token limit",
        )),
        _ => Err(failed_turn(
            session_id,
            op_id,
            "provider_error",
            "dsh turn ended without a completed finish reason",
        )),
    }
}

fn message_delta(session_id: &HostSessionId, op_id: &HostOperationId, text: String) -> HostEvent {
    HostEvent::MessageDelta(TextDeltaEvent {
        session_id: session_id.clone(),
        op_id: op_id.clone(),
        text,
    })
}

fn failed_turn(
    session_id: &HostSessionId,
    op_id: &HostOperationId,
    category: &str,
    message: &str,
) -> OperationFailedEvent {
    OperationFailedEvent {
        session_id: session_id.clone(),
        op_id: op_id.clone(),
        error_category: category.to_string(),
        error_message: message.to_string(),
    }
}

/// Classify a `deepseek_harness_sdk` start/run error into the one terminal
/// `OpFailed` event of the failed turn (PD-3, AR-7 category tokens).
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
/// P0 T2).
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
        Error::RequestTimeout { method, .. } => (
            "timeout",
            match method.as_str() {
                known @ ("initialize" | "session/prompt" | "shutdown") => {
                    format!("dsh request timed out: {known}")
                }
                _ => "dsh request timed out".to_string(),
            },
        ),
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

/// Static failure for classifier/protocol/overflow detected during streaming.
#[must_use]
pub fn operation_protocol_failure(
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> OperationFailedEvent {
    failed_turn(
        session_id,
        op_id,
        "decode_error",
        "dsh runtime violated the wire protocol",
    )
}

/// Static failure when Nexus-owned delivery bounds are exceeded.
#[must_use]
pub fn operation_delivery_overflow_failure(
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> OperationFailedEvent {
    failed_turn(
        session_id,
        op_id,
        "provider_error",
        "dsh operation exceeded Nexus delivery bounds",
    )
}

#[cfg(test)]
mod tests {
    use deepseek_harness_sdk::Notification;
    use serde_json::{json, Map};

    use super::*;

    fn ids() -> (HostSessionId, HostOperationId) {
        (
            HostSessionId(uuid::Uuid::new_v4()),
            HostOperationId(uuid::Uuid::new_v4()),
        )
    }

    fn notification(method: &str, payload: Map<String, Value>) -> Notification {
        Notification {
            method: method.to_string(),
            payload,
        }
    }

    fn root_event(event: Value) -> Notification {
        let mut payload = Map::new();
        payload.insert("sessionId".to_string(), json!("root-sess"));
        payload.insert("event".to_string(), event);
        notification("session.event", payload)
    }

    #[test]
    fn root_assistant_message_becomes_root_text() {
        let n = root_event(json!({
            "type": "assistant/message",
            "data": {"message": {"content": [{"type": "text", "text": "hello"}]}}
        }));
        assert_eq!(
            classify_notification(&n, "root-sess"),
            Ok(DshNotificationClass::RootText("hello".to_string()))
        );
    }

    #[test]
    fn nested_assistant_message_is_ignored() {
        let mut payload = Map::new();
        payload.insert("sessionId".to_string(), json!("child-sess"));
        payload.insert(
            "event".to_string(),
            json!({
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": "nested"}]}
            }),
        );
        let n = notification("session.event", payload);
        assert_eq!(
            classify_notification(&n, "root-sess"),
            Ok(DshNotificationClass::Ignore)
        );
    }

    #[test]
    fn subagent_last_assistant_message_is_never_root_text() {
        let mut payload = Map::new();
        payload.insert("provider".to_string(), json!("deepseek"));
        payload.insert("agentId".to_string(), json!("a1"));
        payload.insert("parentSessionId".to_string(), json!("root-sess"));
        payload.insert("childSessionId".to_string(), json!("child"));
        payload.insert("status".to_string(), json!("ok"));
        payload.insert("stopReason".to_string(), json!("completed"));
        payload.insert(
            "lastAssistantMessage".to_string(),
            json!([{"type": "text", "text": "child prose"}]),
        );
        let n = notification("subagent.finished", payload);
        assert_eq!(
            classify_notification(&n, "root-sess"),
            Ok(DshNotificationClass::Ignore)
        );
    }

    #[test]
    fn malformed_session_event_is_protocol_failure() {
        let mut payload = Map::new();
        payload.insert("sessionId".to_string(), json!(7));
        payload.insert("event".to_string(), json!({"type": "assistant/message"}));
        let n = notification("session.event", payload);
        assert!(classify_notification(&n, "root-sess").is_err());
    }

    #[test]
    fn missing_content_array_is_protocol_failure() {
        let n = root_event(json!({
            "type": "assistant/message",
            "data": {"content": null}
        }));
        assert!(classify_notification(&n, "root-sess").is_err());
    }

    #[test]
    fn absent_content_is_protocol_failure() {
        let n = root_event(json!({
            "type": "assistant/message",
            "data": {}
        }));
        assert!(classify_notification(&n, "root-sess").is_err());
    }

    #[test]
    fn missing_text_field_is_protocol_failure() {
        let n = root_event(json!({
            "type": "assistant/message",
            "data": {"content": [{"type": "text"}]}
        }));
        assert!(classify_notification(&n, "root-sess").is_err());
    }

    #[test]
    fn oversize_root_message_fails_before_delivery() {
        let huge = "x".repeat(DSCH_MAX_ROOT_MESSAGE_TEXT_BYTES + 1);
        let n = root_event(json!({
            "type": "assistant/message",
            "data": {"content": [{"type": "text", "text": huge}]}
        }));
        assert!(classify_notification(&n, "root-sess").is_err());
    }

    #[test]
    fn non_string_text_block_is_protocol_failure() {
        let n = root_event(json!({
            "type": "assistant/message",
            "data": {"content": [{"type": "text", "text": 123}]}
        }));
        assert!(classify_notification(&n, "root-sess").is_err());
    }

    #[test]
    fn completed_run_with_emitted_text_ignores_final_response() {
        let (session_id, op_id) = ids();
        let result = RunResult {
            session_id: "root-sess".to_string(),
            final_response: "different final".to_string(),
            finish_reason: Some("completed".to_string()),
            events: vec![json!({
                "type": "turn/end",
                "data": {"reason": {"kind": "completed"}}
            })],
            notifications: Vec::new(),
        };
        let reconciliation = RunReconciliation {
            emitted_root_text: true,
        };
        let events = finalize_successful_run(&result, &reconciliation, &session_id, &op_id)
            .expect("completed turn");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], HostEvent::OpFinished(_)));
    }

    #[test]
    fn oversized_fallback_final_response_fails_delivery_bounds() {
        let (session_id, op_id) = ids();
        let huge = "x".repeat(DSCH_MAX_ROOT_MESSAGE_TEXT_BYTES + 1);
        let result = RunResult {
            session_id: "root-sess".to_string(),
            final_response: huge,
            finish_reason: Some("completed".to_string()),
            events: vec![json!({
                "type": "turn/end",
                "data": {"reason": {"kind": "completed"}}
            })],
            notifications: Vec::new(),
        };
        let err = finalize_successful_run(
            &result,
            &RunReconciliation::default(),
            &session_id,
            &op_id,
        )
        .expect_err("oversized fallback must fail");
        assert_eq!(err.error_category, "provider_error");
        assert_eq!(
            err.error_message,
            "dsh operation exceeded Nexus delivery bounds"
        );
    }

    #[test]
    fn fallback_emits_final_once_when_no_streamed_text() {
        let (session_id, op_id) = ids();
        let result = RunResult {
            session_id: "root-sess".to_string(),
            final_response: "only final".to_string(),
            finish_reason: Some("completed".to_string()),
            events: vec![
                json!({"type": "assistant/message", "data": {"content": [{"type": "text", "text": "only final"}]}}),
                json!({"type": "turn/end", "data": {"reason": {"kind": "completed"}}}),
            ],
            notifications: Vec::new(),
        };
        let events = finalize_successful_run(
            &result,
            &RunReconciliation::default(),
            &session_id,
            &op_id,
        )
        .expect("fallback turn");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], HostEvent::MessageDelta(d) if d.text == "only final"));
        assert!(matches!(&events[1], HostEvent::OpFinished(_)));
    }

    #[test]
    fn empty_completed_run_fails() {
        let (session_id, op_id) = ids();
        let result = RunResult {
            session_id: "root-sess".to_string(),
            final_response: String::new(),
            finish_reason: Some("completed".to_string()),
            events: vec![json!({
                "type": "turn/end",
                "data": {"reason": {"kind": "completed"}}
            })],
            notifications: Vec::new(),
        };
        assert!(finalize_successful_run(
            &result,
            &RunReconciliation::default(),
            &session_id,
            &op_id
        )
        .is_err());
    }

    #[test]
    fn max_tokens_finish_is_failed_turn() {
        let (session_id, op_id) = ids();
        let result = RunResult {
            session_id: "root-sess".to_string(),
            final_response: "partial".to_string(),
            finish_reason: Some("max-tokens".to_string()),
            events: vec![json!({
                "type": "turn/end",
                "data": {"reason": {"kind": "max-tokens"}}
            })],
            notifications: Vec::new(),
        };
        let err = finalize_successful_run(
            &result,
            &RunReconciliation {
                emitted_root_text: true,
            },
            &session_id,
            &op_id,
        )
        .expect_err("max-tokens is not success");
        assert_eq!(err.error_category, "max_tokens");
    }

    #[test]
    fn a_then_b_finalize_emits_only_end_turn_when_text_already_streamed() {
        let (session_id, op_id) = ids();
        let result = RunResult {
            session_id: "root-sess".to_string(),
            final_response: "B".to_string(),
            finish_reason: Some("completed".to_string()),
            events: vec![
                json!({"type": "assistant/message", "data": {"content": [{"type": "text", "text": "A"}]}}),
                json!({"type": "assistant/message", "data": {"content": [{"type": "text", "text": "B"}]}}),
                json!({"type": "turn/end", "data": {"reason": {"kind": "completed"}}}),
            ],
            notifications: Vec::new(),
        };
        let events = finalize_successful_run(
            &result,
            &RunReconciliation {
                emitted_root_text: true,
            },
            &session_id,
            &op_id,
        )
        .expect("completed");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], HostEvent::OpFinished(_)));
    }

}
