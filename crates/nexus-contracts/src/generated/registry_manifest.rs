//! ACP Registry Manifest
//!
//! Schema for the ACP Registry manifest response from the CDN. The registry lists available ACP agents with their distribution information.
//!
//! @schema_version 1
//! @source registry-manifest.schema.json

use serde::{Deserialize, Serialize};



/// ACP Registry Manifest
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RegistryManifest {
    pub version: String,
    pub agents: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<serde_json::Value>>,
}
