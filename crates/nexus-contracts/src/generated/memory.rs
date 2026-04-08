//! Nexus MemoryItem
//!
//! MemoryItem - structured memory for creator experience and world context. Aligned with data-model-v1.md §5.8.
//!
//! @schema_version 1
//! @source memory.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{MemoryKind, MemoryStatus, MemoryType};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MemorySourceRef {
    pub kind: String,
    pub id: String,
}
/// MemoryItem - structured memory for creator experience and world context. Aligned with data-model-v1.md §5.8.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Memory {
    pub schema_version: u32,
    pub memory_item_id: String,
    pub creator_id: String,
    pub world_id: String,
    pub memory_type: MemoryType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_kind: Option<MemoryKind>,
    pub status: MemoryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_refs: Option<Vec<MemorySourceRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reinforced_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
