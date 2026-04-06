//! Nexus Sync Conflict Response
//!
//! Platform conflict response for bundle push operations. HTTP 200 with success:false indicates a conflict requiring resolution. See hard-vs-soft-validation-v1.md §7.
//!
//! @schema_version 1
//! @source conflict-response.schema.json

use serde::{Deserialize, Serialize};



/// Nexus Sync Conflict Response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ConflictResponse {
    pub success: bool,
    pub conflict_type: String,
    pub conflicts: Vec<serde_json::Value>,
    pub server_world_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_delta_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<i64>,
}
