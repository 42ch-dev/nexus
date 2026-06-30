//! `Nexus` `MemoryFragmentInfo`
//!
//! `A` single memory-fragment row in the list-fragments response. `The` `Local` `API` intentionally exposes only `fragment_id` and `summary`; internal fragment fields (`session_id`, `creator_id`, keywords, `created_at`, ttl) are not part of this response.
//!
//! `@schema_version` 1
//! `@source` memory-fragment-info.schema.json

use serde::{Deserialize, Serialize};

/// `A` single memory-fragment row in the list-fragments response. `The` `Local` `API` intentionally exposes only `fragment_id` and `summary`; internal fragment fields (`session_id`, `creator_id`, keywords, `created_at`, ttl) are not part of this response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryFragmentInfo {
    pub fragment_id: String,
    pub summary: String,
}
