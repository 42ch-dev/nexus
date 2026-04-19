//! Nexus Sync Conflict Response
//!
//! Platform conflict response for bundle push operations. HTTP 200 with success:false indicates a conflict requiring resolution. See hard-vs-soft-validation-v1.md §7.
//!
//! @schema_version 1
//! @source conflict-response.schema.json

use serde::{Deserialize, Serialize};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConflictResponseConflict {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_hint: Option<String>,
}
/// Platform conflict response for bundle push operations. HTTP 200 with success:false indicates a conflict requiring resolution. See hard-vs-soft-validation-v1.md §7.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConflictResponse {
    pub success: bool,
    pub conflict_type: String,
    pub conflicts: Vec<ConflictResponseConflict>,
    pub server_world_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_delta_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<i64>,
}
