//! `Nexus` `WorldKbPatchEntityResponse`
//!
//! `Success` response for `POST` /v1/local/worlds/{`world_id`}/kb/patch-entity (`V1`.73). `Returns` the updated entity projection, the new per-row version, and validation diagnostics.
//!
//! `@schema_version` 1
//! `@source` world-kb-patch-entity-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_entity_projection::WorldKbEntityProjection;

/// `Success` response for `POST` /v1/local/worlds/{`world_id`}/kb/patch-entity (`V1`.73). `Returns` the updated entity projection, the new per-row version, and validation diagnostics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbPatchEntityResponse {
    pub entity: WorldKbEntityProjection,
    pub version: u64,
    pub validation_summary: serde_json::Value,
}
