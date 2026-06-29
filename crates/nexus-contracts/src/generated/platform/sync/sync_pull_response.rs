//! `Nexus` `SyncPullResponse`
//!
//! `Response` body for `POST` /v1/sync/pull — bundles to apply locally plus server cursors.
//!
//! `@schema_version` 1
//! `@source` sync-pull-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::platform::sync::bundle::Bundle;

/// `Response` body for `POST` /v1/sync/pull — bundles to apply locally plus server cursors.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SyncPullResponse {
    pub schema_version: u32,
    pub world_revision: u64,
    pub confirmed_delta_sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_up_to_date: Option<bool>,
    pub bundles: Vec<Bundle>,
}
