//! Nexus SyncCommand
//!
//! SyncCommand entity representing a business action with audit attribution. Aligned with data-model-v1.md §5.10.
//!
//! @schema_version 1
//! @source sync-command.schema.json

use crate::generated::common_types::{CommandOrigin, CommandStatus, CommandType};
use serde::{Deserialize, Serialize};

/// SyncCommand entity representing a business action with audit attribution. Aligned with data-model-v1.md §5.10.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SyncCommand {
    pub schema_version: u32,
    pub command_id: String,
    pub workspace_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub command_type: CommandType,
    pub origin: CommandOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_manuscript: Option<bool>,
    pub status: CommandStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub created_at: String,
}
