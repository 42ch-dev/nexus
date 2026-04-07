//! Nexus OutboxEntry
//!
//! OutboxEntry entity representing a local send queue item. Aligned with data-model-v1.md §5.13.
//!
//! @schema_version 1
//! @source outbox-entry.schema.json

use serde::{Deserialize, Serialize};

/// OutboxEntry entity representing a local send queue item. Aligned with data-model-v1.md §5.13.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OutboxEntry {
    pub schema_version: u32,
    pub outbox_entry_id: String,
    pub bundle_id: String,
    pub idempotency_key: String,
    pub delivery_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
