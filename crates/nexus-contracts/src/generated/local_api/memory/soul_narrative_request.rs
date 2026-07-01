//! `Nexus` `SoulNarrativeRequest`
//!
//! `Request` body for `POST` /v1/local/memory/soul/reflect. `The` endpoint reads or regenerates the cached whole-`Creator` `SOUL` narrative; per-world narratives are out of scope for `V1`.81.
//!
//! `@schema_version` 1
//! `@source` soul-narrative-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/memory/soul/reflect. `The` endpoint reads or regenerates the cached whole-`Creator` `SOUL` narrative; per-world narratives are out of scope for `V1`.81.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SoulNarrativeRequest {
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_regenerate: Option<bool>,
}
