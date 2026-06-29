//! `Nexus` `AppendInspirationResponse`
//!
//! `Response` for `POST` /v1/local/works/{`work_id`}/inspiration.
//!
//! `@schema_version` 1
//! `@source` append-inspiration-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/works/{`work_id`}/inspiration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AppendInspirationResponse {
    pub work_id: String,
    pub inspiration_count: i64,
}
