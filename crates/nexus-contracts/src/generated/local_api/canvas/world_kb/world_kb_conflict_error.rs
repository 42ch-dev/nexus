//! `Nexus` `WorldKbConflictError`
//!
//! `Structured` detail placed inside the canonical `ErrorResponse`.details field when a `World` `KB` patch is rejected because `expected_version` is stale (`HTTP` 409). `Per`-row `OCC` on `kb_key_blocks`.revision / `kb_extract_jobs`.version (`V1`.73).
//!
//! `@schema_version` 1
//! `@source` world-kb-conflict-error.schema.json

use serde::{Deserialize, Serialize};

/// `Structured` detail placed inside the canonical `ErrorResponse`.details field when a `World` `KB` patch is rejected because `expected_version` is stale (`HTTP` 409). `Per`-row `OCC` on `kb_key_blocks`.revision / `kb_extract_jobs`.version (`V1`.73).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbConflictError {
    pub current_version: u64,
    pub entity_id: String,
    pub conflicting_path: String,
    pub recovery_hint: String,
}
