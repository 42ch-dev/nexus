//! Invariant: single terminal event, then the stream ends.
//!
//! Every operation stream must emit exactly one terminal event
//! (`OpFinished` | `OpFailed`) and then end. A stream that ends without a
//! terminal, emits multiple terminals, or yields a stream-level `Err` item
//! violates the contract (the `ProviderAdapter::execute` doc: "The stream MUST
//! emit exactly one terminal event (`OpFinished` or `OpFailed`) before
//! ending").

use nexus_agent_host::capability::model::HostEvent;

use crate::model::{ConformanceFinding, InvariantId};

/// Whether the event is a terminal event.
const fn is_terminal(event: &HostEvent) -> bool {
    matches!(event, HostEvent::OpFinished(_) | HostEvent::OpFailed(_))
}

/// Check the single-terminal contract over the collected events.
///
/// `stream_error` carries the first stream-level `Err` item
/// `(event index, message)` observed by the bounded collector, if any.
#[must_use]
pub fn check(
    events: &[HostEvent],
    stream_error: Option<&(usize, String)>,
) -> Vec<ConformanceFinding> {
    let terminal_indices: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| is_terminal(event))
        .map(|(index, _)| index)
        .collect();

    match terminal_indices.len() {
        0 => {
            let message = match stream_error {
                Some((index, error)) => format!(
                    "stream yielded an error item at index {index} ({error}) and ended without a terminal event (OpFinished | OpFailed)"
                ),
                None => "stream ended without a terminal event (OpFinished | OpFailed)".to_string(),
            };
            vec![ConformanceFinding {
                invariant: InvariantId::SingleTerminal,
                message,
                evidence: stream_error.map(|(index, _)| vec![*index]).unwrap_or_default(),
            }]
        }
        1 => {
            if let Some((index, error)) = stream_error {
                vec![ConformanceFinding {
                    invariant: InvariantId::SingleTerminal,
                    message: format!(
                        "stream yielded an error item at index {index} ({error}) after the terminal event at index {}; the stream must end after the terminal",
                        terminal_indices[0]
                    ),
                    evidence: vec![terminal_indices[0], *index],
                }]
            } else {
                Vec::new()
            }
        }
        count => vec![ConformanceFinding {
            invariant: InvariantId::SingleTerminal,
            message: format!(
                "expected exactly one terminal event (OpFinished | OpFailed), found {count} at indices {terminal_indices:?}"
            ),
            evidence: terminal_indices,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{message_delta, op_failed, op_finished, op_started};
    use nexus_agent_host::capability::model::FinishReason;

    #[test]
    fn single_terminal_last_passes() {
        let events = vec![
            op_started(),
            message_delta(),
            op_finished(FinishReason::EndTurn),
        ];
        let findings = check(&events, None);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn missing_terminal_is_a_finding() {
        let events = vec![op_started(), message_delta()];
        let findings = check(&events, None);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::SingleTerminal);
        assert!(findings[0].evidence.is_empty());
    }

    #[test]
    fn two_terminals_is_a_finding() {
        let events = vec![
            op_started(),
            op_finished(FinishReason::EndTurn),
            op_failed("provider_error", "boom"),
        ];
        let findings = check(&events, None);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::SingleTerminal);
        assert_eq!(findings[0].evidence, vec![1, 2]);
    }

    #[test]
    fn stream_error_instead_of_terminal_is_a_finding() {
        let events = vec![op_started()];
        let findings = check(&events, Some(&(1, "transport died".to_string())));
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::SingleTerminal);
        assert_eq!(findings[0].evidence, vec![1]);
        assert!(
            findings[0].message.contains("transport died"),
            "message: {}",
            findings[0].message
        );
    }

    #[test]
    fn stream_error_after_terminal_is_a_finding() {
        let events = vec![op_started(), op_finished(FinishReason::EndTurn)];
        let findings = check(&events, Some(&(2, "late error".to_string())));
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].evidence, vec![1, 2]);
    }
}
