//! `Nexus` `OrchestrationSessionSummary`
//!
//! `Summary` of an active orchestration engine session.
//!
//! `@schema_version` 1
//! `@source` session-summary.schema.json

use serde::{Deserialize, Serialize};

/// `Summary` of an active orchestration engine session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SessionSummary {
    pub session_id: String,
    pub creator_id: String,
    pub preset_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task_id: Option<String>,
}
