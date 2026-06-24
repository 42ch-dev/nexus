//! `Nexus` `ListOrchestrationSessionsResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/sessions.
//!
//! `@schema_version` 1
//! `@source` list-sessions-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::orchestration::sessions::session_summary::SessionSummary;

/// `Response` for `GET` /v1/local/orchestration/sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionSummary>,
}
