//! `Nexus` `UpdatePresetRequest`
//!
//! `Request` body for `PATCH` /v1/local/presets/{id} (`V1`.65 `P0`). `Replaces` the user preset's preset.yaml content after validation.
//!
//! `@schema_version` 1
//! `@source` update-preset-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `PATCH` /v1/local/presets/{id} (`V1`.65 `P0`). `Replaces` the user preset's preset.yaml content after validation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePresetRequest {
    pub yaml: String,
}
