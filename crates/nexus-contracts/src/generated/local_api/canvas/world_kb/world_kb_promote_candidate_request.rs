//! `Nexus` `PromoteWorldKbCandidateRequest`
//!
//! `Request` body for `POST` /v1/local/worlds/{`world_id`}/kb/promote-candidate (`V1`.73). adopt/reject/merge a pending candidate via the entity-scope-model §5.5.2 promotion state machine. `Per`-row `OCC` on `kb_extract_jobs`.version.
//!
//! `@schema_version` 1
//! `@source` world-kb-promote-candidate-request.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::canvas::world_kb::world_kb_entity_patch::WorldKbEntityPatch;

/// `Request` body for `POST` /v1/local/worlds/{`world_id`}/kb/promote-candidate (`V1`.73). adopt/reject/merge a pending candidate via the entity-scope-model §5.5.2 promotion state machine. `Per`-row `OCC` on `kb_extract_jobs`.version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbPromoteCandidateRequest {
    pub job_id: String,
    pub candidate_id: String,
    pub action: String,
    pub expected_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<WorldKbEntityPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}
