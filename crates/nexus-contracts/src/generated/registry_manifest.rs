//! ACP Registry Manifest
//!
//! Schema for the ACP Registry manifest response from the CDN. The registry lists available ACP agents with their distribution information.
//!
//! @schema_version 1
//! @source registry-manifest.schema.json

use serde::{Deserialize, Serialize};

/// Schema for the ACP Registry manifest response from the CDN. The registry lists available ACP agents with their distribution information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RegistryManifest {
    pub version: String,
    pub agents: Vec<AgentEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<serde_json::Value>>,
}
/// AgentEntry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AgentEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub distribution: Distribution,
}
/// Agent distribution configuration (npx or binary)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Distribution {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npx: Option<NpxDistribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<BinaryDistribution>,
}
/// NpxDistribution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NpxDistribution {
    pub package: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
}
/// Per-platform binary distribution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BinaryDistribution {
    #[serde(rename = "darwin-aarch64")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub darwin_aarch64: Option<PlatformBinary>,
    #[serde(rename = "darwin-x86_64")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub darwin_x86_64: Option<PlatformBinary>,
    #[serde(rename = "linux-aarch64")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux_aarch64: Option<PlatformBinary>,
    #[serde(rename = "linux-x86_64")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux_x86_64: Option<PlatformBinary>,
    #[serde(rename = "windows-aarch64")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_aarch64: Option<PlatformBinary>,
    #[serde(rename = "windows-x86_64")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_x86_64: Option<PlatformBinary>,
}
/// PlatformBinary
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PlatformBinary {
    pub archive: String,
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}
