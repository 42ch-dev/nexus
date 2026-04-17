//! Schedule types for Creator Schedules (WS7).
//!
//! Multi-Schedule lifecycle per creator — queue of planned preset runs,
//! user-visible, CRUD-able. Per spec §3.1.
//!
//! Design: `.agents/plans/knowledge/creator-schedule-and-core-context-v1.md`

pub mod core_context;

use serde::{Deserialize, Serialize};

// Re-export core context types at the schedule module level
pub use core_context::*;

/// Schedule ID — ULID, user-addressable.
///
/// Wraps a ULID string. Pre-1.0: simple newtype wrapper around String
/// without the `ulid` crate for now; the ULID generation happens at the
/// caller site (supervisor / CLI command).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScheduleId(pub String);

/// Lifecycle status of a Schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    /// Not yet started; waiting on deps or user `schedule start`
    Pending,
    /// Current session is active
    Running,
    /// User paused; session may be Some (mid-execution) or None (pre-exec)
    Paused,
    /// Terminal success
    Completed,
    /// Terminal cancel (user or cascade)
    Cancelled,
    /// Terminal failure (preset hit unrecoverable error)
    Failed,
}

/// Concurrency declaration per Schedule.
///
/// Governs how this Schedule may run alongside other Schedules of the
/// same creator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleConcurrency {
    /// Default — queued behind existing non-terminal Schedules
    Serial,
    /// May run concurrently with these specific Schedules
    ParallelWith(Vec<ScheduleId>),
    /// May run concurrently with any sibling Schedule of this creator (escape hatch)
    ParallelAny,
}

/// A Creator Schedule — queue entry for a planned preset run.
///
/// One creator may have multiple Schedules. Each Schedule owns zero or
/// one active Session at a time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Schedule {
    /// ULID, user-addressable
    pub id: ScheduleId,
    pub creator_id: String,
    /// e.g. "novel-writing"
    pub preset_id: String,
    /// Snapshot of the preset at add-time
    pub preset_version: u32,
    pub status: ScheduleStatus,
    pub concurrency: ScheduleConcurrency,
    /// Predecessor Schedules (must be completed/cancelled before this starts)
    pub depends_on: Vec<ScheduleId>,
    /// Points at the current head of `core_context_versions`
    pub current_core_context_version: CoreContextVersion,
    /// Set while executing; None when pending/paused/completed
    pub current_session_id: Option<String>,
    /// Nullable; V1.4 ignored; V1.5 clock-trigger field
    pub scheduled_at: Option<String>,
    /// Optional user-friendly name
    pub label: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub terminated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_status_serialization() {
        assert_eq!(
            serde_json::to_string(&ScheduleStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&ScheduleStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&ScheduleStatus::Paused).unwrap(),
            "\"paused\""
        );
        assert_eq!(
            serde_json::to_string(&ScheduleStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&ScheduleStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
        assert_eq!(
            serde_json::to_string(&ScheduleStatus::Failed).unwrap(),
            "\"failed\""
        );
    }

    #[test]
    fn schedule_status_roundtrip() {
        for status in [
            ScheduleStatus::Pending,
            ScheduleStatus::Running,
            ScheduleStatus::Paused,
            ScheduleStatus::Completed,
            ScheduleStatus::Cancelled,
            ScheduleStatus::Failed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: ScheduleStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn schedule_concurrency_serial_roundtrip() {
        let serial = ScheduleConcurrency::Serial;
        let json = serde_json::to_string(&serial).unwrap();
        assert!(json.contains("\"kind\":\"serial\""));
        let back: ScheduleConcurrency = serde_json::from_str(&json).unwrap();
        assert_eq!(back, serial);

        let parallel_with = ScheduleConcurrency::ParallelWith(vec![ScheduleId(
            "01JMX00000000000000000001".to_string(),
        )]);
        let json = serde_json::to_string(&parallel_with).unwrap();
        assert!(json.contains("\"kind\":\"parallel_with\""));
        let back: ScheduleConcurrency = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parallel_with);

        let parallel_any = ScheduleConcurrency::ParallelAny;
        let json = serde_json::to_string(&parallel_any).unwrap();
        assert!(json.contains("\"kind\":\"parallel_any\""));
        let back: ScheduleConcurrency = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parallel_any);
    }

    #[test]
    fn schedule_id_roundtrip() {
        let id = ScheduleId("01JMXABCDEF00000000000001".to_string());
        let json = serde_json::to_string(&id).unwrap();
        let back: ScheduleId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
