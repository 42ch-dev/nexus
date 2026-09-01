//! Invariant: bounded event count / duration.
//!
//! The runner collects the stream under the [`ConformanceConfig`] bounds. If
//! the event count reaches `max_events` or the wall-clock duration exceeds
//! `max_duration`, collection stops early and a [`InvariantId::BoundedStream`]
//! finding is recorded. A stream that ends within the bounds is conformant.

use std::time::Instant;

use futures_util::StreamExt;
use nexus_agent_host::capability::model::{HostEvent, HostEventStream};

use crate::model::{ConformanceConfig, ConformanceFinding, InvariantId};

/// The outcome of bounded stream collection.
#[derive(Debug)]
pub struct CollectedStream {
    /// Events collected before the stream ended or was truncated.
    pub events: Vec<HostEvent>,
    /// Bounded-stream findings (empty when the stream ended within bounds).
    pub findings: Vec<ConformanceFinding>,
    /// Whether collection stopped early due to a bound.
    pub truncated: bool,
    /// The first stream-level error item, if any: `(event index, message)`.
    pub stream_error: Option<(usize, String)>,
}

/// Collect the stream under the configured bounds.
///
/// Stops on stream end, on a bound hit, or on the first `Err` item. A bound
/// hit records a [`InvariantId::BoundedStream`] finding; an `Err` item is
/// surfaced via [`CollectedStream::stream_error`] for the terminal invariant.
pub async fn collect(stream: HostEventStream, config: ConformanceConfig) -> CollectedStream {
    let started_at = Instant::now();
    let mut events: Vec<HostEvent> = Vec::new();
    let mut findings: Vec<ConformanceFinding> = Vec::new();
    let mut truncated = false;
    let mut stream_error: Option<(usize, String)> = None;

    let mut stream = stream;
    loop {
        if events.len() >= config.max_events {
            findings.push(ConformanceFinding {
                invariant: InvariantId::BoundedStream,
                message: format!(
                    "event count {} reached the bound of {}",
                    events.len(),
                    config.max_events
                ),
                evidence: vec![events.len().saturating_sub(1)],
            });
            truncated = true;
            break;
        }
        let remaining = config.max_duration.saturating_sub(started_at.elapsed());
        match tokio::time::timeout(remaining, stream.next()).await {
            Err(_elapsed) => {
                findings.push(ConformanceFinding {
                    invariant: InvariantId::BoundedStream,
                    message: format!(
                        "stream exceeded the duration bound of {:?}",
                        config.max_duration
                    ),
                    evidence: vec![events.len().saturating_sub(1)],
                });
                truncated = true;
                break;
            }
            Ok(None) => break,
            Ok(Some(Ok(event))) => events.push(event),
            Ok(Some(Err(error))) => {
                stream_error = Some((events.len(), error.to_string()));
                break;
            }
        }
    }

    CollectedStream {
        events,
        findings,
        truncated,
        stream_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::op_started;
    use nexus_agent_host::HostError;
    use std::time::Duration;

    fn stream_of(events: Vec<HostEvent>) -> HostEventStream {
        futures_util::stream::iter(events.into_iter().map(Ok)).boxed()
    }

    #[tokio::test]
    async fn collects_until_stream_ends() {
        let events = vec![op_started()];
        let collected = collect(stream_of(events), ConformanceConfig::default()).await;
        assert_eq!(collected.events.len(), 1);
        assert!(!collected.truncated, "stream ended within bounds");
        assert!(
            collected.findings.is_empty(),
            "expected no findings: {:?}",
            collected.findings
        );
        assert!(collected.stream_error.is_none());
    }

    #[tokio::test]
    async fn event_count_bound_truncates() {
        let events = vec![op_started(), op_started(), op_started(), op_started()];
        let config = ConformanceConfig {
            max_events: 3,
            ..ConformanceConfig::default()
        };
        let collected = collect(stream_of(events), config).await;
        assert_eq!(collected.events.len(), 3, "collection stops at the bound");
        assert!(collected.truncated);
        assert_eq!(
            collected.findings.len(),
            1,
            "expected one finding: {:?}",
            collected.findings
        );
        assert_eq!(collected.findings[0].invariant, InvariantId::BoundedStream);
    }

    #[tokio::test]
    async fn duration_bound_truncates() {
        let stream = futures_util::stream::once(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok(op_started())
        })
        .boxed();
        let config = ConformanceConfig {
            max_duration: Duration::from_millis(10),
            ..ConformanceConfig::default()
        };
        let collected = collect(stream, config).await;
        assert!(collected.truncated);
        assert_eq!(
            collected.findings.len(),
            1,
            "expected one finding: {:?}",
            collected.findings
        );
        assert_eq!(collected.findings[0].invariant, InvariantId::BoundedStream);
    }

    #[tokio::test]
    async fn stream_error_is_recorded() {
        let stream = futures_util::stream::iter(vec![
            Ok(op_started()),
            Err(HostError::ProviderUnavailable {
                provider_id: "mock".to_string().into(),
                message: "bad frame".to_string(),
            }),
        ])
        .boxed();
        let collected = collect(stream, ConformanceConfig::default()).await;
        assert_eq!(collected.events.len(), 1);
        let (index, message) = collected.stream_error.expect("stream error recorded");
        assert_eq!(index, 1);
        assert!(message.contains("bad frame"), "message: {message}");
    }
}
