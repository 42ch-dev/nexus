//! `Nexus` `ScaffoldPresetRequest`
//!
//! `Request` body for `POST` /v1/local/presets — scaffold a new user preset.
//!
//! `@schema_version` 1
//! `@source` scaffold-preset-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/presets — scaffold a new user preset.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ScaffoldPresetRequest {
    pub name: String,
}
