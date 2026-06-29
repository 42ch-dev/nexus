//! `Nexus` `WorkDetailResponse`
//!
//! `Response` for `GET` /v1/local/works/{`work_id`} — full work detail.
//!
//! `@schema_version` 1
//! `@source` work-detail-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/works/{`work_id`} — full work detail.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorkDetailResponse {
    pub work_id: String,
    pub status: String,
    pub title: String,
    pub long_term_goal: String,
    pub initial_idea: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creative_brief: Option<serde_json::Value>,
    pub intake_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_ref: Option<String>,
    pub inspiration_log: Vec<serde_json::Value>,
    pub primary_preset_id: String,
    pub schedule_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub current_stage: String,
    pub stage_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_planned_chapters: Option<i64>,
    pub current_chapter: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_chapter: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_chapter_volume: Option<i64>,
    pub auto_chain_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_schedule_id: Option<String>,
    pub auto_chain_interrupted: bool,
    pub auto_review_master_on_timeout: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_lock_holder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_lock_acquired_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_locked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub novel_completion_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage_from_work_id: Option<String>,
}
