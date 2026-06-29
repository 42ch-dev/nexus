//! `Nexus` `SyncPullRequest`
//!
//! `Request` body for `POST` /v1/sync/pull — incremental bundle fetch from the platform (`CLI`/daemon client contract).
//!
//! `@schema_version` 1
//! `@source` sync-pull-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/sync/pull — incremental bundle fetch from the platform (`CLI`/daemon client contract).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SyncPullRequest {
    pub schema_version: u32,
    pub world_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_confirmed_delta_sequence: Option<u64>,
}
