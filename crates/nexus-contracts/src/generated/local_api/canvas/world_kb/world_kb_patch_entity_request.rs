//! `Nexus` `PatchWorldKbEntityRequest`
//!
//! `Request` body for `POST` /v1/local/worlds/{`world_id`}/kb/patch-entity (`V1`.73). `Edits` an entity (`KeyBlock`) title/body/aliases/`block_type` with per-row `OCC` on `kb_key_blocks`.revision.
//!
//! `@schema_version` 1
//! `@source` world-kb-patch-entity-request.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_entity_patch::WorldKbEntityPatch;

/// `Request` body for `POST` /v1/local/worlds/{`world_id`}/kb/patch-entity (`V1`.73). `Edits` an entity (`KeyBlock`) title/body/aliases/`block_type` with per-row `OCC` on `kb_key_blocks`.revision.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbPatchEntityRequest {
    pub entity_id: String,
    pub expected_version: u64,
    pub patch: WorldKbEntityPatch,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}
