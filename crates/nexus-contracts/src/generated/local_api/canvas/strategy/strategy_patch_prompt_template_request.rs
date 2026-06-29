//! `Nexus` `StrategyPatchPromptTemplateRequest`
//!
//! `Request` body for `POST` /v1/local/strategies/{`strategy_id`}/states/{`state_id`}/prompt/patch (`V1`.71). `Atomically` updates a prompt-template file referenced by a state or inner-graph node inside the `Strategy` bundle.
//!
//! `@schema_version` 1
//! `@source` strategy-patch-prompt-template-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/strategies/{`strategy_id`}/states/{`state_id`}/prompt/patch (`V1`.71). `Atomically` updates a prompt-template file referenced by a state or inner-graph node inside the `Strategy` bundle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategyPatchPromptTemplateRequest {
    pub strategy_id: String,
    pub state_id: String,
    pub base_revision: u64,
    pub template_ref: String,
    pub set: serde_json::Value,
}
