//! Nexus KeyBlock
//!
//! KeyBlock - a structured knowledge unit in a world timeline. Aligned with data-model-v1.md §5.5.
//!
//! @schema_version 1
//! @source key-block.schema.json

use crate::generated::common_types::{BlockType, KeyBlockStatus, SourceAnchor};
use serde::{Deserialize, Serialize};

/// KeyBlock - a structured knowledge unit in a world timeline. Aligned with data-model-v1.md §5.5.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct KeyBlock {
    pub schema_version: u32,
    pub key_block_id: String,
    pub world_id: String,
    pub block_type: BlockType,
    pub canonical_name: String,
    pub status: KeyBlockStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<SourceAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_from_command_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
