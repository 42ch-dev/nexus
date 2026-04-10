//! Nexus NotificationsInboxItem
//!
//! Single inbox notification row (platform plan 20).
//!
//! @schema_version 1
//! @source notifications-inbox-item.schema.json

use serde::{Deserialize, Serialize};

/// Single inbox notification row (platform plan 20).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NotificationsInboxItem {
    pub schema_version: u32,
    pub notification_id: String,
    pub kind: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_url: Option<String>,
}
