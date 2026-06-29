//! `Nexus` `ListPresetsResponse`
//!
//! `Response` for `GET` /v1/local/presets — presets grouped by source (embedded, system, user).
//!
//! `@schema_version` 1
//! `@source` list-presets-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::preset_management::preset_summary::PresetSummary;

/// `Response` for `GET` /v1/local/presets — presets grouped by source (embedded, system, user).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListPresetsResponse {
    pub embedded: Vec<PresetSummary>,
    pub system: Vec<PresetSummary>,
    pub user: Vec<PresetSummary>,
}
