//! Nexus WorldForkRequest
//!
//! Request body for POST /v1/worlds/fork — platform may derive parent world from URL, child world server-side, and creator from auth; body carries fork point and optional title.
//!
//! @schema_version 1
//! @source world-fork-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for POST /v1/worlds/fork — platform may derive parent world from URL, child world server-side, and creator from auth; body carries fork point and optional title.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldForkRequest {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_creator_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_title: Option<String>,
}
