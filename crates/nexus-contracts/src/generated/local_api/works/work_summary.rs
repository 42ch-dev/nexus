//! `Nexus` `WorkSummary`
//!
//! `Summary` row for a work in list responses.
//!
//! `@schema_version` 1
//! `@source` work-summary.schema.json

use serde::{Deserialize, Serialize};

/// `Summary` row for a work in list responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkSummary {
    pub work_id: String,
    pub title: String,
    pub status: String,
    pub intake_status: String,
    pub primary_preset_id: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_locked_at: Option<String>,
}
