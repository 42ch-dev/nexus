//! `Nexus` `SignalScheduleRequest`
//!
//! `Request` body for `POST` /v1/local/orchestration/schedules/{`schedule_id`}/signal.
//!
//! `@schema_version` 1
//! `@source` signal-schedule-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/orchestration/schedules/{`schedule_id`}/signal.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SignalScheduleRequest {
    pub signal: String,
}
