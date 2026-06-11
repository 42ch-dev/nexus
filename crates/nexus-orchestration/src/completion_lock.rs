//! Completion-lock file I/O for multi-Work lifecycle (DF-60 §3).
//!
//! The lock file lives at `Works/<work_ref>/.completion-lock.json`.
//! Operations use atomic tmp+rename to avoid partial writes.
//!
//! # Source-of-truth (SSOT) declaration
//!
//! **DB column `works.completion_locked_at` is the authoritative lock state.**
//! The `.completion-lock.json` file is a derived artifact for cross-tool
//! observation. If the file exists but the DB column is NULL, the supervisor
//! treats the work as unlocked. If the file is missing but the DB column is
//! set, the supervisor treats the work as locked.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Current schema version for the completion-lock file.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Completion-lock payload written to disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionLock {
    /// Schema version for forward-compatibility. Defaults to 1.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// The Work ID this lock protects.
    pub work_id: String,
    /// ISO-8601 timestamp when the lock was applied.
    pub locked_at: String,
    /// Reason for the lock (currently always `"completion"`).
    pub reason: String,
}

const fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

/// File name used inside the Work directory.
const LOCK_FILE_NAME: &str = ".completion-lock.json";

/// Return the path to the completion-lock file for a given Work.
///
/// ```
/// # use nexus_orchestration::completion_lock::completion_lock_path;
/// assert_eq!(
///     completion_lock_path(std::path::Path::new("/ws"), "my-novel"),
///     std::path::Path::new("/ws/Works/my-novel/.completion-lock.json")
/// );
/// ```
#[must_use]
pub fn completion_lock_path(workspace_dir: &Path, work_ref: &str) -> std::path::PathBuf {
    workspace_dir
        .join("Works")
        .join(work_ref)
        .join(LOCK_FILE_NAME)
}

/// Write a completion-lock file atomically (tmp + rename).
///
/// # Errors
///
/// Returns `std::io::Error` if the parent directory doesn't exist,
/// the tmp write fails, or the rename fails.
pub fn write_completion_lock(
    workspace_dir: &Path,
    work_ref: &str,
    lock: &CompletionLock,
) -> Result<(), std::io::Error> {
    let path = completion_lock_path(workspace_dir, work_ref);
    let json = serde_json::to_string_pretty(lock)?;

    // Write to temp file first, then atomic rename.
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &path)?;

    Ok(())
}

/// Read the completion-lock file if present.
///
/// Returns `Ok(Some(lock))` if the file exists and parses correctly,
/// `Ok(None)` if the file does not exist, or `Err` on I/O / parse failure.
///
/// # Errors
///
/// Returns `std::io::Error` if the file exists but cannot be read or parsed.
pub fn read_completion_lock(
    workspace_dir: &Path,
    work_ref: &str,
) -> Result<Option<CompletionLock>, std::io::Error> {
    let path = completion_lock_path(workspace_dir, work_ref);

    if !path.exists() {
        return Ok(None);
    }

    let data = std::fs::read_to_string(&path)?;
    let mut lock: CompletionLock = serde_json::from_str(&data).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid completion-lock JSON: {e}"),
        )
    })?;

    // Backward-compat: missing schema_version treated as 1 (via serde default).
    // Forward-compat: reject unknown future versions.
    if lock.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "completion-lock schema_version {} is newer than supported {}; \
                 upgrade the CLI to handle this lock file",
                lock.schema_version, CURRENT_SCHEMA_VERSION
            ),
        ));
    }

    // Normalize missing schema_version (pre-V1.41 lock files) to 1.
    if lock.schema_version == 0 {
        lock.schema_version = 1;
    }

    Ok(Some(lock))
}

/// Remove the completion-lock file.
///
/// # SSOT declaration
///
/// **DB column `works.completion_locked_at` is the authoritative lock state.**
/// This function removes the on-disk artifact only. The caller is responsible
/// for clearing the DB column in a coordinated operation. If the file deletion
/// fails after the DB column is cleared, the stale file is harmless — the
/// supervisor gates on the DB column, not the file.
///
/// # Errors
///
/// Returns `std::io::Error` if the file exists but cannot be removed.
/// Returns `Ok(())` if the file does not exist (idempotent).
pub fn release_completion_lock(workspace_dir: &Path, work_ref: &str) -> Result<(), std::io::Error> {
    let path = completion_lock_path(workspace_dir, work_ref);

    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn completion_lock_path_format() {
        let p = completion_lock_path(Path::new("/ws"), "my-novel");
        assert_eq!(p, PathBuf::from("/ws/Works/my-novel/.completion-lock.json"));
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        let work_dir = ws.join("Works").join("test-novel");
        std::fs::create_dir_all(&work_dir).unwrap();

        let lock = CompletionLock {
            schema_version: 1,
            work_id: "wrk_001".to_string(),
            locked_at: "2026-06-10T12:00:00Z".to_string(),
            reason: "completion".to_string(),
        };

        write_completion_lock(ws, "test-novel", &lock).unwrap();
        let read = read_completion_lock(ws, "test-novel")
            .unwrap()
            .expect("lock should exist");
        assert_eq!(read, lock);
    }

    #[test]
    fn read_missing_schema_version_treated_as_v1() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        let work_dir = ws.join("Works").join("test-novel");
        std::fs::create_dir_all(&work_dir).unwrap();

        // Write a lock file without schema_version (pre-V1.41 format)
        let legacy_json =
            r#"{"work_id":"wrk_old","locked_at":"2026-06-10T10:00:00Z","reason":"completion"}"#;
        let lock_path = completion_lock_path(ws, "test-novel");
        std::fs::write(&lock_path, legacy_json).unwrap();

        let lock = read_completion_lock(ws, "test-novel")
            .unwrap()
            .expect("lock should parse");
        assert_eq!(lock.schema_version, 1);
        assert_eq!(lock.work_id, "wrk_old");
    }

    #[test]
    fn read_future_schema_version_returns_error() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        let work_dir = ws.join("Works").join("test-novel");
        std::fs::create_dir_all(&work_dir).unwrap();

        let future_json = r#"{"schema_version":99,"work_id":"wrk_future","locked_at":"2030-01-01T00:00:00Z","reason":"completion"}"#;
        let lock_path = completion_lock_path(ws, "test-novel");
        std::fs::write(&lock_path, future_json).unwrap();

        let result = read_completion_lock(ws, "test-novel");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("schema_version 99"),
            "should mention future version: {err_msg}"
        );
    }

    #[test]
    fn read_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = read_completion_lock(dir.path(), "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn release_removes_file() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        let work_dir = ws.join("Works").join("test-novel");
        std::fs::create_dir_all(&work_dir).unwrap();

        let lock = CompletionLock {
            schema_version: 1,
            work_id: "wrk_001".to_string(),
            locked_at: "2026-06-10T12:00:00Z".to_string(),
            reason: "completion".to_string(),
        };

        write_completion_lock(ws, "test-novel", &lock).unwrap();
        assert!(completion_lock_path(ws, "test-novel").exists());

        release_completion_lock(ws, "test-novel").unwrap();
        assert!(!completion_lock_path(ws, "test-novel").exists());
    }

    #[test]
    fn release_idempotent_on_missing() {
        let dir = TempDir::new().unwrap();
        // Should succeed even if file never existed
        release_completion_lock(dir.path(), "no-such-work").unwrap();
    }
}
