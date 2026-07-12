//! `Nexus` `ComputeModuleDetail`
//!
//! `Full` manifest.json shape for a compute module, as defined by compute-module-abi.md §7.
//!
//! `@schema_version` 1
//! `@source` module-detail.schema.json

use serde::{Deserialize, Serialize};

/// `Full` manifest.json shape for a compute module, as defined by compute-module-abi.md §7.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ModuleDetail {
    pub module_id: String,
    pub name: String,
    pub version: String,
    pub nexus_abi_version: i64,
    pub required_key_block_types: Vec<String>,
    pub compute_export: String,
    pub init_export: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_functions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schemas: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battle_report_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fuel: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_memory_mib: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_wall_time_ms: Option<i64>,
}
