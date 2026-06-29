//! `Nexus` `PatchWorldKbRelationshipRequest`
//!
//! `Request` body for `POST` /v1/local/worlds/{`world_id`}/kb/patch-relationship (`V1`.74). `Action`-discriminated add/update/remove for typed `World` `KB` relationships with per-row `OCC` on `kb_relationships`.revision.
//!
//! `@schema_version` 1
//! `@source` world-kb-patch-relationship-request.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_relationship_input::WorldKbRelationshipInput;

/// `Request` body for `POST` /v1/local/worlds/{`world_id`}/kb/patch-relationship (`V1`.74). `Action`-discriminated add/update/remove for typed `World` `KB` relationships with per-row `OCC` on `kb_relationships`.revision.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbPatchRelationshipRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship: Option<WorldKbRelationshipInput>,
}
