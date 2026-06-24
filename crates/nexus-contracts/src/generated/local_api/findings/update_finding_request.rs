//! `Nexus` `UpdateFindingRequest`
//!
//! `Request` body for `PATCH` /v1/local/works/{`work_id`}/findings/{`finding_id`}.
//!
//! `@schema_version` 1
//! `@source` update-finding-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `PATCH` /v1/local/works/{`work_id`}/findings/{`finding_id`}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct UpdateFindingRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_suggestion: Option<String>,
}
