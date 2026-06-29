//! `Nexus` `CreateFindingRequest`
//!
//! `Request` body for `POST` /v1/local/works/{`work_id`}/findings.
//!
//! `@schema_version` 1
//! `@source` create-finding-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/works/{`work_id`}/findings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CreateFindingRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter: Option<i64>,
    pub severity: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_suggestion: Option<String>,
}
