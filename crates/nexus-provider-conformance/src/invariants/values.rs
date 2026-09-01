//! Invariant: forbidden-value exclusion.
//!
//! Normalized fields carry only contract values:
//!
//! - `OpFailed::error_category` is a closed five-token set: `decode_error` /
//!   `stream_closed` / `timeout` / `io_error` / `provider_error` (PD-3, AR-7).
//! - `OpFailed::error_message` is capped at 512 bytes by
//!   `truncate_error_message` (N-2) — the runner flags any longer message.
//! - Enum fields never leak vendor raw strings: `StatusLevel` is a closed
//!   three-token set (`Info` | `Warning` | `Error`); the wildcard arm is the
//!   drift gate for a new variant.

use nexus_agent_host::capability::model::{HostEvent, StatusLevel};

use crate::model::{ConformanceFinding, InvariantId};

/// The closed `error_category` token set (PD-3, AR-7).
const ERROR_CATEGORIES: [&str; 5] = [
    "decode_error",
    "stream_closed",
    "timeout",
    "io_error",
    "provider_error",
];

/// The `error_message` byte cap applied by `truncate_error_message` (N-2).
const MAX_ERROR_MESSAGE_BYTES: usize = 512;

/// Check forbidden-value exclusion over the collected events.
#[must_use]
pub fn check(events: &[HostEvent]) -> Vec<ConformanceFinding> {
    let mut findings = Vec::new();
    for (index, event) in events.iter().enumerate() {
        match event {
            HostEvent::OpFailed(failed) => {
                if !ERROR_CATEGORIES.contains(&failed.error_category.as_str()) {
                    findings.push(ConformanceFinding {
                        invariant: InvariantId::ForbiddenValueExclusion,
                        message: format!(
                            "OpFailed at index {index} carries error_category {:?} outside the closed set {ERROR_CATEGORIES:?}",
                            failed.error_category
                        ),
                        evidence: vec![index],
                    });
                }
                if failed.error_message.len() > MAX_ERROR_MESSAGE_BYTES {
                    findings.push(ConformanceFinding {
                        invariant: InvariantId::ForbiddenValueExclusion,
                        message: format!(
                            "OpFailed at index {index} carries an error_message of {} bytes, exceeding the {MAX_ERROR_MESSAGE_BYTES}-byte cap",
                            failed.error_message.len()
                        ),
                        evidence: vec![index],
                    });
                }
            }
            HostEvent::Status(status) => {
                #[allow(unreachable_patterns)] // drift gate: new variant must trip conformance
                match status.level {
                    StatusLevel::Info | StatusLevel::Warning | StatusLevel::Error => {}
                    _ => findings.push(ConformanceFinding {
                        invariant: InvariantId::ForbiddenValueExclusion,
                        message: format!(
                            "Status at index {index} carries a StatusLevel outside the closed contract set"
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
    use crate::test_support::{op_failed, status};
    use nexus_agent_host::capability::model::StatusLevel;

    #[test]
    fn contract_error_categories_pass() {
        let events = vec![
            op_failed("decode_error", "bad frame"),
            op_failed("stream_closed", "eof"),
            op_failed("timeout", "slow"),
            op_failed("io_error", "spawn"),
            op_failed("provider_error", "nope"),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn vendor_raw_error_category_is_a_finding() {
        let events = vec![op_failed("vendor_internal_error", "raw vendor token")];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::ForbiddenValueExclusion);
        assert_eq!(findings[0].evidence, vec![0]);
    }

    #[test]
    fn oversized_error_message_is_a_finding() {
        let long = "x".repeat(513);
        let events = vec![op_failed("provider_error", &long)];
        let findings = check(&events);
        assert_eq!(findings.len(), 1, "expected one finding: {findings:?}");
        assert_eq!(findings[0].invariant, InvariantId::ForbiddenValueExclusion);
        assert!(
            findings[0].message.contains("512"),
            "message: {}",
            findings[0].message
        );
    }

    #[test]
    fn error_message_at_cap_passes() {
        let at_cap = "x".repeat(512);
        let events = vec![op_failed("provider_error", &at_cap)];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }

    #[test]
    fn contract_status_levels_pass() {
        let events = vec![
            status(StatusLevel::Info),
            status(StatusLevel::Warning),
            status(StatusLevel::Error),
        ];
        let findings = check(&events);
        assert!(findings.is_empty(), "expected no findings: {findings:?}");
    }
}
