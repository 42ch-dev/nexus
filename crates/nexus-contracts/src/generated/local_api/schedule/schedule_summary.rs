//! `Nexus` `ScheduleSummary`
//!
//! `Summary` row for a schedule in list/inspect responses.
//!
//! `@schema_version` 1
//! `@source` schedule-summary.schema.json

use serde::{Deserialize, Serialize};

/// `Summary` row for a schedule in list/inspect responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ScheduleSummary {
    pub schedule_id: String,
    pub creator_id: String,
    pub preset_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub current_core_context_version: i64,
    pub created_at: String,
    pub updated_at: String,
}
