//! `Nexus` `UpdatePresetResponse`
//!
//! `Response` for `PATCH` /v1/local/presets/{id} (`V1`.65 `P0`).
//!
//! `@schema_version` 1
//! `@source` update-preset-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `PATCH` /v1/local/presets/{id} (`V1`.65 `P0`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePresetResponse {
    pub id: String,
    pub updated: bool,
}
