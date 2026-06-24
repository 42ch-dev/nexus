//! `Nexus` `Creator` `Entity`
//!
//! `Creator` entity - a first-class creative agent that can be user-owned or agent-registered. `Aligned` with data-model-v1.md §5.2.
//!
//! `@schema_version` 1
//! `@source` creator.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common::common_types::{CreatorStatus, RegistrationSource};

/// `Creator` entity - a first-class creative agent that can be user-owned or agent-registered. `Aligned` with data-model-v1.md §5.2.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Creator {
    pub schema_version: u32,
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub display_name: String,
    pub status: CreatorStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_platform_owned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_ref: Option<String>,
    pub registration_source: RegistrationSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_profile: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experience_revision: Option<u64>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
