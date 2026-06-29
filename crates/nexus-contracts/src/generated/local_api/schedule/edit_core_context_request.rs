//! `Nexus` `EditCoreContextRequest`
//!
//! `Request` body for `PATCH` /v1/local/orchestration/schedules/{`schedule_id`}/core-context.
//!
//! `@schema_version` 1
//! `@source` edit-core-context-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `PATCH` /v1/local/orchestration/schedules/{`schedule_id`}/core-context.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct EditCoreContextRequest {
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
