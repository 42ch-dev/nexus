//! Nexus WorldForkRequest
//!
//! Request body for POST /v1/worlds/fork — create a forked world from a parent at a timeline event (platform client contract).
//!
//! @schema_version 1
//! @source world-fork-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for POST /v1/worlds/fork — create a forked world from a parent at a timeline event (platform client contract).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldForkRequest {
    pub schema_version: u32,
    pub parent_world_id: String,
    pub child_world_id: String,
    pub forked_from_event_id: String,
    pub created_by_creator_id: String,
}
