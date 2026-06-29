//! `Nexus` `NotificationsMarkReadRequest`
//!
//! `Request` body for marking notifications read (platform plan 20). `Either` pass explicit ids or `mark_all`.
//!
//! `@schema_version` 1
//! `@source` notifications-mark-read-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for marking notifications read (platform plan 20). `Either` pass explicit ids or `mark_all`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NotificationsMarkReadRequest {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mark_all: Option<bool>,
}
