//! Nexus MemoryWebListRequest
//!
//! Request body for memory web read — list / filter MemoryItem rows for a world (platform plan 18). Aligns with domain memory.schema.json field semantics.
//!
//! @schema_version 1
//! @source memory-web-list-request.schema.json

use crate::generated::common_types::{MemoryKind, MemoryStatus, MemoryType};
use serde::{Deserialize, Serialize};

/// Request body for memory web read — list / filter MemoryItem rows for a world (platform plan 18). Aligns with domain memory.schema.json field semantics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryWebListRequest {
    pub schema_version: u32,
    pub world_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_types: Option<Vec<MemoryType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_kinds: Option<Vec<MemoryKind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statuses: Option<Vec<MemoryStatus>>,
}
