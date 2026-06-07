//! HTTP request/response types for Schedule endpoints (`WS7` §9).
//!
//! Hand-written local types — NOT generated from JSON Schema.
//! These are local-only; `nexus-platform` never observes them.
//!
//! Endpoints:
//! - `POST`   `/v1/local/orchestration/schedules`
//! - `GET`    `/v1/local/orchestration/schedules`
//! - `GET`    `/v1/local/orchestration/schedules/{schedule_id}`
//! - `PATCH`  `/v1/local/orchestration/schedules/{schedule_id}/core-context`
//! - `GET`    `/v1/local/orchestration/schedules/{schedule_id}/core-context`
//! - `GET`    `/v1/local/orchestration/schedules/{schedule_id}/core-context-history`
//! - `POST`   `/v1/local/orchestration/schedules/{schedule_id}/signal`
//! - `DELETE` `/v1/local/orchestration/schedules/{schedule_id}`

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// POST /schedules — Add Schedule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddScheduleRequest {
    pub creator_id: String,
    pub preset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<ScheduleConcurrencyRequest>,
    /// `V1.5` `WS-D`: `scheduled_at` as Unix timestamp (string for JSON compatibility).
    /// Accepts `ISO-8601` datetime in `CLI`; HTTP accepts Unix timestamp string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<String>,
    /// Structured input context for the preset (V1.37 R-V136P1-01).
    ///
    /// Carries `novel-project-init` grill-me answers (`work_ref`,
    /// `total_planned_chapters`, `world_id`, `title`) and other preset-specific
    /// key-value pairs into `preset.input.*` for scaffold and prompt rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    /// Force bypass of preset gate evaluation (V1.37 §7.9).
    ///
    /// When `true`, gate evaluation is skipped, an audit row is persisted,
    /// and the schedule is created normally. Requires `reason` to be set.
    #[serde(default)]
    pub force_gates: bool,
    /// Audit reason for `force_gates` (required when `force_gates` is `true`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleConcurrencyRequest {
    Serial,
    ParallelWith { schedule_ids: Vec<String> },
    ParallelAny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddScheduleResponse {
    pub schedule_id: String,
    pub status: String,
    pub core_context_version: u32,
}

// ---------------------------------------------------------------------------
// GET /schedules — List Schedules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSchedulesQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSchedulesResponse {
    pub schedules: Vec<ScheduleSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleSummary {
    pub schedule_id: String,
    pub creator_id: String,
    pub preset_id: String,
    pub status: String,
    pub label: Option<String>,
    pub current_core_context_version: u32,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// GET /schedules/{schedule_id} — Inspect Schedule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectScheduleResponse {
    pub schedule: ScheduleSummary,
    pub depends_on: Vec<String>,
    pub concurrency_kind: String,
}

// ---------------------------------------------------------------------------
// PATCH /schedules/{schedule_id}/core-context — Apply EditOp
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditCoreContextRequest {
    /// Edit operation kind: `append`, `replace`, `struct_merge`, `struct_remove`.
    pub op: String,
    /// Body text (for `append`/`replace`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Patch JSON (for `struct_merge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<serde_json::Value>,
    /// Key path (for `struct_remove`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditCoreContextResponse {
    pub new_version: u32,
}

// ---------------------------------------------------------------------------
// GET /schedules/{schedule_id}/core-context — Current content
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreContextResponse {
    pub version: u32,
    pub payload_kind: String,
    pub content: serde_json::Value,
    pub derivation_kind: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// GET /schedules/{schedule_id}/core-context-history — Version history
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreContextHistoryResponse {
    pub entries: Vec<CoreContextHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreContextHistoryEntry {
    pub version: u32,
    pub payload_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    pub derivation_kind: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// POST /schedules/{schedule_id}/signal — Pause/Resume/Cancel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalScheduleRequest {
    pub signal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalScheduleResponse {
    pub schedule_id: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// DELETE /schedules/{schedule_id} — Remove Schedule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteScheduleResponse {
    pub deleted: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_schedule_request_roundtrip() {
        let req = AddScheduleRequest {
            creator_id: "c1".to_string(),
            preset_id: "novel-writing".to_string(),
            seed: Some("topic=bees".to_string()),
            label: Some("demo".to_string()),
            depends_on: None,
            concurrency: None,
            scheduled_at: None,
            input: None,
            force_gates: false,
            reason: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: AddScheduleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.creator_id, "c1");
        assert_eq!(back.seed, Some("topic=bees".to_string()));
        assert!(back.input.is_none());
        assert!(!back.force_gates);
    }

    #[test]
    fn add_schedule_request_with_input() {
        let input = serde_json::json!({
            "work_ref": "my-novel",
            "total_planned_chapters": 12,
            "title": "The Great Novel"
        });
        let req = AddScheduleRequest {
            creator_id: "c1".to_string(),
            preset_id: "novel-project-init".to_string(),
            seed: None,
            label: None,
            depends_on: None,
            concurrency: None,
            scheduled_at: None,
            input: Some(input.clone()),
            force_gates: false,
            reason: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: AddScheduleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.input.unwrap()["work_ref"], "my-novel");
    }

    #[test]
    fn add_schedule_request_with_force_gates() {
        let req = AddScheduleRequest {
            creator_id: "c1".to_string(),
            preset_id: "novel-writing".to_string(),
            seed: None,
            label: None,
            depends_on: None,
            concurrency: None,
            scheduled_at: None,
            input: None,
            force_gates: true,
            reason: Some("testing override".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: AddScheduleRequest = serde_json::from_str(&json).unwrap();
        assert!(back.force_gates);
        assert_eq!(back.reason.unwrap(), "testing override");
    }

    #[test]
    fn add_schedule_request_with_scheduled_at() {
        let req = AddScheduleRequest {
            creator_id: "c2".to_string(),
            preset_id: "novel-writing".to_string(),
            seed: None,
            label: None,
            depends_on: None,
            concurrency: None,
            scheduled_at: Some("253402300799".to_string()), // Unix timestamp
            input: None,
            force_gates: false,
            reason: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"scheduled_at\":\"253402300799\""));
        let back: AddScheduleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.scheduled_at, Some("253402300799".to_string()));
    }

    #[test]
    fn signal_schedule_request_roundtrip() {
        let req = SignalScheduleRequest {
            signal: "pause".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: SignalScheduleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.signal, "pause");
    }

    #[test]
    fn edit_core_context_request_all_ops() {
        // append
        let req = EditCoreContextRequest {
            op: "append".to_string(),
            body: Some("more".to_string()),
            patch: None,
            path: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"append\""));

        // struct_merge
        let req = EditCoreContextRequest {
            op: "struct_merge".to_string(),
            body: None,
            patch: Some(serde_json::json!({"key": "val"})),
            path: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"struct_merge\""));

        // struct_remove
        let req = EditCoreContextRequest {
            op: "struct_remove".to_string(),
            body: None,
            patch: None,
            path: Some("key".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"struct_remove\""));
    }

    #[test]
    fn list_schedules_query_defaults() {
        let json = "{}";
        let q: ListSchedulesQuery = serde_json::from_str(json).unwrap();
        assert!(q.creator_id.is_none());
        assert!(q.status.is_none());
    }
}
