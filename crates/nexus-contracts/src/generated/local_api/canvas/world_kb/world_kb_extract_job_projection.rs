//! `Nexus` `WorldKbExtractJobProjection`
//!
//! `Extract`-job projection returned after a promotion action (`V1`.73). `version` maps to `kb_extract_jobs`.version `CAS` column.
//!
//! `@schema_version` 1
//! `@source` world-kb-extract-job-projection.schema.json

use serde::{Deserialize, Serialize};

/// `Extract`-job projection returned after a promotion action (`V1`.73). `version` maps to `kb_extract_jobs`.version `CAS` column.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbExtractJobProjection {
    pub job_id: String,
    pub world_id: String,
    pub status: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
