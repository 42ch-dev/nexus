//! `Nexus` `OutlineValidationError`
//!
//! `Structured` detail placed inside the canonical `ErrorResponse`.details field when an `Outline` or `Timeline` patch fails domain validation (`HTTP` 422). `Mirrors` the `validation_summary` shape of `OutlinePatchResponse`.
//!
//! `@schema_version` 1
//! `@source` outline-validation-error.schema.json

use serde::{Deserialize, Serialize};

/// `Structured` detail placed inside the canonical `ErrorResponse`.details field when an `Outline` or `Timeline` patch fails domain validation (`HTTP` 422). `Mirrors` the `validation_summary` shape of `OutlinePatchResponse`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OutlineValidationError {
    pub validation_summary: serde_json::Value,
}
