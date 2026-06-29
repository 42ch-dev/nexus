//! `Nexus` `WorldKbCandidatesResponse`
//!
//! `Read` projection for `GET` /v1/local/worlds/{`world_id`}/kb/candidates (`V1`.73). `Pending` promotion candidates with cursor pagination.
//!
//! `@schema_version` 1
//! `@source` world-kb-candidates-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::kb::pagination_info::PaginationInfo;
use crate::generated::local_api::canvas::world_kb::world_kb_candidate_projection::WorldKbCandidateProjection;

/// `Read` projection for `GET` /v1/local/worlds/{`world_id`}/kb/candidates (`V1`.73). `Pending` promotion candidates with cursor pagination.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbCandidatesResponse {
    pub items: Vec<WorldKbCandidateProjection>,
    pub pagination: PaginationInfo,
}
