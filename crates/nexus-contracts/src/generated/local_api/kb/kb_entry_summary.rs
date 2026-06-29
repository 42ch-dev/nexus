//! `Nexus` `KbEntrySummary`
//!
//! `Summary` row for a `KB` entry in list responses.
//!
//! `@schema_version` 1
//! `@source` kb-entry-summary.schema.json

use serde::{Deserialize, Serialize};

/// `Summary` row for a `KB` entry in list responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct KbEntrySummary {
    pub entry_id: String,
    pub title: String,
    pub created_at: String,
}
