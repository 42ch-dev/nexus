//! `Nexus` `SoulNarrativeRequest`
//!
//! `Request` body for `POST` /v1/local/memory/soul/reflect. `Absent`/null `world_id` reads or regenerates the `Creator`-level narrative; a present `world_id` scopes read/regeneration to that world's per-`World` narrative (ownership verified server-side).
//!
//! `@schema_version` 1
//! `@source` soul-narrative-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/memory/soul/reflect. `Absent`/null `world_id` reads or regenerates the `Creator`-level narrative; a present `world_id` scopes read/regeneration to that world's per-`World` narrative (ownership verified server-side).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SoulNarrativeRequest {
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_regenerate: Option<bool>,
}
