//! `Nexus` `SetActiveWorkspaceResponse`
//!
//! `Response` for `POST` /v1/daemon/workspace/active.
//!
//! `@schema_version` 1
//! `@source` set-active-workspace-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/daemon/workspace/active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SetActiveWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
}
