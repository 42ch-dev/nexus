//! `RegistryManifest` — local-only `ACP` registry manifest.
//!
//! Schema for the `ACP` Registry manifest response from the `CDN`.
//! The registry lists available `ACP` agents with their distribution information.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ACP Registry manifest response from the CDN.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RegistryManifest {
    /// Registry format version (e.g. "1.0.0")
    pub version: String,
    /// List of available ACP agents.
    pub agents: Vec<AgentEntry>,
    /// Registry extensions (reserved for future use).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<serde_json::Value>>,
}

/// An ACP agent entry in the registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AgentEntry {
    /// Unique agent identifier.
    pub id: String,
    /// Human-readable agent name.
    pub name: String,
    /// Agent version.
    pub version: String,
    /// Agent description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Agent source repository URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Agent authors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    /// Agent license identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Agent icon URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Agent distribution configuration (npx or binary).
    pub distribution: Distribution,
}

/// Agent distribution configuration (npx or binary).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Distribution {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npx: Option<NpxDistribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<BinaryDistribution>,
}

/// Npx-based distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct NpxDistribution {
    /// npm package name with optional version (e.g. @scope/pkg@1.0.0)
    pub package: String,
    /// Additional CLI arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Environment variables to set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// Per-platform binary distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

/// Platform-specific binary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PlatformBinary {
    /// Download URL for platform-specific archive.
    pub archive: String,
    /// Command to execute within the archive.
    pub cmd: String,
    /// Additional CLI arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_minimal_manifest() {
        let v = RegistryManifest {
            version: "1.0.0".to_string(),
            agents: vec![AgentEntry {
                id: "test-agent".to_string(),
                name: "Test Agent".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                repository: None,
                authors: None,
                license: None,
                icon: None,
                distribution: Distribution {
                    npx: Some(NpxDistribution {
                        package: "@scope/agent@1.0.0".to_string(),
                        args: None,
                        env: None,
                    }),
                    binary: None,
                },
            }],
            extensions: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: RegistryManifest = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
