//! `Nexus` `ValidatePresetResponse`
//!
//! `Response` for `POST` /v1/local/presets:validate — validation result with structured errors and warnings.
//!
//! `@schema_version` 1
//! `@source` validate-preset-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/presets:validate — validation result with structured errors and warnings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatePresetResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_count: Option<i64>,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}
