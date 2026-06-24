//! `Nexus` `ListSchedulesResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/schedules.
//!
//! `@schema_version` 1
//! `@source` list-schedules-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::schedule::schedule_summary::ScheduleSummary;

/// `Response` for `GET` /v1/local/orchestration/schedules.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListSchedulesResponse {
    pub schedules: Vec<ScheduleSummary>,
}
