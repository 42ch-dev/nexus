//! `Nexus` `ListSchedulesQuery`
//!
//! `Query` parameters for `GET` /v1/local/orchestration/schedules.
//!
//! `@schema_version` 1
//! `@source` list-schedules-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/orchestration/schedules.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListSchedulesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}
