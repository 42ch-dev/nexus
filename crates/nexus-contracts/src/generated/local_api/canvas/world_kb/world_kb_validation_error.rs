//! `Nexus` `WorldKbValidationError`
//!
//! `Structured` detail placed inside the canonical `ErrorResponse`.details field when a `World` `KB` patch is rejected for domain-rule violations (`HTTP` 422, `V1`.73). `Distinct` from 409 `WorldKbConflictError` which is concurrent-write version mismatch only.
//!
//! `@schema_version` 1
//! `@source` world-kb-validation-error.schema.json

use serde::{Deserialize, Serialize};

/// `Structured` detail placed inside the canonical `ErrorResponse`.details field when a `World` `KB` patch is rejected for domain-rule violations (`HTTP` 422, `V1`.73). `Distinct` from 409 `WorldKbConflictError` which is concurrent-write version mismatch only.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbValidationError {
    pub validation_summary: serde_json::Value,
}
