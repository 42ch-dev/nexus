//! Completion-lock file I/O for multi-Work lifecycle (DF-60 §3).
//!
//! The lock file lives at `Works/<work_ref>/.completion-lock.json`.
//! Operations use atomic tmp+rename to avoid partial writes.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Completion-lock payload written to disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionLock {
    /// The Work ID this lock protects.
    pub work_id: String,
    /// ISO-8601 timestamp when the lock was applied.
    pub locked_at: String,
    /// Reason for the lock (currently always `"completion"`).
    pub reason: String,
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
    let lock: CompletionLock = serde_json::from_str(&data).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid completion-lock JSON: {e}"),
        )
    })?;

    Ok(Some(lock))
}

/// Remove the completion-lock file.
///
/// # Errors
///
/// Returns `std::io::Error` if the file exists but cannot be removed.
/// Returns `Ok(())` if the file does not exist (idempotent).
pub fn release_completion_lock(
    workspace_dir: &Path,
    work_ref: &str,
) -> Result<(), std::io::Error> {
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
        assert_eq!(
            p,
            PathBuf::from("/ws/Works/my-novel/.completion-lock.json")
        );
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = TempDir::new().unwrap();
        let ws = dir.path();
        let work_dir = ws.join("Works").join("test-novel");
        std::fs::create_dir_all(&work_dir).unwrap();

        let lock = CompletionLock {
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
