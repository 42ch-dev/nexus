//! `Nexus` `Entity` `Attributes` (`Static` `Compute` `Params`)
//!
//! `Per`-`BlockType` static attribute schemas for computable `KeyBlocks` (`V1`.61 `KB` structured layer, compass `Q4`). `Attributes` are `IMMUTABLE` compute parameters (e.g. `max_hp`, `base_atk`) stored inside a `KeyBlock` body. `The` character shape is fully specified; other block types are permissive placeholders (additionalProperties: true) to be tightened as compute modules are added. `Note`: 'environment' is not a `Nexus` `BlockType`; the combat-relevant computable block types are used here instead.
//!
//! `@schema_version` 1
//! `@source` entity-attributes.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{BlockType};

/// `Per`-`BlockType` static attribute schemas for computable `KeyBlocks` (`V1`.61 `KB` structured layer, compass `Q4`). `Attributes` are `IMMUTABLE` compute parameters (e.g. `max_hp`, `base_atk`) stored inside a `KeyBlock` body. `The` character shape is fully specified; other block types are permissive placeholders (additionalProperties: true) to be tightened as compute modules are added. `Note`: 'environment' is not a `Nexus` `BlockType`; the combat-relevant computable block types are used here instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct EntityAttributes {
    pub schema_version: u32,
    pub block_type: BlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}
/// `Fully`-specified static attributes for character `KeyBlocks`. `Additional` module-declared stats are permitted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CharacterAttributes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_atk: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_def: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
}
