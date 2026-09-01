//! Invariant: lifecycle ordering.
//!
//! `OpStarted` must precede every op-scoped event (`ThoughtDelta`,
//! `MessageDelta`, `ToolCall`, `ToolCallUpdate`, `PlanUpdate`), and the
//! terminal event (`OpFinished` | `OpFailed`) must be the last event in the
//! stream.
//!
//! Session-scoped events are exempt from the "after `OpStarted`" rule:
//! `SessionCreated` (session bootstrap precedes the operation) and `Status`
//! (session-level status messages carry no `op_id`). The "terminal last" rule
//! applies to every event kind.

use nexus_agent_host::capability::model::HostEvent;

use crate::model::{ConformanceFinding, InvariantId};

/// Whether the event is operation-scoped and must follow `OpStarted`.
const fn is_op_scoped(event: &HostEvent) -> bool {
    matches!(
        event,
        HostEvent::ThoughtDelta(_)
            | HostEvent::MessageDelta(_)
            | HostEvent::ToolCall(_)
            | HostEvent::ToolCallUpdate(_)
            | HostEvent::PlanUpdate(_)
    )
}

/// Whether the event is a terminal event.
const fn is_terminal(event: &HostEvent) -> bool {
    matches!(event, HostEvent::OpFinished(_) | HostEvent::OpFailed(_))
}

/// Human-readable variant name for finding messages.
const fn variant_name(event: &HostEvent) -> &'static str {
    match event {
        HostEvent::SessionCreated(_) => "SessionCreated",
        HostEvent::OpStarted(_) => "OpStarted",
        HostEvent::ThoughtDelta(_) => "ThoughtDelta",
        HostEvent::MessageDelta(_) => "MessageDelta",
        HostEvent::ToolCall(_) => "ToolCall",
        HostEvent::ToolCallUpdate(_) => "ToolCallUpdate",
        HostEvent::PlanUpdate(_) => "PlanUpdate",
        HostEvent::Status(_) => "Status",
        HostEvent::OpFinished(_) => "OpFinished",
        HostEvent::OpFailed(_) => "OpFailed",
        HostEvent::SessionStopped(_) => "SessionStopped",
    }
}

/// Check lifecycle ordering over the collected events.
///
/// The missing-`OpStarted` case is owned by the [`crate::invariants::started`]
/// invariant; ordering stays silent when no `OpStarted` exists so one root
/// cause produces one finding.
#[must_use]
pub fn check(events: &[HostEvent]) -> Vec<ConformanceFinding> {
    let mut findings = Vec::new();
    let op_started_index = events
        .iter()
        .position(|event| matches!(event, HostEvent::OpStarted(_)));

    if let Some(started) = op_started_index {
        for (index, event) in events.iter().enumerate() {
            if is_op_scoped(event) && index < started {
                findings.push(ConformanceFinding {
                    invariant: InvariantId::LifecycleOrdering,
                    message: format!(
                        "{} at index {index} must follow OpStarted at index {started}",
                        variant_name(event)
                    ),
                    evidence: vec![index, started],
                });
            }
        }
    }

    if let Some(terminal) = events.iter().position(is_terminal) {
        if terminal + 1 < events.len() {
            findings.push(ConformanceFinding {
                invariant: InvariantId::LifecycleOrdering,
                message: format!(
                    "terminal event at index {terminal} must be the last event; {} event(s) follow",
                    events.len() - terminal - 1
                ),
                evidence: vec![terminal, events.len() - 1],
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        message_delta, op_finished, op_started, plan_update, session_created, status,
        thought_delta, tool_call, tool_call_update,
    };
    use nexus_agent_host::capability::model::{FinishReason, StatusLevel};

    #[test]
    fn started_before_op_scoped_events_passes() {
        let events = vec![
            op_started(),
            thought_delta(),
            message_delta(),
            tool_call(),
            tool_call_update(),
            plan_update(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn session_created_before_started_is_allowed() {
        let events = vec![
            session_created(),
            op_started(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn status_before_started_is_allowed() {
        let events = vec![
            status(StatusLevel::Info),
            op_started(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn message_delta_before_started_is_a_finding() {
        let events = vec![
            message_delta(),
            op_started(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::LifecycleOrdering);
        assert_eq!(findings[0].evidence, vec![0, 1]);
    }

    #[test]
    fn tool_call_before_started_is_a_finding() {
        let events = vec![
            tool_call(),
            op_started(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].evidence, vec![0, 1]);
    }

    #[test]
    fn plan_update_before_started_is_a_finding() {
        let events = vec![
            plan_update(),
            op_started(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].evidence, vec![0, 1]);
    }

    #[test]
    fn events_after_terminal_are_a_finding() {
        let events = vec![
            op_started(),
            op_finished(FinishReason::EndTurn),
            message_delta(),
        ];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::LifecycleOrdering);
        assert_eq!(findings[0].evidence, vec![1, 2]);
    }

    #[test]
    fn no_started_does_not_double_report_ordering() {
        // The exactly-one-OpStarted invariant owns the missing-start case;
        // ordering stays silent to avoid duplicate findings for one root cause.
        let events = vec![message_delta(), op_finished(FinishReason::EndTurn)];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }
}
