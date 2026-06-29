//! `Nexus` `StrategyPatchResponse`
//!
//! `Success` response for `Strategy` patch routes (`V1`.71). `Returns` the committed revision and any domain validation diagnostics produced during the patch.
//!
//! `@schema_version` 1
//! `@source` strategy-patch-response.schema.json

use serde::{Deserialize, Serialize};

/// `Success` response for `Strategy` patch routes (`V1`.71). `Returns` the committed revision and any domain validation diagnostics produced during the patch.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategyPatchResponse {
    pub new_revision: i64,
    pub validation_summary: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_effects: Option<Vec<String>>,
}
