//! HTTP request/response types for Schedule endpoints (`WS7` §9).
//!
//! Hand-written local types — NOT generated from JSON Schema.
//! These are local-only; `nexus-platform` never observes them.
//!
//! Endpoints:
//! - `POST`   `/v1/daemon/orchestration/schedules`
//! - `GET`    `/v1/daemon/orchestration/schedules`
//! - `GET`    `/v1/daemon/orchestration/schedules/{schedule_id}`
//! - `PATCH`  `/v1/daemon/orchestration/schedules/{schedule_id}` — edit label/metadata (V1.171 P2 AR-29)
//! - `PATCH`  `/v1/daemon/orchestration/schedules/{schedule_id}/core-context`
//! - `GET`    `/v1/daemon/orchestration/schedules/{schedule_id}/core-context`
//! - `GET`    `/v1/daemon/orchestration/schedules/{schedule_id}/core-context-history`
//! - `POST`   `/v1/daemon/orchestration/schedules/{schedule_id}/signal`
//! - `DELETE` `/v1/daemon/orchestration/schedules/{schedule_id}`
//! - `GET`/`PUT` `/v1/daemon/works/{work_id}/cron` — per-Work cron config (V1.171 P2 AR-29)

use crate::generated::daemon_api::kb::pagination_info::PaginationInfo;
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
#[serde(rename_all = "snake_case")]
pub struct ListSchedulesQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Maximum number of items to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Opaque pagination cursor returned by the previous response's
    /// `pagination.next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Comma-separated sort terms (e.g. `-created_at`, `preset_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSchedulesResponse {
    pub items: Vec<ScheduleSummary>,
    /// Cursor-based pagination envelope.
    pub pagination: PaginationInfo,
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
// PATCH /schedules/{schedule_id} — Edit label/metadata (V1.171 P2 AR-29)
// ---------------------------------------------------------------------------

/// Partial edit of schedule metadata (AR-29). Only `label` is updateable
/// today; the `creator_schedules` table carries no other metadata columns.
/// Status/core-context have dedicated endpoints (AR-31).
///
/// Asymmetry vs [`AddScheduleRequest`]: on `POST /schedules` the label is
/// stored verbatim, while on this PATCH `""` is normalized to NULL, i.e. a
/// label cleared (never stored as an empty string).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditScheduleRequest {
    /// New label. `null` / absent → label unchanged. `Some("")` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// GET/PUT /v1/daemon/works/{work_id}/cron — per-Work cron config (V1.171 P2 AR-29)
// ---------------------------------------------------------------------------

/// One role's cron entry inside the per-Work schedule (spec §2.1 of
/// `cron-staggering.md`). Mirrors the shared `WorkSchedule` model in
/// `nexus-orchestration::schedule::work_schedule`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkCronRoleDto {
    /// 5-field cron expression (author local TZ).
    pub cron: String,
    /// Per-role opt-out without removing the schedule.
    pub enabled: bool,
}

/// The three-role staggering set (spec §2.1 `roles`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkCronRolesDto {
    /// `brainstorm` → `novel-brainstorm` preset.
    pub brainstorm: WorkCronRoleDto,
    /// `write` → `novel-write` preset.
    pub write: WorkCronRoleDto,
    /// `review` → `novel-review-master` preset.
    pub review: WorkCronRoleDto,
}

/// `GET /v1/daemon/works/{work_id}/cron` response — the effective per-Work
/// cron configuration.
///
/// Returns the stored `works.schedule_json` (or the spec defaults when
/// unset), plus an `is_default` marker so the UI can honestly say "using
/// defaults" (AR-29 / AR-30).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkCronResponse {
    /// IANA timezone string. Daemon converts to UTC for cron firing.
    pub tz: String,
    /// Per-role cron entries.
    pub roles: WorkCronRolesDto,
    /// `true` when `works.schedule_json` is unset/empty and this payload is
    /// the spec-default schedule; `false` once a schedule has been written.
    pub is_default: bool,
}

/// `PUT /v1/daemon/works/{work_id}/cron` request — full replacement of the
/// per-Work cron configuration.
///
/// The body is the complete `WorkSchedule` shape; `expected_current_json` is
/// an optional CAS pre-image — the exact stored `schedule_json` text that
/// must match for the write to apply. An empty/whitespace string means "the
/// stored config must currently be unset" (same as the default state);
/// omitting the field means an unconditional write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkCronRequest {
    /// IANA timezone string. Daemon converts to UTC for cron firing.
    pub tz: String,
    /// Per-role cron entries.
    pub roles: WorkCronRolesDto,
    /// CAS pre-image: the exact stored `schedule_json` blob. An
    /// empty/whitespace value means "must currently be unset (defaults)";
    /// `null`/absent means an unconditional write. Pass the value returned by
    /// a prior `GET` to guard against concurrent writers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_current_json: Option<String>,
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
            input: Some(input),
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

    #[test]
    fn edit_schedule_request_roundtrip() {
        let req = EditScheduleRequest {
            label: Some("edited label".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: EditScheduleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label.as_deref(), Some("edited label"));

        // Absent label deserializes as None (unchanged).
        let absent: EditScheduleRequest = serde_json::from_str("{}").unwrap();
        assert!(absent.label.is_none());
        // explicit null also None.
        let null_label: EditScheduleRequest = serde_json::from_str(r#"{"label":null}"#).unwrap();
        assert!(null_label.label.is_none());
    }

    #[test]
    fn work_cron_dto_roundtrip() {
        let req = UpdateWorkCronRequest {
            tz: "Asia/Shanghai".to_string(),
            roles: WorkCronRolesDto {
                brainstorm: WorkCronRoleDto {
                    cron: "0 9 * * *".to_string(),
                    enabled: true,
                },
                write: WorkCronRoleDto {
                    cron: "0 10 * * *".to_string(),
                    enabled: false,
                },
                review: WorkCronRoleDto {
                    cron: "0,30 * * * *".to_string(),
                    enabled: true,
                },
            },
            expected_current_json: Some(r#"{"tz":"UTC"}"#.to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: UpdateWorkCronRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tz, "Asia/Shanghai");
        assert_eq!(back.roles.brainstorm.cron, "0 9 * * *");
        assert!(!back.roles.write.enabled);
        assert_eq!(
            back.expected_current_json.as_deref(),
            Some(r#"{"tz":"UTC"}"#)
        );

        // expected_current_json is optional.
        let no_cas: UpdateWorkCronRequest = serde_json::from_str(
            r#"{"tz":"UTC","roles":{"brainstorm":{"cron":"0 3,9,15,21 * * *","enabled":true},"write":{"cron":"0 4,10,16,22 * * *","enabled":true},"review":{"cron":"0,30 * * * *","enabled":true}}}"#,
        )
        .unwrap();
        assert!(no_cas.expected_current_json.is_none());

        // GET response carries the is_default marker.
        let resp = WorkCronResponse {
            tz: "UTC".to_string(),
            roles: WorkCronRolesDto {
                brainstorm: WorkCronRoleDto {
                    cron: "0 3,9,15,21 * * *".to_string(),
                    enabled: true,
                },
                write: WorkCronRoleDto {
                    cron: "0 4,10,16,22 * * *".to_string(),
                    enabled: true,
                },
                review: WorkCronRoleDto {
                    cron: "0,30 * * * *".to_string(),
                    enabled: true,
                },
            },
            is_default: true,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: WorkCronResponse = serde_json::from_str(&json).unwrap();
        assert!(back.is_default);
        assert_eq!(back.roles.review.cron, "0,30 * * * *");
    }
}
