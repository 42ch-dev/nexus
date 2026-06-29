//! `Nexus` `InspectScheduleResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/schedules/{`schedule_id`}.
//!
//! `@schema_version` 1
//! `@source` inspect-schedule-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::schedule::schedule_summary::ScheduleSummary;

/// `Response` for `GET` /v1/local/orchestration/schedules/{`schedule_id`}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct InspectScheduleResponse {
    pub schedule: ScheduleSummary,
    pub depends_on: Vec<String>,
    pub concurrency_kind: String,
}
