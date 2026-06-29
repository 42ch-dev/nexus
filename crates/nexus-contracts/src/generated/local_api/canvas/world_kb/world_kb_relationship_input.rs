//! `Nexus` `WorldKbRelationshipInput`
//!
//! `Author`-editable payload for a `World` `KB` relationship (`V1`.74). `Supplied` inside `WorldKbPatchRelationshipRequest` for add/update actions.
//!
//! `@schema_version` 1
//! `@source` world-kb-relationship-input.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_relationship_kind::WorldKbRelationshipKind;

/// `Author`-editable payload for a `World` `KB` relationship (`V1`.74). `Supplied` inside `WorldKbPatchRelationshipRequest` for add/update actions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbRelationshipInput {
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: WorldKbRelationshipKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_label: Option<String>,
    pub symmetric: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}
