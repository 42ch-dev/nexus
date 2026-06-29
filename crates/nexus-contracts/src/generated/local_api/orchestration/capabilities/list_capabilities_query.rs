//! `Nexus` `ListCapabilitiesQuery`
//!
//! `Query` parameters for `GET` /v1/local/orchestration/capabilities (cursor-based pagination + sort, `F`-`F1`).
//!
//! `@schema_version` 2
//! `@source` list-capabilities-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/orchestration/capabilities (cursor-based pagination + sort, `F`-`F1`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListCapabilitiesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}
