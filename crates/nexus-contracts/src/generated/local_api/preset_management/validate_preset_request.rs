//! `Nexus` `ValidatePresetRequest`
//!
//! `Request` body for `POST` /v1/local/presets:validate.
//!
//! `@schema_version` 1
//! `@source` validate-preset-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/presets:validate.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatePresetRequest {
    pub path: String,
}
