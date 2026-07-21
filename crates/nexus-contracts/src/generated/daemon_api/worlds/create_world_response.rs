//! `Nexus` `CreateWorldResponse`
//!
//! `Response` body for `POST` /v1/daemon/worlds (201 `Created`).
//!
//! `@schema_version` 1
//! `@source` create-world-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `POST` /v1/daemon/worlds (201 `Created`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorldResponse {
    pub world_id: String,
    pub status: String,
}
