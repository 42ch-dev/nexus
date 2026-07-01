//! `Nexus` `ListMemoryFragmentsQuery`
//!
//! `Query` parameters for `GET` /v1/local/memory/fragments. `keyword` is an optional case-insensitive `LIKE` filter; `limit` defaults to 50 (clamped 1..=250) when omitted.
//!
//! `@schema_version` 1
//! `@source` list-memory-fragments-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/memory/fragments. `keyword` is an optional case-insensitive `LIKE` filter; `limit` defaults to 50 (clamped 1..=250) when omitted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListMemoryFragmentsQuery {
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}
