//! `Nexus` `ReloadPresetResponse`
//!
//! `Response` for `POST` /v1/local/presets/{id}:reload.
//!
//! `@schema_version` 1
//! `@source` reload-preset-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/presets/{id}:reload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ReloadPresetResponse {
    pub id: String,
    pub reloaded: bool,
}
