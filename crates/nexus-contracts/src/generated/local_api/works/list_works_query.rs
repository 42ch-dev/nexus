//! `Nexus` `ListWorksQuery`
//!
//! `Query` parameters for `GET` /v1/local/works (cursor-based pagination + sort, `F`-`P1` / `F`-`F1`).
//!
//! `@schema_version` 2
//! `@source` list-works-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/works (cursor-based pagination + sort, `F`-`P1` / `F`-`F1`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListWorksQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intake_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}
