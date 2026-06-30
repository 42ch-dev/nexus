//! `Nexus` `ReviewResponse`
//!
//! `Response` body for `POST` /v1/local/memory/review. `Summarizes` how many pending entries were promoted to long-term memory, fragmented, or dropped by the rule-based classifier. `Shipped` behavior: `PassthroughSummarizer` (no `LLM`); each pending row is classified and the pending row is deleted on promote/fragment/drop success.
//!
//! `@schema_version` 1
//! `@source` review-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `POST` /v1/local/memory/review. `Summarizes` how many pending entries were promoted to long-term memory, fragmented, or dropped by the rule-based classifier. `Shipped` behavior: `PassthroughSummarizer` (no `LLM`); each pending row is classified and the pending row is deleted on promote/fragment/drop success.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReviewResponse {
    pub promoted: i64,
    pub fragmented: i64,
    pub dropped: i64,
}
