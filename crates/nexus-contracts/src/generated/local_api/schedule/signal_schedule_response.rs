//! `Nexus` `SignalScheduleResponse`
//!
//! `Response` for `POST` /v1/local/orchestration/schedules/{`schedule_id`}/signal.
//!
//! `@schema_version` 1
//! `@source` signal-schedule-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/orchestration/schedules/{`schedule_id`}/signal.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SignalScheduleResponse {
    pub schedule_id: String,
    pub status: String,
}
