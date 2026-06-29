//! `Nexus` `OrchestrationSessionDetailResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/sessions/{`session_id`} — full session detail with status.
//!
//! `@schema_version` 1
//! `@source` session-detail-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::orchestration::sessions::session_summary::SessionSummary;

/// `Response` for `GET` /v1/local/orchestration/sessions/{`session_id`} — full session detail with status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SessionDetailResponse {
    pub session: SessionSummary,
}
