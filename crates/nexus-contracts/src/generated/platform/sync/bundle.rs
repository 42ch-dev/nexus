//! `Nexus` `DeltaBundle` `Envelope`
//!
//! `DeltaBundle` envelope containing delta operations for world synchronization. `Aligned` with bundle-envelope-schema-v1.md §5.
//!
//! `@schema_version` 1
//! `@source` bundle.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common::common_types::{BundleType, ManuscriptPhase};
use crate::generated::platform::sync::delta::Delta;

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BundleDeltaResult {
    pub delta_index: u64,
    pub delta_apply_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_entity_revision: Option<i64>,
}
/// `DeltaBundle` envelope containing delta operations for world synchronization. `Aligned` with bundle-envelope-schema-v1.md §5.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Bundle {
    pub schema_version: u32,
    pub bundle_id: String,
    pub command_id: String,
    pub workspace_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub submitting_creator_id: String,
    pub bundle_type: BundleType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manuscript_phase: Option<ManuscriptPhase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_manuscript: Option<bool>,
    pub idempotency_key: String,
    pub canonical_hash: String,
    pub base_versions: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_confirmed_delta_sequence: Option<u64>,
    pub deltas: Vec<Delta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_apply_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_results: Option<Vec<BundleDeltaResult>>,
    pub created_at: String,
}
