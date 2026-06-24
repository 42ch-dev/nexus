//! `Nexus` `CoreContextHistoryResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/schedules/{`schedule_id`}/core-context-history.
//!
//! `@schema_version` 1
//! `@source` core-context-history-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::schedule::core_context_history_entry::CoreContextHistoryEntry;

/// `Response` for `GET` /v1/local/orchestration/schedules/{`schedule_id`}/core-context-history.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CoreContextHistoryResponse {
    pub entries: Vec<CoreContextHistoryEntry>,
}
