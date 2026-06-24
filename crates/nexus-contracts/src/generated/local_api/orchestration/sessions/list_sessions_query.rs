//! `Nexus` `ListOrchestrationSessionsQuery`
//!
//! `Query` parameters for `GET` /v1/local/orchestration/sessions.
//!
//! `@schema_version` 1
//! `@source` list-sessions-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/orchestration/sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListSessionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
}
