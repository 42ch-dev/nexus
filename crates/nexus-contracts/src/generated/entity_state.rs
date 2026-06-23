//! `Nexus` `Entity` `State` (`Dynamic` `Compute` `Runtime`)
//!
//! `Per`-`BlockType` dynamic state schemas for computable `KeyBlocks` (`V1`.61 `KB` structured layer, compass `Q4`/`Q5`). `State` is `MUTABLE` runtime data (e.g. `current_hp`, `status_effects`) nested by `block_type` so the same `KeyBlock` can serve different module types without field-name collisions (compass `Q5`: state.character.`current_hp`). `The` character shape is fully specified; other block types are permissive placeholders.
//!
//! `@schema_version` 1
//! `@source` entity-state.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{BlockType};

/// `Per`-`BlockType` dynamic state schemas for computable `KeyBlocks` (`V1`.61 `KB` structured layer, compass `Q4`/`Q5`). `State` is `MUTABLE` runtime data (e.g. `current_hp`, `status_effects`) nested by `block_type` so the same `KeyBlock` can serve different module types without field-name collisions (compass `Q5`: state.character.`current_hp`). `The` character shape is fully specified; other block types are permissive placeholders.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct EntityState {
    pub schema_version: u32,
    pub block_type: BlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<serde_json::Value>,
}
/// `Fully`-specified dynamic state for character `KeyBlocks`. `Additional` module-declared runtime fields are permitted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CharacterState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_effects: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_alive: Option<bool>,
}
