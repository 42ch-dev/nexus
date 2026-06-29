//! `Nexus` `OutlineConflictError`
//!
//! `Structured` detail placed inside the canonical `ErrorResponse`.details field when an `Outline` or `Timeline` patch is rejected because `base_revision` is stale (`HTTP` 409).
//!
//! `@schema_version` 1
//! `@source` outline-conflict-error.schema.json

use serde::{Deserialize, Serialize};

/// `Structured` detail placed inside the canonical `ErrorResponse`.details field when an `Outline` or `Timeline` patch is rejected because `base_revision` is stale (`HTTP` 409).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OutlineConflictError {
    pub current_revision: u64,
    pub node_id: String,
    pub conflicting_path: String,
    pub recovery_hint: String,
}
