//! `Nexus` `GetPresetResponse`
//!
//! `Response` for `GET` /v1/local/presets/{id} (`V1`.65 `P0`). `Returns` the preset manifest as raw `YAML` so clients can edit and `PATCH` it back.
//!
//! `@schema_version` 1
//! `@source` get-preset-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/presets/{id} (`V1`.65 `P0`). `Returns` the preset manifest as raw `YAML` so clients can edit and `PATCH` it back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GetPresetResponse {
    pub id: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub yaml: String,
}
