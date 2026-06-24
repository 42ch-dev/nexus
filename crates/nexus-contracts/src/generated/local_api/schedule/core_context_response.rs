//! `Nexus` `CoreContextResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/schedules/{`schedule_id`}/core-context.
//!
//! `@schema_version` 1
//! `@source` core-context-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/orchestration/schedules/{`schedule_id`}/core-context.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CoreContextResponse {
    pub version: i64,
    pub payload_kind: String,
    pub content: serde_json::Value,
    pub derivation_kind: String,
    pub created_at: String,
}
