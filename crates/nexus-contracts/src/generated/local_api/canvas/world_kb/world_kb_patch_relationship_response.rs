//! `Nexus` `PatchWorldKbRelationshipResponse`
//!
//! `Success` response for `POST` /v1/local/worlds/{`world_id`}/kb/patch-relationship (`V1`.74). `Returns` the committed relationship projection (absent on remove), the new per-row version, and validation diagnostics.
//!
//! `@schema_version` 1
//! `@source` world-kb-patch-relationship-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_relationship_projection::WorldKbRelationshipProjection;

/// `Success` response for `POST` /v1/local/worlds/{`world_id`}/kb/patch-relationship (`V1`.74). `Returns` the committed relationship projection (absent on remove), the new per-row version, and validation diagnostics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbPatchRelationshipResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship: Option<WorldKbRelationshipProjection>,
    pub version: u64,
    pub validation_summary: serde_json::Value,
}
