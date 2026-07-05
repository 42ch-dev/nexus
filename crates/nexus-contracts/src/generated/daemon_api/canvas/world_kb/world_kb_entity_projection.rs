//! `Nexus` `WorldKbEntityProjection`
//!
//! `Flat` wire projection of a `World` `KB` `KeyBlock` entity for canvas graph + inspector surfaces (`V1`.73). `version` maps to the `SQLite` per-row `OCC` column (`kb_key_blocks`.revision, `NULL`-normalized to 0).
//!
//! `@schema_version` 1
//! `@source` world-kb-entity-projection.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common::common_types::{BlockType};

/// `Flat` wire projection of a `World` `KB` `KeyBlock` entity for canvas graph + inspector surfaces (`V1`.73). `version` maps to the `SQLite` per-row `OCC` column (`kb_key_blocks`.revision, `NULL`-normalized to 0).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbEntityProjection {
    pub key_block_id: String,
    pub world_id: String,
    pub block_type: BlockType,
    pub canonical_name: String,
    pub status: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
