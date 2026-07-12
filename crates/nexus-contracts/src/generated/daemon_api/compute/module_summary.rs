//! `Nexus` `ComputeModuleSummary`
//!
//! `Summary` of an installed compute module surfaced by the registry list endpoint.
//!
//! `@schema_version` 1
//! `@source` module-summary.schema.json

use serde::{Deserialize, Serialize};

/// `Summary` of an installed compute module surfaced by the registry list endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ModuleSummary {
    pub module_id: String,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required_key_block_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battle_report_kind: Option<String>,
}
