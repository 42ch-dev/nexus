//! `Nexus` `CapabilityInfo`
//!
//! `Description` of a single registered capability (name + `I`/`O` schemas).
//!
//! `@schema_version` 1
//! `@source` capability-info.schema.json

use serde::{Deserialize, Serialize};

/// `Description` of a single registered capability (name + `I`/`O` schemas).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CapabilityInfo {
    pub name: String,
    pub input_schema: String,
    pub output_schema: String,
}
