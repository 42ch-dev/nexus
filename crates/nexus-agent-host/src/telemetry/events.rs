//! Structured telemetry event helpers for all `HostEvent` variants.
//!
//! Each event carries: `run_id` (Option), `session_id`, `provider_id`,
//! `protocol_kind`, `timestamp`. Provides serialization helpers for JSON output
//! and optional JSONL trace support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::capability::model::{
    FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    PlanUpdateEvent, SessionCreatedEvent, SessionStopReason, SessionStoppedEvent, StatusEvent,
    StatusLevel, TextDeltaEvent, ToolCallEvent,
};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};

/// Common context for telemetry event construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryContext {
    /// Optional run ID for correlation across operations.
    pub run_id: Option<String>,
    /// Provider ID for the event source.
    pub provider_id: ProviderId,
    /// Protocol kind of the provider.
    pub protocol_kind: crate::capability::model::ProtocolKind,
    /// Timestamp for the event.
    pub timestamp: DateTime<Utc>,
}

impl TelemetryContext {
    /// Create a new telemetry context.
    #[must_use]
    pub fn new(
        provider_id: ProviderId,
        protocol_kind: crate::capability::model::ProtocolKind,
    ) -> Self {
        Self {
            run_id: None,
            provider_id,
            protocol_kind,
            timestamp: Utc::now(),
        }
    }

    /// Set the run ID.
    #[must_use]
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }
}

// ── Event creation helpers ──────────────────────────────────────────

/// Create a `SessionCreated` event.
#[must_use]
pub const fn session_created(session_id: HostSessionId, provider_id: ProviderId) -> HostEvent {
    HostEvent::SessionCreated(SessionCreatedEvent {
        session_id,
        provider_id,
    })
}

/// Create an `OpStarted` event.
#[must_use]
pub const fn op_started(op_id: HostOperationId, session_id: HostSessionId) -> HostEvent {
    HostEvent::OpStarted(OperationStartedEvent { op_id, session_id })
}

/// Create a `ThoughtDelta` event.
#[must_use]
pub const fn thought_delta(
    session_id: HostSessionId,
    op_id: HostOperationId,
    text: String,
) -> HostEvent {
    HostEvent::ThoughtDelta(TextDeltaEvent {
        session_id,
        op_id,
        text,
    })
}

/// Create a `MessageDelta` event.
#[must_use]
pub const fn message_delta(
    session_id: HostSessionId,
    op_id: HostOperationId,
    text: String,
) -> HostEvent {
    HostEvent::MessageDelta(TextDeltaEvent {
        session_id,
        op_id,
        text,
    })
}

/// Create a `ToolCall` event.
#[must_use]
pub const fn tool_call(
    session_id: HostSessionId,
    op_id: HostOperationId,
    tool_call_id: String,
    tool_name: String,
) -> HostEvent {
    HostEvent::ToolCall(ToolCallEvent {
        session_id,
        op_id,
        tool_call_id,
        tool_name,
    })
}

/// Create a `PlanUpdate` event.
#[must_use]
pub const fn plan_update(
    session_id: HostSessionId,
    op_id: HostOperationId,
    content: String,
) -> HostEvent {
    HostEvent::PlanUpdate(PlanUpdateEvent {
        session_id,
        op_id,
        content,
    })
}

/// Create a `Status` event.
#[must_use]
pub const fn status_event(
    session_id: Option<HostSessionId>,
    level: StatusLevel,
    message: String,
) -> HostEvent {
    HostEvent::Status(StatusEvent {
        session_id,
        level,
        message,
    })
}

/// Create an `OpFinished` terminal event.
#[must_use]
pub const fn op_finished(
    session_id: HostSessionId,
    op_id: HostOperationId,
    reason: FinishReason,
) -> HostEvent {
    HostEvent::OpFinished(OperationFinishedEvent {
        session_id,
        op_id,
        reason,
    })
}

/// Create an `OpFailed` terminal event.
#[must_use]
pub const fn op_failed(
    session_id: HostSessionId,
    op_id: HostOperationId,
    error_category: String,
    error_message: String,
) -> HostEvent {
    HostEvent::OpFailed(OperationFailedEvent {
        session_id,
        op_id,
        error_category,
        error_message,
    })
}

/// Create a `SessionStopped` event.
#[must_use]
pub const fn session_stopped(session_id: HostSessionId, reason: SessionStopReason) -> HostEvent {
    HostEvent::SessionStopped(SessionStoppedEvent { session_id, reason })
}

// ── Serialization helpers ───────────────────────────────────────────

/// A telemetry event enriched with context for JSONL output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedEvent {
    /// Telemetry context.
    #[serde(flatten)]
    pub context: TelemetryContext,
    /// The host event.
    pub event: HostEvent,
}

impl EnrichedEvent {
    /// Create an enriched event from context and host event.
    #[must_use]
    pub const fn new(context: TelemetryContext, event: HostEvent) -> Self {
        Self { context, event }
    }

    /// Serialize to JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::model::ProtocolKind;

    fn test_session_id() -> HostSessionId {
        HostSessionId::new()
    }

    fn test_op_id() -> HostOperationId {
        HostOperationId::new()
    }

    fn test_provider_id() -> ProviderId {
        ProviderId::new("test-provider")
    }

    #[test]
    fn session_created_event() {
        let sid = test_session_id();
        let pid = test_provider_id();
        let event = session_created(sid.clone(), pid.clone());
        match event {
            HostEvent::SessionCreated(e) => {
                assert_eq!(e.session_id, sid);
                assert_eq!(e.provider_id, pid);
            }
            _ => panic!("expected SessionCreated"),
        }
    }

    #[test]
    fn op_started_event() {
        let sid = test_session_id();
        let oid = test_op_id();
        let event = op_started(oid.clone(), sid.clone());
        match event {
            HostEvent::OpStarted(e) => {
                assert_eq!(e.op_id, oid);
                assert_eq!(e.session_id, sid);
            }
            _ => panic!("expected OpStarted"),
        }
    }

    #[test]
    fn thought_delta_event() {
        let sid = test_session_id();
        let oid = test_op_id();
        let event = thought_delta(sid.clone(), oid.clone(), "thinking...".to_string());
        match event {
            HostEvent::ThoughtDelta(e) => {
                assert_eq!(e.text, "thinking...");
            }
            _ => panic!("expected ThoughtDelta"),
        }
    }

    #[test]
    fn message_delta_event() {
        let sid = test_session_id();
        let oid = test_op_id();
        let event = message_delta(sid.clone(), oid.clone(), "hello".to_string());
        match event {
            HostEvent::MessageDelta(e) => {
                assert_eq!(e.text, "hello");
            }
            _ => panic!("expected MessageDelta"),
        }
    }

    #[test]
    fn tool_call_event() {
        let sid = test_session_id();
        let oid = test_op_id();
        let event = tool_call(
            sid.clone(),
            oid.clone(),
            "tc-1".to_string(),
            "file_read".to_string(),
        );
        match event {
            HostEvent::ToolCall(e) => {
                assert_eq!(e.tool_call_id, "tc-1");
                assert_eq!(e.tool_name, "file_read");
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn op_finished_event() {
        let sid = test_session_id();
        let oid = test_op_id();
        let event = op_finished(sid.clone(), oid.clone(), FinishReason::EndTurn);
        match event {
            HostEvent::OpFinished(e) => {
                assert_eq!(e.reason, FinishReason::EndTurn);
            }
            _ => panic!("expected OpFinished"),
        }
    }

    #[test]
    fn op_failed_event() {
        let sid = test_session_id();
        let oid = test_op_id();
        let event = op_failed(
            sid.clone(),
            oid.clone(),
            "provider_protocol_error".to_string(),
            "connection reset".to_string(),
        );
        match event {
            HostEvent::OpFailed(e) => {
                assert_eq!(e.error_category, "provider_protocol_error");
                assert_eq!(e.error_message, "connection reset");
            }
            _ => panic!("expected OpFailed"),
        }
    }

    #[test]
    fn session_stopped_event() {
        let sid = test_session_id();
        let event = session_stopped(sid.clone(), SessionStopReason::GracefulShutdown);
        match event {
            HostEvent::SessionStopped(e) => {
                assert_eq!(e.reason, SessionStopReason::GracefulShutdown);
            }
            _ => panic!("expected SessionStopped"),
        }
    }

    #[test]
    fn status_event_with_session() {
        let sid = test_session_id();
        let event = status_event(
            Some(sid.clone()),
            StatusLevel::Warning,
            "slow response".to_string(),
        );
        match event {
            HostEvent::Status(e) => {
                assert_eq!(e.level, StatusLevel::Warning);
                assert!(e.session_id.is_some());
            }
            _ => panic!("expected Status"),
        }
    }

    #[test]
    fn status_event_without_session() {
        let event = status_event(None, StatusLevel::Info, "host started".to_string());
        match event {
            HostEvent::Status(e) => {
                assert!(e.session_id.is_none());
            }
            _ => panic!("expected Status"),
        }
    }

    #[test]
    fn telemetry_context_new() {
        let ctx = TelemetryContext::new(ProviderId::new("test"), ProtocolKind::Acp);
        assert!(ctx.run_id.is_none());
        assert_eq!(ctx.provider_id.0, "test");
    }

    #[test]
    fn telemetry_context_with_run_id() {
        let ctx = TelemetryContext::new(ProviderId::new("test"), ProtocolKind::Acp)
            .with_run_id("run-123");
        assert_eq!(ctx.run_id.as_deref(), Some("run-123"));
    }

    #[test]
    fn enriched_event_serialization() {
        let ctx = TelemetryContext::new(ProviderId::new("test"), ProtocolKind::Acp)
            .with_run_id("run-123");
        let sid = test_session_id();
        let oid = test_op_id();
        let event = op_finished(sid.clone(), oid.clone(), FinishReason::EndTurn);
        let enriched = EnrichedEvent::new(ctx, event);

        let json = enriched.to_json().expect("should serialize");
        assert!(json.contains("run-123"));
        assert!(json.contains("test"));

        let pretty = enriched.to_json_pretty().expect("should serialize pretty");
        assert!(pretty.contains("run_id"));
        assert!(pretty.contains("provider_id"));
    }

    #[test]
    fn enriched_event_roundtrip() {
        let ctx = TelemetryContext::new(ProviderId::new("test"), ProtocolKind::NativeCli)
            .with_run_id("run-456");
        let sid = test_session_id();
        let event = session_created(sid.clone(), ProviderId::new("test"));
        let enriched = EnrichedEvent::new(ctx, event);

        let json = enriched.to_json().expect("should serialize");
        let deserialized: EnrichedEvent = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized.context.run_id.as_deref(), Some("run-456"));
    }

    #[test]
    fn jsonl_format_multiple_events() {
        let ctx = TelemetryContext::new(ProviderId::new("test"), ProtocolKind::Acp);
        let sid = test_session_id();
        let oid = test_op_id();

        let events = vec![
            EnrichedEvent::new(
                ctx.clone(),
                session_created(sid.clone(), ProviderId::new("test")),
            ),
            EnrichedEvent::new(ctx.clone(), op_started(oid.clone(), sid.clone())),
            EnrichedEvent::new(
                ctx.clone(),
                message_delta(sid.clone(), oid.clone(), "hello".into()),
            ),
            EnrichedEvent::new(ctx, op_finished(sid, oid, FinishReason::EndTurn)),
        ];

        let jsonl: String = events
            .iter()
            .map(|e| e.to_json().expect("should serialize"))
            .collect::<Vec<_>>()
            .join("\n");

        let lines: Vec<&str> = jsonl.lines().collect();
        assert_eq!(lines.len(), 4);
        for line in &lines {
            assert!(serde_json::from_str::<serde_json::Value>(line).is_ok());
        }
    }
}
