//! `Nexus` `CreateWorldRequest`
//!
//! `Request` body for `POST` /v1/daemon/worlds. `The` daemon resolves the active creator; clients never send ownership.
//!
//! `@schema_version` 1
//! `@source` create-world-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/daemon/worlds. `The` daemon resolves the active creator; clients never send ownership.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorldRequest {
    pub title: String,
}
