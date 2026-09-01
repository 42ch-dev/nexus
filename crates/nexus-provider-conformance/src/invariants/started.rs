//! Invariant: exactly one `OpStarted` event per operation stream.
//!
//! The normalized operation lifecycle begins with `OpStarted`
//! (`OperationStartedEvent`). A stream with zero or multiple `OpStarted`
//! events violates the contract.

use nexus_agent_host::capability::model::HostEvent;

use crate::model::{ConformanceFinding, InvariantId};

/// Check that the stream contains exactly one `OpStarted` event.
///
/// Evidence: the indices of every `OpStarted` event (empty when none were
/// emitted — a missing event has no index to point at).
#[must_use]
pub fn check(events: &[HostEvent]) -> Vec<ConformanceFinding> {
    let indices: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event, HostEvent::OpStarted(_)))
        .map(|(index, _)| index)
        .collect();

    match indices.len() {
        0 => vec![ConformanceFinding {
            invariant: InvariantId::ExactlyOneStarted,
            message: "stream emitted no OpStarted event".to_string(),
            evidence: Vec::new(),
        }],
        1 => Vec::new(),
        count => vec![ConformanceFinding {
            invariant: InvariantId::ExactlyOneStarted,
            message: format!(
                "expected exactly one OpStarted event, found {count} at indices {indices:?}"
            ),
            evidence: indices,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{message_delta, op_finished, op_started};
    use nexus_agent_host::capability::model::FinishReason;

    #[test]
    fn exactly_one_started_passes() {
        let events = vec![
            op_started(),
            message_delta(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn missing_started_is_a_finding() {
        let events = vec![message_delta(), op_finished(FinishReason::EndTurn)];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::ExactlyOneStarted);
        assert!(
            findings[0].evidence.is_empty(),
            "a missing event has no index evidence: {:?}",
            findings[0].evidence
        );
    }

    #[test]
    fn duplicate_started_is_a_finding() {
        let events = vec![
            op_started(),
            op_started(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::ExactlyOneStarted);
        assert_eq!(findings[0].evidence, vec![0, 1]);
    }
}
