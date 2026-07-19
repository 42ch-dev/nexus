//! `Nexus` `TimelineOverviewResponse`
//!
//! `Cursor`-paginated overview of visible `Worlds` with per-`World` era/event counts and last activity timestamp. `Response` for `GET` /v1/daemon/timeline/overview.
//!
//! `@schema_version` 1
//! `@source` timeline-overview-response.schema.json

use serde::{Deserialize, Serialize};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TimelineOverviewResponseWorld {
    pub world_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub era_count: u64,
    pub event_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_at: Option<String>,
}
/// `Cursor`-paginated overview of visible `Worlds` with per-`World` era/event counts and last activity timestamp. `Response` for `GET` /v1/daemon/timeline/overview.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TimelineOverviewResponse {
    pub worlds: Vec<TimelineOverviewResponseWorld>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub total_worlds: u64,
}
