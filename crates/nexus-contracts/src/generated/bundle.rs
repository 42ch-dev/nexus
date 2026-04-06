//! Nexus CLI Sync Bundle (V1.0)
//!
//! V1.0 sync-specific bundle view for CLI <-> Platform synchronization. Reuses domain bundle envelope with sync-specific constraints.
//!
//! @schema_version 1
//! @source bundle.schema.json

use serde::{Deserialize, Serialize};



/// Nexus CLI Sync Bundle (V1.0)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Bundle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_type: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manuscript_phase: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_manuscript: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitting_creator_id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deltas: Option<serde_json::Value>,
}
