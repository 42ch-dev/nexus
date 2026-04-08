//! Partial Apply Semantics
//!
//! Handles Phase A/B partial success per roadmap §3.1.4 (P1).
//!
//! When the platform returns `bundle_apply_status: "partial"`:
//! - Phase A (data write) succeeded for some deltas
//! - Phase B (projection/indexing) may have failed for others
//!
//! This module parses partial apply results, stores retry state,
//! and exposes data freshness hints to the caller (CLI).

use serde::{Deserialize, Serialize};

use crate::errors::{SyncError, SyncResult};
use crate::sync_client::PushResponse;

/// Result of a partial bundle apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PartialApplyResult {
    /// Total number of deltas in the bundle.
    pub total_count: usize,
    /// Number of deltas that were successfully applied.
    pub succeeded_count: usize,
    /// Number of deltas that failed.
    pub failed_count: usize,
    /// Details of succeeded deltas (indices and entity revisions).
    pub succeeded_deltas: Vec<DeltaApplyInfo>,
    /// Details of failed deltas (indices and error codes).
    pub failed_deltas: Vec<DeltaApplyInfo>,
    /// Whether the failed deltas can be retried.
    pub retryable: bool,
    /// Server-side data freshness hint.
    pub data_freshness_hint: Option<String>,
    /// Last indexed bundle ID on the server.
    pub last_indexed_bundle_id: Option<String>,
}

/// Per-delta apply information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeltaApplyInfo {
    /// Index into the original deltas[] array.
    pub delta_index: usize,
    /// Apply status: applied, rejected, or skipped_dependency.
    pub apply_status: String,
    /// Error code if the delta was rejected.
    pub error_code: Option<String>,
    /// Entity revision after successful apply.
    pub applied_entity_revision: Option<u64>,
}

impl PartialApplyResult {
    /// Parse a partial apply result from a push response.
    pub fn from_push_response(push_response: &PushResponse) -> SyncResult<Self> {
        let delta_results = push_response.delta_results.as_ref().ok_or_else(|| {
            SyncError::PartialApplyStateError(
                "partial apply response missing delta_results".to_string(),
            )
        })?;

        let mut succeeded_deltas = Vec::new();
        let mut failed_deltas = Vec::new();

        for result in delta_results {
            let delta_index = result
                .get("delta_index")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(0);

            let apply_status = result
                .get("delta_apply_status")
                .and_then(|v| v.as_str())
                .unwrap_or("skipped_dependency")
                .to_string();

            let error_code = result
                .get("error_code")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let applied_entity_revision = result
                .get("applied_entity_revision")
                .and_then(|v| v.as_u64());

            let info = DeltaApplyInfo {
                delta_index,
                apply_status: apply_status.clone(),
                error_code: error_code.clone(),
                applied_entity_revision,
            };

            match apply_status.as_str() {
                "applied" => succeeded_deltas.push(info),
                "rejected" => failed_deltas.push(info),
                "skipped_dependency" => {
                    // Skipped due to dependency failure — treat as failed
                    failed_deltas.push(info);
                }
                _ => {
                    failed_deltas.push(info);
                }
            }
        }

        let succeeded_count = succeeded_deltas.len();
        let failed_count = failed_deltas.len();
        let total_count = succeeded_count + failed_count;

        // Determine if the failed deltas are retryable.
        // Deltas with dependency skips are retryable once their dependencies are resolved.
        let retryable = failed_deltas.iter().all(|d| {
            d.apply_status == "skipped_dependency"
                || d.error_code
                    .as_ref()
                    .map(|c| RETRYABLE_ERROR_CODES.contains(&c.as_str()))
                    .unwrap_or(false)
        });

        tracing::info!(
            total = total_count,
            succeeded = succeeded_count,
            failed = failed_count,
            retryable = retryable,
            "Partial apply result parsed"
        );

        Ok(Self {
            total_count,
            succeeded_count,
            failed_count,
            succeeded_deltas,
            failed_deltas,
            retryable,
            data_freshness_hint: push_response.data_freshness_hint.clone(),
            last_indexed_bundle_id: push_response.last_indexed_bundle_id.clone(),
        })
    }

    /// Whether all deltas were applied (not a partial result).
    pub fn is_full_success(&self) -> bool {
        self.failed_count == 0
    }

    /// Whether all deltas failed (total failure).
    pub fn is_total_failure(&self) -> bool {
        self.succeeded_count == 0 && self.failed_count > 0
    }

    /// Get indices of failed deltas for retry.
    pub fn failed_delta_indices(&self) -> Vec<usize> {
        self.failed_deltas.iter().map(|d| d.delta_index).collect()
    }

    /// Get a summary string for logging.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!(
            "Partial apply: {}/{} deltas succeeded",
            self.succeeded_count, self.total_count
        )];

        for info in &self.failed_deltas {
            let error = info.error_code.as_deref().unwrap_or("unknown");
            lines.push(format!(
                "  Delta[{}] failed: {} ({})",
                info.delta_index, info.apply_status, error
            ));
        }

        if self.retryable {
            lines.push("  Status: retryable".to_string());
        }

        lines.join("\n")
    }
}

/// Error codes that are considered retryable.
const RETRYABLE_ERROR_CODES: &[&str] = &[
    "dependency_not_found",
    "optimistic_lock_failed",
    "transient_validation_error",
    "indexing_timeout",
    "projection_failed",
];

/// Stored partial apply state for retry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PartialApplyState {
    /// Bundle ID that had the partial apply.
    pub bundle_id: String,
    /// World ID.
    pub world_id: String,
    /// The partial apply result.
    pub result: PartialApplyResult,
    /// When this state was recorded.
    pub recorded_at: String,
    /// Number of times we've retried this partial apply.
    pub retry_count: u32,
}

impl PartialApplyState {
    /// Create a new partial apply state record.
    pub fn new(bundle_id: &str, world_id: &str, result: PartialApplyResult) -> Self {
        Self {
            bundle_id: bundle_id.to_string(),
            world_id: world_id.to_string(),
            result,
            recorded_at: chrono::Utc::now().to_rfc3339(),
            retry_count: 0,
        }
    }

    /// Increment the retry count.
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
        self.recorded_at = chrono::Utc::now().to_rfc3339();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_delta_result(index: u64, status: &str, error_code: Option<&str>) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "delta_index": index,
            "delta_apply_status": status,
        });
        if let Some(code) = error_code {
            obj["error_code"] = serde_json::json!(code);
        }
        if status == "applied" {
            obj["applied_entity_revision"] = serde_json::json!(index + 10);
        }
        obj
    }

    fn push_response_with_deltas(
        apply_status: &str,
        deltas: Vec<serde_json::Value>,
    ) -> PushResponse {
        PushResponse {
            success: apply_status == "all_success",
            bundle_apply_status: Some(apply_status.to_string()),
            world_revision: Some(5),
            confirmed_delta_sequence: Some(10),
            delta_results: Some(deltas),
            data_freshness_hint: Some("hint".to_string()),
            last_indexed_bundle_id: Some("bdl_prev".to_string()),
        }
    }

    #[test]
    fn partial_apply_parse_mixed_results() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "rejected", Some("validation_error")),
            make_delta_result(2, "applied", None),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        assert_eq!(result.total_count, 3);
        assert_eq!(result.succeeded_count, 2);
        assert_eq!(result.failed_count, 1);
        assert!(!result.retryable);
        assert_eq!(result.failed_delta_indices(), vec![1]);
        assert_eq!(result.data_freshness_hint, Some("hint".to_string()));
    }

    #[test]
    fn partial_apply_retryable_dependency_skip() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "skipped_dependency", None),
            make_delta_result(2, "skipped_dependency", None),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        assert_eq!(result.succeeded_count, 1);
        assert_eq!(result.failed_count, 2);
        assert!(result.retryable);
    }

    #[test]
    fn partial_apply_retryable_transient_error() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "rejected", Some("optimistic_lock_failed")),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        assert!(result.retryable);
    }

    #[test]
    fn partial_apply_non_retryable_hard_error() {
        let deltas = vec![make_delta_result(0, "rejected", Some("schema_violation"))];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        assert!(!result.retryable);
    }

    #[test]
    fn partial_apply_is_full_success() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "applied", None),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        assert!(result.is_full_success());
    }

    #[test]
    fn partial_apply_is_total_failure() {
        let deltas = vec![make_delta_result(0, "rejected", Some("validation_error"))];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        assert!(result.is_total_failure());
    }

    #[test]
    fn partial_apply_missing_delta_results_errors() {
        let push_response = PushResponse {
            success: false,
            bundle_apply_status: Some("partial".to_string()),
            world_revision: None,
            confirmed_delta_sequence: None,
            delta_results: None,
            data_freshness_hint: None,
            last_indexed_bundle_id: None,
        };

        let result = PartialApplyResult::from_push_response(&push_response);
        assert!(matches!(
            result,
            Err(SyncError::PartialApplyStateError { .. })
        ));
    }

    #[test]
    fn partial_apply_summary() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "rejected", Some("validation_error")),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");
        let summary = result.summary();

        assert!(summary.contains("1/2 deltas succeeded"));
        assert!(summary.contains("Delta[1] failed"));
        assert!(summary.contains("validation_error"));
    }

    #[test]
    fn partial_apply_state_lifecycle() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "rejected", Some("transient")),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let partial_result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        let mut state = PartialApplyState::new("bdl_test", "wld_test", partial_result);
        assert_eq!(state.retry_count, 0);

        state.increment_retry();
        assert_eq!(state.retry_count, 1);
    }

    #[test]
    fn partial_apply_serialization_roundtrip() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "rejected", Some("error")),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        let json = serde_json::to_string(&result).expect("serialize");
        let recovered: PartialApplyResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(result, recovered);
    }

    #[test]
    fn partial_apply_state_serialization_roundtrip() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "skipped_dependency", None),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let partial_result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        let mut state = PartialApplyState::new("bdl_test", "wld_test", partial_result);
        state.increment_retry();
        state.increment_retry();

        let json = serde_json::to_string(&state).expect("serialize");
        let recovered: PartialApplyState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(state.bundle_id, recovered.bundle_id);
        assert_eq!(state.world_id, recovered.world_id);
        assert_eq!(state.retry_count, 2);
        assert_eq!(state.result, recovered.result);
    }

    #[test]
    fn partial_apply_state_retryable_deltas_indices() {
        let deltas = vec![
            make_delta_result(0, "applied", None),
            make_delta_result(1, "rejected", Some("optimistic_lock_failed")),
            make_delta_result(2, "skipped_dependency", None),
        ];

        let push_response = push_response_with_deltas("partial", deltas);
        let result = PartialApplyResult::from_push_response(&push_response).expect("parse");

        assert!(result.retryable);
        assert_eq!(result.failed_delta_indices(), vec![1, 2]);
    }
}
