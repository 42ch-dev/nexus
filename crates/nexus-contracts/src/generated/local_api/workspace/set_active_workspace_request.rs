//! `Nexus` `SetActiveWorkspaceRequest`
//!
//! `Request` body for `POST` /v1/local/workspace/active.
//!
//! `@schema_version` 1
//! `@source` set-active-workspace-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/workspace/active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SetActiveWorkspaceRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    pub workspace_slug: String,
}
