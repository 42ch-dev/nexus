//! `AgentProfile` — local-only agent configuration.
//!
//! Configuration for an `ACP` agent in a workspace. Aligned with data-model-v1.md §5.15.

use serde::{Deserialize, Serialize};

// Re-use wire common types for enums that live in common.schema.json
use crate::generated::common_types::{AgentProfileStatus, ProfileKind, SelectionMode, Transport};

/// Configuration for an ACP agent in a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AgentProfile {
    pub schema_version: u32,
    pub agent_profile_id: String,
    pub workspace_id: String,
    pub profile_kind: ProfileKind,
    pub selection_mode: SelectionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_output_manuscript: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<i64>,
    pub status: AgentProfileStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_agent_profile() {
        let v = AgentProfile {
            schema_version: 1,
            agent_profile_id: "ap_test123".to_string(),
            workspace_id: "wrk_test".to_string(),
            profile_kind: ProfileKind::LocalAgent,
            selection_mode: SelectionMode::Registry,
            registry_agent_id: Some("agent-id".to_string()),
            launch_command: None,
            transport: Some(Transport::Stdio),
            default_output_manuscript: Some(true),
            protocol_version: Some(1),
            status: AgentProfileStatus::Active,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: AgentProfile = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
