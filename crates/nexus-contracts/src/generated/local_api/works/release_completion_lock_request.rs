//! `Nexus` `ReleaseCompletionLockRequest`
//!
//! `Request` body for `POST` /v1/local/works/{`work_id`}/completion-lock/release.
//!
//! `@schema_version` 1
//! `@source` release-completion-lock-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/works/{`work_id`}/completion-lock/release.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ReleaseCompletionLockRequest {
    pub reason: String,
}
