//! Local trace DTOs for ACP run and capability-call correlation.
//!
//! These types are owned by the Nexus CLI/daemon layer and are never observed
//! by `nexus-platform`. Stored in per-run JSONL files under
//! `~/.nexus42/acp/runs/<run_id>/trace.jsonl`.
// invariant: never observed by `nexus-platform`; the daemon owns these files.

use serde::{Deserialize, Serialize};

/// note: opaque run-level correlation identifier.
///
/// Generated form: `run_<32 lowercase hex chars>` (UUID v4 simple).
/// Users may provide their own via `--run-id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct NexusRunId(pub String);

/// note: opaque capability-call-level correlation identifier.
///
/// Generated form: `cap_<32 lowercase hex chars>` (UUID v4 simple).
/// Unique within a run; always paired with a parent `NexusRunId`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct NexusCapabilityCallId(pub String);

/// note: status value for `run_finished` and `capability_call_finished` events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NexusTraceStatus {
    /// note: completed successfully.
    Completed,
    /// note: finished with an error.
    Failed,
    /// note: cancelled by user (Ctrl+C) or external signal.
    Cancelled,
    /// note: exceeded allowed execution time.
    Timeout,
}

impl std::fmt::Display for NexusTraceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

/// note: trace event emitted when a run starts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct NexusRunStarted {
    /// note: schema version for forward-compatible parsing.
    pub schema_version: u32,
    /// note: the run's correlation ID.
    pub run_id: NexusRunId,
    /// note: what CLI entrypoint initiated this run (e.g. `acp.run`, `daemon.orchestrate.run`).
    pub entrypoint: String,
    /// note: agent identifier, if known at start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// note: working directory of the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// note: process ID of the CLI or daemon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// note: ISO 8601 / RFC 3339 timestamp.
    pub timestamp: String,
}

/// note: trace event emitted when a run finishes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct NexusRunFinished {
    /// note: schema version for forward-compatible parsing.
    pub schema_version: u32,
    /// note: the run's correlation ID.
    pub run_id: NexusRunId,
    /// note: final status of the run.
    pub status: NexusTraceStatus,
    /// note: error message, if the run failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// note: ISO 8601 / RFC 3339 timestamp.
    pub timestamp: String,
}

/// note: trace event emitted when a capability call starts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct NexusCapabilityCallStarted {
    /// note: schema version for forward-compatible parsing.
    pub schema_version: u32,
    /// note: parent run correlation ID.
    pub run_id: NexusRunId,
    /// note: this capability call's correlation ID.
    pub capability_call_id: NexusCapabilityCallId,
    /// note: name of the capability being invoked (e.g. `acp.prompt`).
    pub capability_name: String,
    /// note: ACP session ID, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// note: orchestration task ID, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// note: worker request ID, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_request_id: Option<String>,
    /// note: ISO 8601 / RFC 3339 timestamp.
    pub timestamp: String,
}

/// note: trace event emitted when a capability call finishes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct NexusCapabilityCallFinished {
    /// note: schema version for forward-compatible parsing.
    pub schema_version: u32,
    /// note: parent run correlation ID.
    pub run_id: NexusRunId,
    /// note: this capability call's correlation ID.
    pub capability_call_id: NexusCapabilityCallId,
    /// note: final status of the capability call.
    pub status: NexusTraceStatus,
    /// note: error message, if the call failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// note: ISO 8601 / RFC 3339 timestamp.
    pub timestamp: String,
}

/// note: tagged trace event — one JSONL line per event.
///
/// The `event` tag uses `snake_case` strings for forward-compatible parsing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum NexusTraceEvent {
    /// note: a run has started.
    RunStarted(NexusRunStarted),
    /// note: a run has finished.
    RunFinished(NexusRunFinished),
    /// note: a capability call has started.
    CapabilityCallStarted(NexusCapabilityCallStarted),
    /// note: a capability call has finished.
    CapabilityCallFinished(NexusCapabilityCallFinished),
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn run_id() -> NexusRunId {
        NexusRunId("run_abcdef0123456789abcdef0123456789".to_string())
    }

    fn cap_id() -> NexusCapabilityCallId {
        NexusCapabilityCallId("cap_1234567890abcdef1234567890abcdef".to_string())
    }

    fn now_rfc3339() -> String {
        "2026-05-13T12:00:00Z".to_string()
    }

    #[test]
    fn run_id_serializes_transparently() {
        let id = NexusRunId("run_abc".to_string());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"run_abc\"");
        let back: NexusRunId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn capability_call_id_serializes_transparently() {
        let id = NexusCapabilityCallId("cap_xyz".to_string());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"cap_xyz\"");
        let back: NexusCapabilityCallId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn status_roundtrip() {
        let statuses = [
            NexusTraceStatus::Completed,
            NexusTraceStatus::Failed,
            NexusTraceStatus::Cancelled,
            NexusTraceStatus::Timeout,
        ];
        for s in &statuses {
            let json = serde_json::to_string(s).unwrap();
            let back: NexusTraceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, s);
        }
    }

    #[test]
    fn run_started_roundtrip() {
        let event = NexusTraceEvent::RunStarted(NexusRunStarted {
            schema_version: 1,
            run_id: run_id(),
            entrypoint: "acp.run".to_string(),
            agent_id: Some("claude-acp".to_string()),
            cwd: None,
            pid: Some(1234),
            timestamp: now_rfc3339(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains("\"event\":\"run_started\""),
            "must use snake_case tag"
        );
        assert!(json.contains("\"run_id\""));
        assert!(json.contains("\"entrypoint\""));
        let back: NexusTraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn run_finished_roundtrip() {
        let event = NexusTraceEvent::RunFinished(NexusRunFinished {
            schema_version: 1,
            run_id: run_id(),
            status: NexusTraceStatus::Completed,
            error: None,
            timestamp: now_rfc3339(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains("\"event\":\"run_finished\""),
            "must use snake_case tag"
        );
        let back: NexusTraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn capability_call_started_roundtrip() {
        let event = NexusTraceEvent::CapabilityCallStarted(NexusCapabilityCallStarted {
            schema_version: 1,
            run_id: run_id(),
            capability_call_id: cap_id(),
            capability_name: "acp.prompt".to_string(),
            session_id: None,
            task_id: None,
            worker_request_id: None,
            timestamp: now_rfc3339(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains("\"event\":\"capability_call_started\""),
            "must use snake_case tag"
        );
        let back: NexusTraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn capability_call_finished_roundtrip() {
        let event = NexusTraceEvent::CapabilityCallFinished(NexusCapabilityCallFinished {
            schema_version: 1,
            run_id: run_id(),
            capability_call_id: cap_id(),
            status: NexusTraceStatus::Failed,
            error: Some("something went wrong".to_string()),
            timestamp: now_rfc3339(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains("\"event\":\"capability_call_finished\""),
            "must use snake_case tag"
        );
        let back: NexusTraceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn optional_fields_skip_when_none() {
        let event = NexusTraceEvent::RunStarted(NexusRunStarted {
            schema_version: 1,
            run_id: run_id(),
            entrypoint: "acp.run".to_string(),
            agent_id: None,
            cwd: None,
            pid: None,
            timestamp: now_rfc3339(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("agent_id"), "None fields should be skipped");
        assert!(!json.contains("cwd"));
        assert!(!json.contains("pid"));
    }
}
