//! `Nexus` `SoulNarrativeResponse`
//!
//! `Response` body for `POST` /v1/local/memory/soul/reflect. `Reports` the whole-`Creator` `SOUL` narrative cache state, stale metadata, current counts, and insufficient-data thresholds.
//!
//! `@schema_version` 1
//! `@source` soul-narrative-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `POST` /v1/local/memory/soul/reflect. `Reports` the whole-`Creator` `SOUL` narrative cache state, stale metadata, current counts, and insufficient-data thresholds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SoulNarrativeResponse {
    pub creator_id: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrative: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment_count_at_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fragment_created_at_at_generation: Option<String>,
    pub current_fragment_count: u64,
    pub current_distinct_keyword_count: u64,
    pub min_fragment_count: i64,
    pub min_distinct_keyword_count: i64,
}
