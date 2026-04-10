//! Nexus WorldSnapshotRequest
//!
//! Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor for a world (platform client contract).
//!
//! @schema_version 1
//! @source world-snapshot-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor for a world (platform client contract).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldSnapshotRequest {
    pub schema_version: u32,
    pub world_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_event_id: Option<String>,
}
