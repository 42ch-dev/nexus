//! Conformance report model: findings, invariant identifiers, and config.

use std::fmt;
use std::time::Duration;

/// Identifier for each conformance invariant checked by the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvariantId {
    /// Exactly one `OpStarted` event per operation stream.
    ExactlyOneStarted,
    /// Event count and wall-clock duration stay within the configured bounds.
    BoundedStream,
    /// `OpStarted` precedes op-scoped events; the terminal event is last.
    LifecycleOrdering,
    /// Exactly one terminal event (`OpFinished` | `OpFailed`), then the stream ends.
    SingleTerminal,
    /// `FinishReason` / `SessionStopReason` carry only closed-set values.
    StopReasonConsistency,
    /// Normalized fields carry only contract values (closed `error_category`,
    /// 512-byte `error_message` cap, closed enum fields).
    ForbiddenValueExclusion,
}

/// A single conformance finding: one invariant violation with event-index evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceFinding {
    /// The invariant that was violated.
    pub invariant: InvariantId,
    /// Human-readable description of the violation.
    pub message: String,
    /// Event indices that constitute the evidence for the violation.
    pub evidence: Vec<usize>,
}

/// Bounds for stream collection.
///
/// The runner stops collecting when either bound is hit and reports a
/// [`InvariantId::BoundedStream`] finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConformanceConfig {
    /// Maximum number of events collected before the stream is truncated.
    pub max_events: usize,
    /// Maximum wall-clock duration before the stream is truncated.
    pub max_duration: Duration,
}

impl Default for ConformanceConfig {
    fn default() -> Self {
        Self {
            max_events: 10_000,
            max_duration: Duration::from_secs(60),
        }
    }
}

/// Result of running conformance over one operation stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceReport {
    /// All findings across every invariant (empty = conformant).
    pub findings: Vec<ConformanceFinding>,
    /// Number of events collected before the stream ended or was truncated.
    pub event_count: usize,
    /// Wall-clock time spent collecting the stream.
    pub duration: Duration,
    /// Whether collection stopped early due to a bound (count or duration).
    pub truncated: bool,
}

impl ConformanceReport {
    /// Whether the stream conformed: no findings were produced.
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.findings.is_empty()
    }
}

impl fmt::Display for ConformanceReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "ConformanceReport: {} events in {:?} (truncated: {})",
            self.event_count, self.duration, self.truncated
        )?;
        if self.findings.is_empty() {
            writeln!(f, "  PASS — no findings")?;
        } else {
            for finding in &self.findings {
                writeln!(
                    f,
                    "  FAIL [{:?}] {} (evidence: {:?})",
                    finding.invariant, finding.message, finding.evidence
                )?;
            }
        }
        Ok(())
    }
}
