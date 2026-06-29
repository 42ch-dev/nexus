//! `Nexus` `AddScheduleResponse`
//!
//! `Response` for `POST` /v1/local/orchestration/schedules.
//!
//! `@schema_version` 1
//! `@source` add-schedule-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/orchestration/schedules.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AddScheduleResponse {
    pub schedule_id: String,
    pub status: String,
    pub core_context_version: i64,
}
