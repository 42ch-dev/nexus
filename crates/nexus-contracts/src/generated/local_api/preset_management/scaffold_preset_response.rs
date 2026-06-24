//! `Nexus` `ScaffoldPresetResponse`
//!
//! `Response` for `POST` /v1/local/presets — scaffold result with created paths.
//!
//! `@schema_version` 1
//! `@source` scaffold-preset-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/presets — scaffold result with created paths.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ScaffoldPresetResponse {
    pub id: String,
    pub path: String,
}
