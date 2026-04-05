//! Nexus VersionRef
//!
//! Value object describing the baseline version of a bundle/entity/world. Aligned with data-model-v1.md §6.2.
//!
//! @schema_version 1
//! @source version-ref.schema.json

use serde::{Deserialize, Serialize};



/// Nexus VersionRef
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct VersionRef {
    pub entity_type: String,
    pub entity_id: String,
    pub revision: u64,
}
