//! Nexus Meta Schema
//!
//! Meta schema defining schema versioning and structure rules for all Nexus schemas
//!
//! @schema_version 1
//! @source meta.schema.json

use serde::{Deserialize, Serialize};

/// Nexus Meta Schema
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Meta {
    #[serde(rename = "$schema")]
    pub dollar_schema: String,
    #[serde(rename = "$id")]
    pub dollar_id: String,
    pub schema_version: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String,
}
