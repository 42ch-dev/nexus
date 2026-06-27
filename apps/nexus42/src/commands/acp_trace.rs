//! CLI-local trace writer helpers for ACP run correlation.
//!
//! Small, focused module that generates correlation IDs and appends trace
//! events to per-run JSONL files. Trace writes are best-effort (non-fatal).
//!
//! The writer uses a process-wide singleton `BufWriter` behind a mutex to
//! avoid per-event open/close overhead and to serialize concurrent writes.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use tracing::warn;

// Re-export trace DTOs for use by other command modules
pub use nexus_contracts::local::acp_runtime::trace::{
    NexusCapabilityCallId, NexusRunId, NexusTraceEvent,
};
use nexus_home_layout::validate_run_id_safe;

/// Process-wide singleton trace writer.
///
/// Holds an optional `TraceWriter` behind a mutex. The first trace event
/// for a given run creates the writer; subsequent events reuse it. The
/// mutex serializes concurrent writes within the same process.
static TRACE_WRITER: LazyLock<Mutex<Option<TraceWriter>>> = LazyLock::new(|| Mutex::new(None));

/// Maximum trace file size in bytes (10 MB). When the file exceeds this
/// limit it is truncated and writing starts from the beginning.
const TRACE_FILE_SIZE_LIMIT: u64 = 10 * 1024 * 1024;

/// Buffered trace writer that keeps the file handle open for the
/// lifetime of a single CLI run.
struct TraceWriter {
    writer: BufWriter<std::fs::File>,
    path: PathBuf,
    /// Approximate bytes written since open — used for size-limit checks
    /// without re-statting the file on every event.
    bytes_written: u64,
}

impl TraceWriter {
    /// Open (or create) the trace file and wrap it in a buffered writer.
    /// Creates parent directories as needed. If the existing file exceeds
    /// `TRACE_FILE_SIZE_LIMIT`, it is truncated to zero length.
    fn open(trace_file: &Path) -> std::result::Result<Self, std::io::Error> {
        if let Some(parent) = trace_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Check existing file size; truncate if over the limit.
        let existing_size = std::fs::metadata(trace_file).map_or(0, |m| m.len());

        let file = if existing_size > TRACE_FILE_SIZE_LIMIT {
            // Truncate: open without append, which resets to zero length.
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(trace_file)?
        } else {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(trace_file)?
        };

        Ok(Self {
            writer: BufWriter::new(file),
            path: trace_file.to_path_buf(),
            bytes_written: 0,
        })
    }

    /// Write a single JSONL line and flush.
    fn write_event(&mut self, event: &NexusTraceEvent) -> std::result::Result<(), std::io::Error> {
        let mut line = serde_json::to_string(event)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;
        self.bytes_written += line.len() as u64;
        Ok(())
    }
}

impl Drop for TraceWriter {
    fn drop(&mut self) {
        // Best-effort final flush on drop
        let _ = self.writer.flush();
    }
}

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
/// Uses a process-wide buffered singleton writer to avoid per-event
/// open/close overhead. Concurrent writes are serialized via a mutex.
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

    // Recover from mutex poisoning (a previous panic left the lock tainted)
    // rather than propagating the error — trace writes are best-effort.
    let mut guard = TRACE_WRITER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // If the singleton is empty or points to a different file, (re)open it.
    let needs_reopen = guard.as_ref().is_none_or(|w| w.path != trace_file);

    if needs_reopen {
        match TraceWriter::open(&trace_file) {
            Ok(w) => *guard = Some(w),
            Err(e) => {
                warn!("failed to open trace file: {e}");
                return Ok(());
            }
        }
    }

    if let Some(writer) = guard.as_mut() {
        if let Err(e) = writer.write_event(event) {
            warn!("failed to write trace event: {e}");
        }
    }
    drop(guard);

    Ok(())
}

/// Flush and release the singleton trace writer.
///
/// Call this at the end of a CLI run to ensure all buffered data is
/// written and the file handle is released. Safe to call multiple times.
pub fn flush_trace_writer() {
    let mut guard = TRACE_WRITER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = None;
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

    /// Reset the singleton trace writer so each test starts clean.
    /// Must run with --test-threads=1 to avoid cross-test interference.
    fn reset_trace_writer() {
        let mut guard = TRACE_WRITER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = None;
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
        reset_trace_writer();
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
        flush_trace_writer();

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
        reset_trace_writer();
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

    #[test]
    fn singleton_writer_reuses_file_handle() {
        reset_trace_writer();
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        let run_id = NexusRunId("run_singleton00000000000000000001".to_string());

        let event = NexusTraceEvent::RunStarted(NexusRunStarted {
            schema_version: 1,
            run_id: run_id.clone(),
            entrypoint: "acp.run".to_string(),
            agent_id: None,
            cwd: None,
            pid: None,
            timestamp: now_rfc3339(),
        });

        // First call creates the writer
        append_trace_event(home, &run_id, &event).unwrap();

        // Verify the singleton holds the writer pointing to the correct file
        let guard = TRACE_WRITER.lock().unwrap();
        assert!(
            guard.is_some(),
            "singleton should be populated after first write"
        );
        let expected_path = nexus_home_layout::acp_run_trace_file(home, &run_id.0);
        assert_eq!(guard.as_ref().unwrap().path, expected_path);
        drop(guard);

        // Second call to the same file reuses the writer
        append_trace_event(home, &run_id, &event).unwrap();
        flush_trace_writer();

        let trace_path = nexus_home_layout::acp_run_trace_file(home, &run_id.0);
        let content = std::fs::read_to_string(&trace_path).unwrap();
        assert_eq!(content.lines().count(), 2, "both events should be written");
    }

    #[test]
    fn trace_file_size_limit_truncates_oversized_file() {
        reset_trace_writer();
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        let run_id = NexusRunId("run_sizelimit0000000000000000001".to_string());
        let trace_path = nexus_home_layout::acp_run_trace_file(home, &run_id.0);

        // Pre-create a trace file that exceeds the size limit
        if let Some(parent) = trace_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        // Write TRACE_FILE_SIZE_LIMIT + 1 bytes of padding
        // SAFE: constant is 10 MiB, always fits in usize
        #[allow(clippy::cast_possible_truncation)]
        let oversize_content = "X".repeat((TRACE_FILE_SIZE_LIMIT + 1) as usize);
        std::fs::write(&trace_path, oversize_content.as_bytes()).unwrap();
        assert!(
            std::fs::metadata(&trace_path).unwrap().len() > TRACE_FILE_SIZE_LIMIT,
            "pre-condition: file should exceed limit"
        );

        // Now write a trace event — the oversized file should be truncated
        let event = NexusTraceEvent::RunStarted(NexusRunStarted {
            schema_version: 1,
            run_id: run_id.clone(),
            entrypoint: "acp.run".to_string(),
            agent_id: None,
            cwd: None,
            pid: None,
            timestamp: now_rfc3339(),
        });

        append_trace_event(home, &run_id, &event).unwrap();
        flush_trace_writer();

        let file_size = std::fs::metadata(&trace_path).unwrap().len();
        assert!(
            file_size <= TRACE_FILE_SIZE_LIMIT,
            "file should be capped at the limit after truncation, got {file_size} bytes"
        );

        // The truncated file should contain exactly one JSONL line
        let content = std::fs::read_to_string(&trace_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1, "truncated file should have exactly 1 event");
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["event"], "run_started");
    }
}
