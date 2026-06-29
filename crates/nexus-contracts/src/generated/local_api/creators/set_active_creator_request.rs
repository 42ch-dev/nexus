//! `Nexus` `SetActiveCreatorRequest`
//!
//! `Request` body for `POST` /v1/local/creators/active.
//!
//! `@schema_version` 1
//! `@source` set-active-creator-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/creators/active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SetActiveCreatorRequest {
    pub creator_id: String,
}
