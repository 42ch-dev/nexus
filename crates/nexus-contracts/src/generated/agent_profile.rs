//! AgentProfile
//!
//! Configuration for an ACP agent in a workspace. Aligned with data-model-v1.md §5.15.
//!
//! @schema_version 1
//! @source agent-profile.schema.json

use serde::{Deserialize, Serialize};

/// Configuration for an ACP agent in a workspace. Aligned with data-model-v1.md §5.15.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AgentProfile {
    pub schema_version: u32,
    pub agent_profile_id: String,
    pub workspace_id: String,
    pub profile_kind: String,
    pub selection_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_output_manuscript: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
