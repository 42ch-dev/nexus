//! `Nexus` `SetActiveCreatorResponse`
//!
//! `Response` for `POST` /v1/local/creators/active.
//!
//! `@schema_version` 1
//! `@source` set-active-creator-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/creators/active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SetActiveCreatorResponse {
    pub creator_id: String,
}
