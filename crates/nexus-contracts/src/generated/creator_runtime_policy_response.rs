//! CreatorRuntimePolicyResponseV1
//!
//! GET /creators/:id/runtime-policy 200 response body. Exposes Creator-level policy capabilities for CLI consumption. SSOT: v1-spec platform/local-first-runtime-policy-v1.md §4, §7.
//!
//! @schema_version 1
//! @source creator-runtime-policy-response.schema.json

use serde::{Deserialize, Serialize};

/// GET /creators/:id/runtime-policy 200 response body. Exposes Creator-level policy capabilities for CLI consumption. SSOT: v1-spec platform/local-first-runtime-policy-v1.md §4, §7.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CreatorRuntimePolicyResponse {
    pub schema_version: u32,
    pub creator_id: String,
    pub memory_structured_write: bool,
    pub memory_vector_index: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_first_embedding_remaining: Option<u64>,
}
