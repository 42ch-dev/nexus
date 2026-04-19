//! Nexus WorldSnapshotRequest
//!
//! Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor with optional branch and size limits (platform API).
//!
//! @schema_version 1
//! @source world-snapshot-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for POST /v1/worlds/snapshot — capture a read-only snapshot cursor with optional branch and size limits (platform API).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldSnapshotRequest {
    pub schema_version: u32,
    pub world_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_block_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_event_limit: Option<i64>,
}
