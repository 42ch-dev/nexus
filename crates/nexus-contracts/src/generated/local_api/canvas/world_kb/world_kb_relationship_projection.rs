//! `Nexus` `WorldKbRelationshipProjection`
//!
//! `Canonical` wire projection of a `World` `KB` relationship row (`V1`.74). `One` stored row may yield two projections when symmetric=true: the stored direction and a derived `symmetric_reverse` direction.
//!
//! `@schema_version` 1
//! `@source` world-kb-relationship-projection.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_relationship_kind::WorldKbRelationshipKind;

/// `Canonical` wire projection of a `World` `KB` relationship row (`V1`.74). `One` stored row may yield two projections when symmetric=true: the stored direction and a derived `symmetric_reverse` direction.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbRelationshipProjection {
    pub relationship_id: String,
    pub world_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: WorldKbRelationshipKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_label: Option<String>,
    pub symmetric: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    pub source_anchor_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub version: u64,
    pub updated_at: String,
    pub projection_direction: String,
}
