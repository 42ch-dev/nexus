//! `Nexus` `ListOrchestrationSessionsQuery`
//!
//! `Query` parameters for `GET` /v1/local/orchestration/sessions (cursor-based pagination + sort, `F`-`F1`).
//!
//! `@schema_version` 2
//! `@source` list-sessions-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/orchestration/sessions (cursor-based pagination + sort, `F`-`F1`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListSessionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}
