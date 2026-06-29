//! `Nexus` `CreateWorkRequest`
//!
//! `Request` body for `POST` /v1/local/works.
//!
//! `@schema_version` 1
//! `@source` create-work-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/works.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkRequest {
    pub title: String,
    pub long_term_goal: String,
    pub initial_idea: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_preset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage_from_work_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_pool_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_profile: Option<String>,
}
