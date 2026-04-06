//! Conflict Resolution
//!
//! Handles sync conflicts detected by the platform during bundle push.
//! Implements conflict types from hard-vs-soft-validation-v1.md §7:
//! - `version_mismatch`: Client world_revision is stale
//! - `sequence_conflict`: Client delta_sequence has gaps
//! - `hard_validation_failure`: Schema/contract violations
//! - `soft_validation_warning`: Non-blocking validation warnings
//!
//! Resolution strategies:
//! - `AutoAccept`: Accept server state, discard local changes
//! - `AutoReject`: Keep local state, discard server changes
//! - `ManualReview`: Present conflict to user for decision

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{SyncError, SyncResult};

/// Conflict type categories matching hard-vs-soft-validation-v1.md §7.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictType {
    /// Client world_revision is stale compared to server.
    VersionMismatch,
    /// Client delta_sequence has gaps or overlaps.
    SequenceConflict,
    /// Hard validation failure — schema/contract violation (blocking).
    HardValidationFailure,
    /// Soft validation warning — non-blocking but noteworthy.
    SoftValidationWarning,
}

impl ConflictType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::VersionMismatch => "version_mismatch",
            Self::SequenceConflict => "sequence_conflict",
            Self::HardValidationFailure => "hard_validation_failure",
            Self::SoftValidationWarning => "soft_validation_warning",
        }
    }

    pub fn parse(s: &str) -> SyncResult<Self> {
        match s {
            "version_mismatch" => Ok(Self::VersionMismatch),
            "sequence_conflict" => Ok(Self::SequenceConflict),
            "hard_validation_failure" => Ok(Self::HardValidationFailure),
            "soft_validation_warning" => Ok(Self::SoftValidationWarning),
            other => Err(SyncError::UnresolvableConflict(format!(
                "unknown conflict type: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for ConflictType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolution strategy for a conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Accept server state, discard local changes (rebase/refresh).
    AutoAccept,
    /// Keep local state, force-push (risky).
    AutoReject,
    /// Present conflict to user for manual resolution.
    ManualReview,
}

impl ConflictResolution {
    pub fn as_str(&self) -> &str {
        match self {
            Self::AutoAccept => "auto_accept",
            Self::AutoReject => "auto_reject",
            Self::ManualReview => "manual_review",
        }
    }

    pub fn parse(s: &str) -> SyncResult<Self> {
        match s {
            "auto_accept" => Ok(Self::AutoAccept),
            "auto_reject" => Ok(Self::AutoReject),
            "manual_review" => Ok(Self::ManualReview),
            other => Err(SyncError::UnresolvableConflict(format!(
                "unknown resolution strategy: {other}"
            ))),
        }
    }

    /// Whether this resolution requires user interaction.
    pub fn requires_manual_review(&self) -> bool {
        matches!(self, Self::ManualReview)
    }
}

/// A single conflict detail within a conflict response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConflictDetail {
    /// Machine-readable conflict code.
    pub code: String,
    /// Human-readable conflict description.
    pub message: String,
    /// Index into the deltas[] array, if applicable.
    pub delta_index: Option<usize>,
    /// Expected value that caused the conflict.
    pub expected: Option<Value>,
    /// Actual value received.
    pub actual: Option<Value>,
    /// Suggested resolution strategy.
    pub resolution_hint: Option<ConflictResolution>,
}

/// Parsed conflict response from the platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConflictResponse {
    /// Always false for conflict responses.
    pub success: bool,
    /// Category of conflict.
    pub conflict_type: ConflictType,
    /// Individual conflict details.
    pub conflicts: Vec<ConflictDetail>,
    /// Current world revision on the server.
    pub server_world_revision: u64,
    /// Current confirmed delta sequence on the server.
    pub server_delta_sequence: Option<u64>,
    /// Suggested retry delay in seconds, if applicable.
    pub retry_after: Option<u64>,
}

impl ConflictResponse {
    /// Parse a conflict response from a JSON body.
    pub fn from_json(json_str: &str) -> SyncResult<Self> {
        let val: Value = serde_json::from_str(json_str)?;

        let success = val
            .get("success")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| {
                SyncError::Serialization(
                    "missing or invalid 'success' field in conflict response".into(),
                )
            })?;

        if success {
            return Err(SyncError::UnresolvableConflict(
                "response indicates success, not a conflict".to_string(),
            ));
        }

        let conflict_type_str = val
            .get("conflict_type")
            .and_then(|v| v.as_str())
            .unwrap_or("hard_validation_failure");
        let conflict_type = ConflictType::parse(conflict_type_str)?;

        let conflicts = val
            .get("conflicts")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(parse_conflict_detail).collect())
            .unwrap_or_default();

        let server_world_revision = val
            .get("server_world_revision")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let server_delta_sequence = val.get("server_delta_sequence").and_then(|v| v.as_u64());

        let retry_after = val.get("retry_after").and_then(|v| v.as_u64());

        Ok(Self {
            success: false,
            conflict_type,
            conflicts,
            server_world_revision,
            server_delta_sequence,
            retry_after,
        })
    }

    /// Whether this conflict is a hard failure (blocking).
    pub fn is_hard_failure(&self) -> bool {
        matches!(
            self.conflict_type,
            ConflictType::HardValidationFailure | ConflictType::VersionMismatch
        )
    }

    /// Whether this conflict is a soft warning (non-blocking).
    pub fn is_soft_warning(&self) -> bool {
        matches!(self.conflict_type, ConflictType::SoftValidationWarning)
    }

    /// Get the suggested resolution for this conflict.
    ///
    /// Defaults to `ManualReview` for hard failures, `AutoAccept` for soft warnings.
    pub fn suggested_resolution(&self) -> ConflictResolution {
        if self.is_soft_warning() {
            ConflictResolution::AutoAccept
        } else {
            ConflictResolution::ManualReview
        }
    }

    /// Build a human-readable conflict summary for user review.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!("Conflict: {}", self.conflict_type.as_str())];
        lines.push(format!(
            "Server world revision: {}",
            self.server_world_revision
        ));
        if let Some(seq) = self.server_delta_sequence {
            lines.push(format!("Server delta sequence: {seq}"));
        }
        lines.push(String::new());

        for (i, detail) in self.conflicts.iter().enumerate() {
            lines.push(format!("  [{}] {}", i + 1, detail.message));
            if let Some(ref delta_idx) = detail.delta_index {
                lines.push(format!("      Delta index: {delta_idx}"));
            }
            if let Some(ref hint) = detail.resolution_hint {
                lines.push(format!("      Suggested: {}", hint.as_str()));
            }
        }

        lines.join("\n")
    }
}

fn parse_conflict_detail(val: &Value) -> Option<ConflictDetail> {
    let code = val
        .get("code")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let message = val
        .get("message")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let delta_index = val
        .get("delta_index")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let expected = val.get("expected").cloned();
    let actual = val.get("actual").cloned();
    let resolution_hint = val
        .get("resolution_hint")
        .and_then(|v| v.as_str())
        .and_then(|s| ConflictResolution::parse(s).ok());

    Some(ConflictDetail {
        code,
        message,
        delta_index,
        expected,
        actual,
        resolution_hint,
    })
}

/// Conflict resolver that determines the appropriate resolution strategy.
pub struct ConflictResolver;

impl ConflictResolver {
    /// Determine the resolution strategy for a conflict response.
    pub fn resolve(conflict: &ConflictResponse) -> ConflictResolution {
        // Use per-conflict hints if all agree
        let hints: Vec<&ConflictResolution> = conflict
            .conflicts
            .iter()
            .filter_map(|c| c.resolution_hint.as_ref())
            .collect();

        if hints.is_empty() {
            return conflict.suggested_resolution();
        }

        // If all hints agree, use that; otherwise, manual review
        let first = hints[0];
        if hints.iter().all(|h| *h == first) {
            first.clone()
        } else {
            ConflictResolution::ManualReview
        }
    }

    /// Log a conflict for user review.
    ///
    /// Returns a formatted log entry string.
    pub fn log_conflict(conflict: &ConflictResponse) -> String {
        let resolution = Self::resolve(conflict);
        let mut log = format!(
            "[CONFLICT] type={} resolution={}\n",
            conflict.conflict_type.as_str(),
            resolution.as_str()
        );
        log.push_str(&conflict.summary());

        tracing::warn!(
            conflict_type = %conflict.conflict_type,
            resolution = %resolution.as_str(),
            conflict_count = conflict.conflicts.len(),
            "Sync conflict detected"
        );

        log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_type_roundtrip() {
        assert_eq!(
            ConflictType::parse("version_mismatch").unwrap(),
            ConflictType::VersionMismatch
        );
        assert_eq!(
            ConflictType::parse("sequence_conflict").unwrap(),
            ConflictType::SequenceConflict
        );
        assert!(ConflictType::parse("bogus").is_err());
    }

    #[test]
    fn resolution_roundtrip() {
        assert_eq!(
            ConflictResolution::parse("auto_accept").unwrap(),
            ConflictResolution::AutoAccept
        );
        assert_eq!(
            ConflictResolution::parse("manual_review").unwrap(),
            ConflictResolution::ManualReview
        );
        assert!(ConflictResolution::parse("bogus").is_err());
    }

    fn sample_conflict_json() -> &'static str {
        r#"{
            "success": false,
            "conflict_type": "version_mismatch",
            "conflicts": [
                {
                    "code": "revision_outdated",
                    "message": "Client world_revision 3 is behind server revision 5",
                    "expected": 5,
                    "actual": 3,
                    "resolution_hint": "auto_accept"
                }
            ],
            "server_world_revision": 5,
            "server_delta_sequence": 12,
            "retry_after": null
        }"#
    }

    #[test]
    fn parse_conflict_response() {
        let response = ConflictResponse::from_json(sample_conflict_json()).expect("parse conflict");

        assert!(!response.success);
        assert_eq!(response.conflict_type, ConflictType::VersionMismatch);
        assert_eq!(response.conflicts.len(), 1);
        assert_eq!(response.server_world_revision, 5);
        assert_eq!(response.server_delta_sequence, Some(12));
    }

    #[test]
    fn parse_soft_warning_conflict() {
        let json = r#"{
            "success": false,
            "conflict_type": "soft_validation_warning",
            "conflicts": [
                {
                    "code": "optional_field_missing",
                    "message": "manuscript_phase not set",
                    "resolution_hint": "auto_accept"
                }
            ],
            "server_world_revision": 5
        }"#;

        let response = ConflictResponse::from_json(json).expect("parse");
        assert_eq!(response.conflict_type, ConflictType::SoftValidationWarning);
        assert!(response.is_soft_warning());
        assert!(!response.is_hard_failure());
    }

    #[test]
    fn parse_success_response_fails() {
        let json = r#"{"success": true, "conflict_type": "version_mismatch", "conflicts": [], "server_world_revision": 5}"#;
        let result = ConflictResponse::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn resolver_auto_accept_for_soft_warning() {
        let response = ConflictResponse {
            success: false,
            conflict_type: ConflictType::SoftValidationWarning,
            conflicts: vec![ConflictDetail {
                code: "warning".to_string(),
                message: "test".to_string(),
                delta_index: None,
                expected: None,
                actual: None,
                resolution_hint: None,
            }],
            server_world_revision: 1,
            server_delta_sequence: None,
            retry_after: None,
        };

        let resolution = ConflictResolver::resolve(&response);
        assert_eq!(resolution, ConflictResolution::AutoAccept);
    }

    #[test]
    fn resolver_manual_review_for_hard_failure() {
        let response = ConflictResponse {
            success: false,
            conflict_type: ConflictType::HardValidationFailure,
            conflicts: vec![ConflictDetail {
                code: "schema_violation".to_string(),
                message: "invalid field".to_string(),
                delta_index: Some(0),
                expected: None,
                actual: None,
                resolution_hint: None,
            }],
            server_world_revision: 1,
            server_delta_sequence: None,
            retry_after: None,
        };

        let resolution = ConflictResolver::resolve(&response);
        assert_eq!(resolution, ConflictResolution::ManualReview);
    }

    #[test]
    fn resolver_uses_hint_when_present() {
        let response = ConflictResponse {
            success: false,
            conflict_type: ConflictType::VersionMismatch,
            conflicts: vec![ConflictDetail {
                code: "revision_outdated".to_string(),
                message: "stale revision".to_string(),
                delta_index: None,
                expected: None,
                actual: None,
                resolution_hint: Some(ConflictResolution::AutoAccept),
            }],
            server_world_revision: 5,
            server_delta_sequence: None,
            retry_after: None,
        };

        let resolution = ConflictResolver::resolve(&response);
        assert_eq!(resolution, ConflictResolution::AutoAccept);
    }

    #[test]
    fn resolver_manual_review_for_mixed_hints() {
        let response = ConflictResponse {
            success: false,
            conflict_type: ConflictType::HardValidationFailure,
            conflicts: vec![
                ConflictDetail {
                    code: "a".to_string(),
                    message: "test".to_string(),
                    delta_index: None,
                    expected: None,
                    actual: None,
                    resolution_hint: Some(ConflictResolution::AutoAccept),
                },
                ConflictDetail {
                    code: "b".to_string(),
                    message: "test".to_string(),
                    delta_index: None,
                    expected: None,
                    actual: None,
                    resolution_hint: Some(ConflictResolution::AutoReject),
                },
            ],
            server_world_revision: 1,
            server_delta_sequence: None,
            retry_after: None,
        };

        let resolution = ConflictResolver::resolve(&response);
        assert_eq!(resolution, ConflictResolution::ManualReview);
    }

    #[test]
    fn conflict_summary_output() {
        let response = ConflictResponse::from_json(sample_conflict_json()).expect("parse conflict");
        let summary = response.summary();
        assert!(summary.contains("version_mismatch"));
        assert!(summary.contains("Server world revision: 5"));
        assert!(summary.contains("Client world_revision 3"));
    }

    #[test]
    fn conflict_serialization_roundtrip() {
        let detail = ConflictDetail {
            code: "test".to_string(),
            message: "test message".to_string(),
            delta_index: Some(3),
            expected: Some(serde_json::json!(5)),
            actual: Some(serde_json::json!(3)),
            resolution_hint: Some(ConflictResolution::ManualReview),
        };

        let json = serde_json::to_string(&detail).expect("serialize");
        let recovered: ConflictDetail = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(detail, recovered);
    }
}
