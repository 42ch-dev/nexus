//! Nexus WorldSnapshotResponse
//!
//! Response body for POST /v1/worlds/snapshot — snapshot anchor and revision metadata.
//!
//! @schema_version 1
//! @source world-snapshot-response.schema.json

use serde::{Deserialize, Serialize};

/// Response body for POST /v1/worlds/snapshot — snapshot anchor and revision metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldSnapshotResponse {
    pub schema_version: u32,
    pub world_id: String,
    pub world_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_event_id: Option<String>,
    pub captured_at: String,
}
