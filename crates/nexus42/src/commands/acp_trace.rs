//! CLI-local trace writer helpers for ACP run correlation.
//!
//! Small, focused module that generates correlation IDs and appends trace
//! events to per-run JSONL files. Trace writes are best-effort (non-fatal).

use std::io::Write;
use std::path::Path;

// Re-export trace DTOs for use by other command modules
pub use nexus_contracts::local::acp_runtime::trace::{
    NexusCapabilityCallId, NexusRunId, NexusTraceEvent,
};
use nexus_home_layout::validate_run_id_safe;

/// Generate a new run-level correlation ID: `run_<uuid32>`.
#[must_use]
pub fn generate_run_id() -> NexusRunId {
    NexusRunId(format!("run_{}", uuid::Uuid::new_v4().simple()))
}

/// Generate a new capability-call-level correlation ID: `cap_<uuid32>`.
#[must_use]
pub fn generate_capability_call_id() -> NexusCapabilityCallId {
    NexusCapabilityCallId(format!("cap_{}", uuid::Uuid::new_v4().simple()))
}

/// Append a trace event to the run's JSONL file.
///
/// Creates parent directories as needed. Validates the run ID for path safety.
/// Trace write failures are non-fatal — logs a warning and returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err` only if the run ID fails path-safety validation.
/// I/O failures during write are logged and swallowed (best-effort).
pub fn append_trace_event(
    home: &Path,
    run_id: &NexusRunId,
    event: &NexusTraceEvent,
) -> std::result::Result<(), String> {
    validate_run_id_safe(&run_id.0)?;

    let trace_file = nexus_home_layout::acp_run_trace_file(home, &run_id.0);

    // Best-effort directory + file write
    if let Err(e) = std::fs::create_dir_all(
        trace_file
            .parent()
            .unwrap_or_else(|| std::path::Path::new("")),
    ) {
        eprintln!("Warning: failed to create trace directory: {e}");
        return Ok(());
    }

    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trace_file)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Warning: failed to open trace file: {e}");
            return Ok(());
        }
    };

    let mut line = match serde_json::to_string(event) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Warning: failed to serialize trace event: {e}");
            return Ok(());
        }
    };
    line.push('\n');

    if let Err(e) = file.write_all(line.as_bytes()) {
        eprintln!("Warning: failed to write trace event: {e}");
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_contracts::local::acp_runtime::trace::{
        NexusRunFinished, NexusRunStarted, NexusTraceStatus,
    };

    fn now_rfc3339() -> String {
        "2026-05-13T12:00:00Z".to_string()
    }

    #[test]
    fn generate_run_id_format() {
        let id = generate_run_id();
        assert!(id.0.starts_with("run_"));
        assert_eq!(id.0.len(), 4 + 32, "run_ prefix + 32 hex chars");
        // All hex after prefix
        assert!(id.0[4..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_capability_call_id_format() {
        let id = generate_capability_call_id();
        assert!(id.0.starts_with("cap_"));
        assert_eq!(id.0.len(), 4 + 32, "cap_ prefix + 32 hex chars");
        assert!(id.0[4..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn append_trace_event_writes_jsonl() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        let run_id = NexusRunId("run_testappend00000000000000000001".to_string());

        let event1 = NexusTraceEvent::RunStarted(NexusRunStarted {
            schema_version: 1,
            run_id: run_id.clone(),
            entrypoint: "acp.run".to_string(),
            agent_id: None,
            cwd: None,
            pid: None,
            timestamp: now_rfc3339(),
        });

        let event2 = NexusTraceEvent::RunFinished(NexusRunFinished {
            schema_version: 1,
            run_id: run_id.clone(),
            status: NexusTraceStatus::Completed,
            error: None,
            timestamp: now_rfc3339(),
        });

        append_trace_event(home, &run_id, &event1).unwrap();
        append_trace_event(home, &run_id, &event2).unwrap();

        let trace_path = nexus_home_layout::acp_run_trace_file(home, &run_id.0);
        let content = std::fs::read_to_string(&trace_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "should have 2 JSONL lines");

        // Each line is valid JSON
        let parsed1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed1["event"], "run_started");
        let parsed2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(parsed2["event"], "run_finished");
    }

    #[test]
    fn append_trace_event_rejects_invalid_run_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        let bad_id = NexusRunId("../../etc/passwd".to_string());

        let event = NexusTraceEvent::RunStarted(NexusRunStarted {
            schema_version: 1,
            run_id: bad_id.clone(),
            entrypoint: "acp.run".to_string(),
            agent_id: None,
            cwd: None,
            pid: None,
            timestamp: now_rfc3339(),
        });

        let result = append_trace_event(home, &bad_id, &event);
        assert!(result.is_err(), "should reject path-traversal run ID");
    }
}
