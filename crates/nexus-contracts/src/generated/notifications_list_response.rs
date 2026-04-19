//! Nexus NotificationsListResponse
//!
//! Paginated notifications list (platform plan 20). Item shape matches NotificationsInboxItem fields for wire stability.
//!
//! @schema_version 1
//! @source notifications-list-response.schema.json

use serde::{Deserialize, Serialize};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NotificationsListResponseItem {
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
/// Paginated notifications list (platform plan 20). Item shape matches NotificationsInboxItem fields for wire stability.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NotificationsListResponse {
    pub schema_version: u32,
    pub items: Vec<NotificationsListResponseItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
