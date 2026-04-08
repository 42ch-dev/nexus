//! WorkspaceBinding
//!
//! Binding between a local workspace and a remote world. Aligned with data-model-v1.md §5.14.
//!
//! @schema_version 1
//! @source workspace-binding.schema.json

use crate::generated::common_types::BindingStatus;
use serde::{Deserialize, Serialize};

/// Binding between a local workspace and a remote world. Aligned with data-model-v1.md §5.14.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
