//! `Nexus` `DeleteScheduleResponse`
//!
//! `Response` for `DELETE` /v1/local/orchestration/schedules/{`schedule_id`}.
//!
//! `@schema_version` 1
//! `@source` delete-schedule-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `DELETE` /v1/local/orchestration/schedules/{`schedule_id`}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DeleteScheduleResponse {
    pub deleted: bool,
}
