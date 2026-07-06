//! `Nexus` `FindingBatchPatch`
//!
//! `Fields` to patch on each matching finding in a batch update. `At` least one field should be present.
//!
//! `@schema_version` 1
//! `@source` finding-batch-patch.schema.json

use serde::{Deserialize, Serialize};

/// `Fields` to patch on each matching finding in a batch update. `At` least one field should be present.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct FindingBatchPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_executor: Option<String>,
}
