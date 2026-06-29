//! `Nexus` `PromoteWorldKbCandidateResponse`
//!
//! `Success` response for `POST` /v1/local/worlds/{`world_id`}/kb/promote-candidate (`V1`.73). `entity` is the resulting (or null for reject) `KeyBlock`; `job` is the updated extract-job projection; `version` is the new per-row version.
//!
//! `@schema_version` 1
//! `@source` world-kb-promote-candidate-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_entity_projection::WorldKbEntityProjection;
use crate::generated::local_api::canvas::world_kb::world_kb_extract_job_projection::WorldKbExtractJobProjection;

/// `Success` response for `POST` /v1/local/worlds/{`world_id`}/kb/promote-candidate (`V1`.73). `entity` is the resulting (or null for reject) `KeyBlock`; `job` is the updated extract-job projection; `version` is the new per-row version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbPromoteCandidateResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<WorldKbEntityProjection>,
    pub job: WorldKbExtractJobProjection,
    pub version: u64,
    pub validation_summary: serde_json::Value,
}
