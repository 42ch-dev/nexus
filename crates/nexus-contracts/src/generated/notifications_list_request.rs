//! Nexus NotificationsListRequest
//!
//! Request body for listing notifications (platform plan 20).
//!
//! @schema_version 1
//! @source notifications-list-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for listing notifications (platform plan 20).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NotificationsListRequest {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread_only: Option<bool>,
}
