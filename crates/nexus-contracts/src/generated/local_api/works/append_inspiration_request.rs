//! `Nexus` `AppendInspirationRequest`
//!
//! `Request` body for `POST` /v1/local/works/{`work_id`}/inspiration.
//!
//! `@schema_version` 1
//! `@source` append-inspiration-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/works/{`work_id`}/inspiration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AppendInspirationRequest {
    pub note: String,
}
