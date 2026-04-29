//! `WorkspaceBinding` — local-only workspace binding.
//!
//! Binding between a local workspace and a remote world.
//! Aligned with data-model-v1.md §5.14.

use serde::{Deserialize, Serialize};

use crate::generated::common_types::BindingStatus;

/// Binding between a local workspace and a remote world.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceBinding {
    pub schema_version: u32,
    pub workspace_id: String,
    pub local_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    pub world_id: String,
    pub creator_id: String,
    pub binding_status: BindingStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_workspace_binding() {
        let v = WorkspaceBinding {
            schema_version: 1,
            workspace_id: "wrk_test".to_string(),
            local_root: "/home/user/project".to_string(),
            profile_name: Some("default".to_string()),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            binding_status: BindingStatus::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: WorkspaceBinding = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
