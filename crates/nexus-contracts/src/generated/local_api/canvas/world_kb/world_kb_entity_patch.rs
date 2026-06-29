//! `Nexus` `WorldKbEntityPatch`
//!
//! `Field` set for `world_kb`.`patch_entity` (`V1`.73). `title` maps to `kb_key_blocks`.`canonical_name`; `body` to `body_json`; `block_type` re-classifies the entity (entity-scope-model §5.1.1). `At` least one property must be provided.
//!
//! `@schema_version` 1
//! `@source` world-kb-entity-patch.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common::common_types::{BlockType};

/// `Field` set for `world_kb`.`patch_entity` (`V1`.73). `title` maps to `kb_key_blocks`.`canonical_name`; `body` to `body_json`; `block_type` re-classifies the entity (entity-scope-model §5.1.1). `At` least one property must be provided.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbEntityPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_type: Option<BlockType>,
}
