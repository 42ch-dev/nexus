//! `Nexus` `PatchWorkRequest`
//!
//! `Request` body for `PATCH` /v1/local/works/{`work_id`}.
//!
//! `@schema_version` 1
//! `@source` patch-work-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `PATCH` /v1/local/works/{`work_id`}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PatchWorkRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_term_goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creative_brief: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intake_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_preset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_review_master_on_timeout: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_chain_interrupted: Option<bool>,
}
