//! `Nexus` `CoreContextHistoryEntry`
//!
//! `Single` entry in core context version history.
//!
//! `@schema_version` 1
//! `@source` core-context-history-entry.schema.json

use serde::{Deserialize, Serialize};

/// `Single` entry in core context version history.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CoreContextHistoryEntry {
    pub version: i64,
    pub payload_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    pub derivation_kind: String,
    pub created_at: String,
}
