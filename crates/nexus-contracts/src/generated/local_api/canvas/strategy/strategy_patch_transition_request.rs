//! `Nexus` `StrategyPatchTransitionRequest`
//!
//! `Request` body for `POST` /v1/local/strategies/{`strategy_id`}/transitions/patch (`V1`.71). `Rewires` an outer transition (linear next, conditional branch, or default target) and/or updates its condition label.
//!
//! `@schema_version` 1
//! `@source` strategy-patch-transition-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/strategies/{`strategy_id`}/transitions/patch (`V1`.71). `Rewires` an outer transition (linear next, conditional branch, or default target) and/or updates its condition label.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategyPatchTransitionRequest {
    pub strategy_id: String,
    pub base_revision: u64,
    pub source_state_id: String,
    pub old_target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition_kind: Option<String>,
}
