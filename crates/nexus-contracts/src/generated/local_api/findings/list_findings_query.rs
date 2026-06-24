//! `Nexus` `ListFindingsQuery`
//!
//! `Query` parameters for `GET` /v1/local/works/{`work_id`}/findings.
//!
//! `@schema_version` 1
//! `@source` list-findings-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/works/{`work_id`}/findings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListFindingsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}
