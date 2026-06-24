//! `Nexus` `FindingDetailResponse`
//!
//! `Response` for `GET` /v1/local/works/{`work_id`}/findings/{`finding_id`} and create/update responses.
//!
//! `@schema_version` 1
//! `@source` finding-detail-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/works/{`work_id`}/findings/{`finding_id`} and create/update responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct FindingDetailResponse {
    pub finding_id: String,
    pub work_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter: Option<i64>,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub description: String,
    pub target_executor: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_suggestion: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_hint: Option<String>,
}
