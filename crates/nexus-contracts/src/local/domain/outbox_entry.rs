//! OutboxEntry — local-only outbox send queue item.
//!
//! OutboxEntry entity representing a local send queue item.
//! Platform does not observe this type directly (it's internal to
//! the CLI sync mechanism). Aligned with data-model-v1.md §5.13.

use serde::{Deserialize, Serialize};

use crate::generated::common_types::DeliveryState;

/// OutboxEntry entity representing a local send queue item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OutboxEntry {
    pub schema_version: u32,
    pub outbox_entry_id: String,
    pub bundle_id: String,
    pub idempotency_key: String,
    pub delivery_state: DeliveryState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_outbox_entry() {
        let v = OutboxEntry {
            schema_version: 1,
            outbox_entry_id: "obx_test123".to_string(),
            bundle_id: "bdl_test".to_string(),
            idempotency_key: "key_abc".to_string(),
            delivery_state: DeliveryState::Staged,
            retry_count: Some(0),
            last_error: None,
            next_retry_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: OutboxEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
