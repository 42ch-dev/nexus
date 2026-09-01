//! Invariant: stop-reason consistency.
//!
//! `OpFinished` must carry a `FinishReason` from the closed contract set
//! (`EndTurn` | `MaxTokens` | `MaxTurnRequests` | `Refusal`) and
//! `SessionStopped` must carry a `SessionStopReason` from the closed contract
//! set (`GracefulShutdown` | `ProviderExit` | `Error` | `Cancelled`).
//!
//! The wildcard arm of each match is the drift gate: because the enums are
//! closed Rust types, a value outside the set can only appear if the model
//! gains a new variant (vendor drift). The runner then fails conformance with
//! a typed finding instead of passing silently. The current closed sets are
//! asserted by the tests; the drift arm is not directly constructible from
//! today's enums.

use nexus_agent_host::capability::model::{FinishReason, HostEvent, SessionStopReason};

use crate::model::{ConformanceFinding, InvariantId};

/// Check stop-reason consistency over the collected events.
#[must_use]
pub fn check(events: &[HostEvent]) -> Vec<ConformanceFinding> {
    let mut findings = Vec::new();
    for (index, event) in events.iter().enumerate() {
        match event {
            HostEvent::OpFinished(finished) => {
                // Closed contract set; the wildcard arm is the drift gate —
                // a new FinishReason variant fails conformance.
                #[allow(unreachable_patterns)] // drift gate: new variant must trip conformance
                match finished.reason {
                    FinishReason::EndTurn
                    | FinishReason::MaxTokens
                    | FinishReason::MaxTurnRequests
                    | FinishReason::Refusal => {}
                    _ => findings.push(ConformanceFinding {
                        invariant: InvariantId::StopReasonConsistency,
                        message: format!(
                            "OpFinished at index {index} carries a FinishReason outside the closed contract set"
                        ),
                        evidence: vec![index],
                    }),
                }
            }
            HostEvent::SessionStopped(stopped) => {
                #[allow(unreachable_patterns)] // drift gate: new variant must trip conformance
                match stopped.reason {
                    SessionStopReason::GracefulShutdown
                    | SessionStopReason::ProviderExit
                    | SessionStopReason::Error
                    | SessionStopReason::Cancelled => {}
                    _ => findings.push(ConformanceFinding {
                        invariant: InvariantId::StopReasonConsistency,
                        message: format!(
                            "SessionStopped at index {index} carries a SessionStopReason outside the closed contract set"
                        ),
                        evidence: vec![index],
                    }),
                }
            }
            _ => {}
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{op_finished, op_started, session_stopped};
    use nexus_agent_host::capability::model::{FinishReason, SessionStopReason};

    #[test]
    fn contract_finish_reasons_pass() {
        let events = vec![
            op_started(),
            op_finished(FinishReason::EndTurn),
            op_finished(FinishReason::MaxTokens),
            op_finished(FinishReason::MaxTurnRequests),
            op_finished(FinishReason::Refusal),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn contract_session_stop_reasons_pass() {
        let events = vec![
            session_stopped(SessionStopReason::GracefulShutdown),
            session_stopped(SessionStopReason::ProviderExit),
            session_stopped(SessionStopReason::Error),
            session_stopped(SessionStopReason::Cancelled),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn non_stop_events_are_ignored() {
        let events = vec![op_started()];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }
}
