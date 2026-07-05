//! `Nexus` `MemoryFragmentInfo`
//!
//! `A` single memory-fragment row in the list-fragments response. `V1`.79 exposes keyword and creation-time metadata for read-only `SOUL` visualization; write-only/internal fragment fields (`session_id`, `creator_id`, ttl) remain out of this response.
//!
//! `@schema_version` 1
//! `@source` memory-fragment-info.schema.json

use serde::{Deserialize, Serialize};

/// `A` single memory-fragment row in the list-fragments response. `V1`.79 exposes keyword and creation-time metadata for read-only `SOUL` visualization; write-only/internal fragment fields (`session_id`, `creator_id`, ttl) remain out of this response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryFragmentInfo {
    pub fragment_id: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}
