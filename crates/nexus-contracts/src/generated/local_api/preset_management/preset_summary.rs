//! `Nexus` `PresetSummary`
//!
//! `Summary` of a single preset entry (id, source, run intents).
//!
//! `@schema_version` 1
//! `@source` preset-summary.schema.json

use serde::{Deserialize, Serialize};

/// `Summary` of a single preset entry (id, source, run intents).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PresetSummary {
    pub id: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_intents: Option<Vec<String>>,
}
