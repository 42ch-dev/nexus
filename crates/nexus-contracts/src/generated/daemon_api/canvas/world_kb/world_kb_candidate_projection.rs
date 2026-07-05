//! `Nexus` `WorldKbCandidateProjection`
//!
//! `Pending` promotion candidate projection for the `World` `KB` promotion inspector (`V1`.73). `Backed` by `kb_extract_jobs` + the pending `KeyBlock` row.
//!
//! `@schema_version` 1
//! `@source` world-kb-candidate-projection.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common::common_types::{BlockType};

/// `Pending` promotion candidate projection for the `World` `KB` promotion inspector (`V1`.73). `Backed` by `kb_extract_jobs` + the pending `KeyBlock` row.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbCandidateProjection {
    pub candidate_id: String,
    pub job_id: String,
    pub world_id: String,
    pub block_type: BlockType,
    pub canonical_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}
