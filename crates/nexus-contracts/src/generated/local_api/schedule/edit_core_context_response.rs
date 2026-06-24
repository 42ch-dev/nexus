//! `Nexus` `EditCoreContextResponse`
//!
//! `Response` for `PATCH` /v1/local/orchestration/schedules/{`schedule_id`}/core-context.
//!
//! `@schema_version` 1
//! `@source` edit-core-context-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `PATCH` /v1/local/orchestration/schedules/{`schedule_id`}/core-context.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct EditCoreContextResponse {
    pub new_version: i64,
}
