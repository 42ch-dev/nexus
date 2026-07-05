//! `Nexus` `ListWorkspacesQuery`
//!
//! `Query` parameters for `GET` /v1/daemon/workspaces.
//!
//! `@schema_version` 1
//! `@source` list-workspaces-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/daemon/workspaces.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListWorkspacesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
