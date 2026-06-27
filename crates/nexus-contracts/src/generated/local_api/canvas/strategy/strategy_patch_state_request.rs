//! `Nexus` `StrategyPatchStateRequest`
//!
//! `Request` body for `POST` /v1/local/strategies/{`strategy_id`}/states/{`state_id`}/patch (`V1`.71). `Renames` and/or updates the description of a single outer state-machine state.
//!
//! `@schema_version` 1
//! `@source` strategy-patch-state-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/strategies/{`strategy_id`}/states/{`state_id`}/patch (`V1`.71). `Renames` and/or updates the description of a single outer state-machine state.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct StrategyPatchStateRequest {
    pub strategy_id: String,
    pub state_id: String,
    pub base_revision: u64,
    pub set: serde_json::Value,
}
