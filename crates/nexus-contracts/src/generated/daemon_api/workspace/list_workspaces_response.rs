//! `Nexus` `ListWorkspacesResponse`
//!
//! `Response` for `GET` /v1/daemon/workspaces.
//!
//! `@schema_version` 1
//! `@source` list-workspaces-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::kb::pagination_info::PaginationInfo;
use crate::generated::daemon_api::workspace::workspace_summary::WorkspaceSummary;

/// `Response` for `GET` /v1/daemon/workspaces.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListWorkspacesResponse {
    pub items: Vec<WorkspaceSummary>,
    pub pagination: PaginationInfo,
}
