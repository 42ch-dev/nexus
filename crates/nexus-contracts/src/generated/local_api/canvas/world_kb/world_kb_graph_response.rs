//! `Nexus` `WorldKbGraphResponse`
//!
//! `Read` projection for `GET` /v1/local/worlds/{`world_id`}/kb/graph (`V1`.73). `Entities` + source-anchor provenance edges. `relationships` is always empty in `V1`.73 (no `kb_relationships` table until `V1`.74); derived reference edges render read-only from `source_anchors`.
//!
//! `@schema_version` 1
//! `@source` world-kb-graph-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_entity_projection::WorldKbEntityProjection;
use crate::generated::local_api::canvas::world_kb::world_kb_source_anchor_projection::WorldKbSourceAnchorProjection;

/// `Read` projection for `GET` /v1/local/worlds/{`world_id`}/kb/graph (`V1`.73). `Entities` + source-anchor provenance edges. `relationships` is always empty in `V1`.73 (no `kb_relationships` table until `V1`.74); derived reference edges render read-only from `source_anchors`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbGraphResponse {
    pub entities: Vec<WorldKbEntityProjection>,
    pub source_anchors: Vec<WorldKbSourceAnchorProjection>,
    pub relationships: Vec<serde_json::Value>,
}
