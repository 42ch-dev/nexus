//! `Nexus` `CreateWorkResponse`
//!
//! `Response` for `POST` /v1/local/works.
//!
//! `@schema_version` 1
//! `@source` create-work-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/works.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkResponse {
    pub work_id: String,
    pub status: String,
}
