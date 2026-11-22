//! Conformance report model: findings, invariant identifiers, and config.
// note: these report types are shared by every invariant check.

use std::fmt;
use std::time::Duration;

/// note: identifier for each conformance invariant checked by the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvariantId {
    /// note: exactly one `OpStarted` event per operation stream.
    ExactlyOneStarted,
    /// note: event count and wall-clock duration stay within the configured bounds.
    BoundedStream,
    /// note: `OpStarted` precedes op-scoped events; the terminal event is last.
    LifecycleOrdering,
    /// note: exactly one terminal event (`OpFinished` | `OpFailed`), then the stream ends.
    SingleTerminal,
    /// note: `FinishReason` / `SessionStopReason` carry only closed-set values.
    StopReasonConsistency,
    /// note: normalized fields carry only contract values (closed `error_category`,
    /// 512-byte `error_message` cap, closed enum fields).
    ForbiddenValueExclusion,
}

/// note: a single conformance finding — one invariant violation with event-index evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceFinding {
    /// note: the invariant that was violated.
    pub invariant: InvariantId,
    /// note: human-readable description of the violation.
    pub message: String,
    /// note: event indices that constitute the evidence for the violation.
    pub evidence: Vec<usize>,
}

/// note: bounds for stream collection.
///
/// The runner stops collecting when either bound is hit and reports a
/// [`InvariantId::BoundedStream`] finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConformanceConfig {
    /// note: maximum number of events collected before the stream is truncated.
    pub max_events: usize,
    /// note: maximum wall-clock duration before the stream is truncated.
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

/// note: result of running conformance over one operation stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceReport {
    /// note: all findings across every invariant (empty = conformant).
    pub findings: Vec<ConformanceFinding>,
    /// note: number of events collected before the stream ended or was truncated.
    pub event_count: usize,
    /// note: wall-clock time spent collecting the stream.
    pub duration: Duration,
    /// note: whether collection stopped early due to a bound (count or duration).
    pub truncated: bool,
}

impl ConformanceReport {
    /// note: whether the stream conformed — no findings were produced.
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
