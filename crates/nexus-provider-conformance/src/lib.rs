//! nexus-provider-conformance — neutral provider stream conformance runner.
//!
//! A developer-visible CI gate (v1.180 P0, RN-OGA-1): a PR that drifts
//! native-provider stream lifecycle (`claude-codes` / `codex-codes` /
//! `dsh-native`) fails in CI without credentials. This crate consumes the
//! normalized-stream contract types
//! `nexus_agent_host::capability::model::{HostEvent, HostEventStream}` and
//! asserts adapter-normalized invariants over one operation stream.
//!
//! The runner is provider-neutral: it is a pure function of the normalized
//! stream plus the [`ConformanceConfig`] bounds. Vendor wire bytes live only
//! in fixtures (Task 2); this crate never parses vendor frames.
//!
//! # Invariants
//!
//! - exactly one `OpStarted` ([`invariants::started`])
//! - bounded event count / duration ([`invariants::bounds`])
//! - lifecycle ordering: `OpStarted` before op-scoped events, terminal last
//!   ([`invariants::ordering`])
//! - single terminal: exactly one `OpFinished` | `OpFailed`, then the stream
//!   ends ([`invariants::terminal`])
//! - stop-reason consistency: `FinishReason` / `SessionStopReason` closed sets
//!   ([`invariants::stop_reason`])
//! - forbidden-value exclusion: closed `error_category`, 512-byte
//!   `error_message` cap, closed enum fields ([`invariants::values`])

#![deny(clippy::unwrap_used)]
// Test modules use ergonomic `.unwrap()`/`.expect()`; production keeps the strict deny.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]

pub mod invariants;
pub mod model;

pub use model::{ConformanceConfig, ConformanceFinding, ConformanceReport, InvariantId};

use std::time::Instant;

use nexus_agent_host::capability::model::HostEventStream;

/// Run the conformance checks over one operation stream.
///
/// Collects the stream under the configured bounds, then evaluates every
/// invariant over the collected events. The returned report carries one typed
/// [`ConformanceFinding`] per violation with event-index evidence.
#[must_use]
pub async fn run_conformance(
    stream: HostEventStream,
    config: ConformanceConfig,
) -> ConformanceReport {
    let started_at = Instant::now();
    let collected = invariants::bounds::collect(stream, config).await;

    let mut findings = collected.findings;
    findings.extend(invariants::started::check(&collected.events));
    findings.extend(invariants::ordering::check(&collected.events));
    findings.extend(invariants::terminal::check(
        &collected.events,
        collected.stream_error.as_ref(),
    ));
    findings.extend(invariants::stop_reason::check(&collected.events));
    findings.extend(invariants::values::check(&collected.events));

    ConformanceReport {
        findings,
        event_count: collected.events.len(),
        duration: started_at.elapsed(),
        truncated: collected.truncated,
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Helpers that build real `HostEvent` values for invariant tests.

    use nexus_agent_host::capability::model::{
        FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent,
        OperationStartedEvent, PlanUpdateEvent, SessionCreatedEvent, SessionStopReason,
        SessionStoppedEvent, StatusEvent, StatusLevel, TextDeltaEvent, ToolCallEvent,
        ToolCallUpdateEvent,
    };
    use nexus_agent_host::{HostOperationId, HostSessionId};

    pub fn op_started() -> HostEvent {
        HostEvent::OpStarted(OperationStartedEvent {
            op_id: HostOperationId::new(),
            session_id: HostSessionId::new(),
        })
    }

    pub fn thought_delta() -> HostEvent {
        HostEvent::ThoughtDelta(TextDeltaEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            text: "thinking".to_string(),
        })
    }

    pub fn message_delta() -> HostEvent {
        HostEvent::MessageDelta(TextDeltaEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            text: "hello".to_string(),
        })
    }

    pub fn tool_call() -> HostEvent {
        HostEvent::ToolCall(ToolCallEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            tool_call_id: "call_1".to_string(),
            tool_name: "bash".to_string(),
        })
    }

    pub fn tool_call_update() -> HostEvent {
        HostEvent::ToolCallUpdate(ToolCallUpdateEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            tool_call_id: "call_1".to_string(),
            content: "done".to_string(),
        })
    }

    pub fn plan_update() -> HostEvent {
        HostEvent::PlanUpdate(PlanUpdateEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            content: "plan".to_string(),
        })
    }

    pub fn status(level: StatusLevel) -> HostEvent {
        HostEvent::Status(StatusEvent {
            session_id: Some(HostSessionId::new()),
            level,
            message: "status".to_string(),
        })
    }

    pub fn op_finished(reason: FinishReason) -> HostEvent {
        HostEvent::OpFinished(OperationFinishedEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            reason,
        })
    }

    pub fn op_failed(category: &str, message: &str) -> HostEvent {
        HostEvent::OpFailed(OperationFailedEvent {
            session_id: HostSessionId::new(),
            op_id: HostOperationId::new(),
            error_category: category.to_string(),
            error_message: message.to_string(),
        })
    }

    pub fn session_created() -> HostEvent {
        HostEvent::SessionCreated(SessionCreatedEvent {
            session_id: HostSessionId::new(),
            provider_id: "mock".to_string().into(),
        })
    }

    pub fn session_stopped(reason: SessionStopReason) -> HostEvent {
        HostEvent::SessionStopped(SessionStoppedEvent {
            session_id: HostSessionId::new(),
            reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        message_delta, op_failed, op_finished, op_started, tool_call, tool_call_update,
    };
    use futures_util::StreamExt;
    use nexus_agent_host::capability::model::{FinishReason, HostEvent};
    use nexus_agent_host::HostError;
    use std::time::Duration;

    fn stream_of(events: Vec<HostEvent>) -> HostEventStream {
        futures_util::stream::iter(events.into_iter().map(Ok)).boxed()
    }

    #[tokio::test]
    async fn happy_path_passes() {
        let events = vec![
            op_started(),
            message_delta(),
            tool_call(),
            tool_call_update(),
            op_finished(FinishReason::EndTurn),
        ];
        let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
        assert!(report.passed(), "expected clean report: {report}");
        assert_eq!(report.event_count, 5);
        assert!(!report.truncated);
    }

    #[tokio::test]
    async fn missing_started_is_reported() {
        let events = vec![message_delta(), op_finished(FinishReason::EndTurn)];
        let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
        assert!(!report.passed());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.invariant == InvariantId::ExactlyOneStarted),
            "findings: {report}"
        );
    }

    #[tokio::test]
    async fn duplicate_terminal_is_reported() {
        let events = vec![
            op_started(),
            op_finished(FinishReason::EndTurn),
            op_failed("provider_error", "boom"),
        ];
        let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
        assert!(!report.passed());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.invariant == InvariantId::SingleTerminal),
            "findings: {report}"
        );
    }

    #[tokio::test]
    async fn forbidden_error_category_is_reported() {
        let events = vec![op_started(), op_failed("vendor_raw", "boom")];
        let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
        assert!(!report.passed());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.invariant == InvariantId::ForbiddenValueExclusion),
            "findings: {report}"
        );
    }

    #[tokio::test]
    async fn event_count_bound_is_reported() {
        let events = vec![op_started(); 5];
        let config = ConformanceConfig {
            max_events: 3,
            ..ConformanceConfig::default()
        };
        let report = run_conformance(stream_of(events), config).await;
        assert!(!report.passed());
        assert!(report.truncated);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.invariant == InvariantId::BoundedStream),
            "findings: {report}"
        );
    }

    #[tokio::test]
    async fn duration_bound_is_reported() {
        let stream = futures_util::stream::once(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok(op_started())
        })
        .boxed();
        let config = ConformanceConfig {
            max_duration: Duration::from_millis(10),
            ..ConformanceConfig::default()
        };
        let report = run_conformance(stream, config).await;
        assert!(!report.passed());
        assert!(report.truncated);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.invariant == InvariantId::BoundedStream),
            "findings: {report}"
        );
    }

    #[tokio::test]
    async fn stream_error_is_reported() {
        let stream = futures_util::stream::iter(vec![
            Ok(op_started()),
            Err(HostError::ProviderUnavailable {
                provider_id: "mock".to_string().into(),
                message: "bad frame".to_string(),
            }),
        ])
        .boxed();
        let report = run_conformance(stream, ConformanceConfig::default()).await;
        assert!(!report.passed());
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.invariant == InvariantId::SingleTerminal),
            "findings: {report}"
        );
    }
}
