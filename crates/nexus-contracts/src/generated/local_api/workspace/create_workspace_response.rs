//! `Nexus` `CreateWorkspaceResponse`
//!
//! `Response` for `POST` /v1/local/workspaces.
//!
//! `@schema_version` 1
//! `@source` create-workspace-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/workspaces.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: String,
    pub operational_dir: String,
    pub state_db_path: String,
}
