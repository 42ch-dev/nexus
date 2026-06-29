//! `Nexus` `AddScheduleRequest`
//!
//! `Request` body for `POST` /v1/local/orchestration/schedules — create a new schedule.
//!
//! `@schema_version` 1
//! `@source` add-schedule-request.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::schedule::schedule_concurrency_request::ScheduleConcurrencyRequest;

/// `Request` body for `POST` /v1/local/orchestration/schedules — create a new schedule.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AddScheduleRequest {
    pub creator_id: String,
    pub preset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<ScheduleConcurrencyRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_gates: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
