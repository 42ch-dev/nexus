//! `Nexus` `ActiveWorkspaceResponse`
//!
//! `Response` for `GET` /v1/local/workspace.
//!
//! `@schema_version` 1
//! `@source` active-workspace-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/workspace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ActiveWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creative_root: Option<String>,
    pub operational_dir: String,
}
