//! `Nexus` `CreateWorkspaceRequest`
//!
//! `Request` body for `POST` /v1/local/workspaces.
//!
//! `@schema_version` 1
//! `@source` create-workspace-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/workspaces.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkspaceRequest {
    pub creator_id: String,
    pub workspace_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creative_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}
