//! `Nexus` `WorldKbKeyBlockStateResponse`
//!
//! `Read` projection for `GET` /v1/daemon/worlds/{`world_id`}/kb/key-blocks/{`key_block_id`}/state (`V1`.114 `P2`). `Surfaces` the mutable runtime state of a computable `KeyBlock` plus its computability flag and per-row `OCC` version.
//!
//! `@schema_version` 1
//! `@source` world-kb-key-block-state-response.schema.json

use serde::{Deserialize, Serialize};

/// `Read` projection for `GET` /v1/daemon/worlds/{`world_id`}/kb/key-blocks/{`key_block_id`}/state (`V1`.114 `P2`). `Surfaces` the mutable runtime state of a computable `KeyBlock` plus its computability flag and per-row `OCC` version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbKeyBlockStateResponse {
    pub state: serde_json::Value,
    pub is_computable: bool,
    pub version: u64,
}
