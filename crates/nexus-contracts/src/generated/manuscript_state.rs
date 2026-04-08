//! Nexus ManuscriptState
//!
//! ManuscriptState - local-only manuscript phase machine tracking creation progression. Platform may receive manuscript_phase as bundle metadata but does not own this aggregate in V1.0. Aligned with data-model-v1.md §5.9B.
//!
//! @schema_version 1
//! @source manuscript-state.schema.json

use crate::generated::common_types::ManuscriptPhase;
use serde::{Deserialize, Serialize};

/// ManuscriptState - local-only manuscript phase machine tracking creation progression. Platform may receive manuscript_phase as bundle metadata but does not own this aggregate in V1.0. Aligned with data-model-v1.md §5.9B.
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
