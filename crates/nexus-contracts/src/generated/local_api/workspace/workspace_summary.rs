//! `Nexus` `WorkspaceSummary`
//!
//! `Summary` row for a workspace in list responses.
//!
//! `@schema_version` 1
//! `@source` workspace-summary.schema.json

use serde::{Deserialize, Serialize};

/// `Summary` row for a workspace in list responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceSummary {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}
