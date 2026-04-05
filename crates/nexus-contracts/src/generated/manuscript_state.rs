//! Nexus ManuscriptState
//!
//! ManuscriptState - local-only manuscript phase machine. Platform does not own this in V1.0. Aligned with data-model-v1.md §5.9B.
//!
//! @schema_version 1
//! @source manuscript-state.schema.json

use serde::{Deserialize, Serialize};

use crate::generated::common_types::ManuscriptPhase;

/// Nexus ManuscriptState
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ManuscriptState {
    pub schema_version: u32,
    pub manuscript_state_id: String,
    pub workspace_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub manuscript_phase: ManuscriptPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_manifest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_confirmed_delta_sequence: Option<u64>,
    pub updated_at: String,
}
