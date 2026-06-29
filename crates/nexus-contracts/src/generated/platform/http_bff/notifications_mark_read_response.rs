//! `Nexus` `NotificationsMarkReadResponse`
//!
//! `Response` for mark-read mutations (platform plan 20).
//!
//! `@schema_version` 1
//! `@source` notifications-mark-read-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for mark-read mutations (platform plan 20).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NotificationsMarkReadResponse {
    pub schema_version: u32,
    pub success: bool,
    pub updated_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
