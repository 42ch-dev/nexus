//! Integration test: works_status_lock_holder (V1.51 T-B P0).
//!
//! Tests that lock_holder info is correctly read from filesystem.
//! Full status JSON integration deferred to QA (requires daemon).

use std::path::PathBuf;

fn test_work_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("Works").join("test-status-work");
    std::fs::create_dir_all(&work_dir).unwrap();
    (dir, work_dir)
}

#[test]
fn lock_holder_json_serialises_correctly() {
    let (_dir, work_dir) = test_work_dir();

    // Write a lock file with known metadata.
    let lock_path = work_dir.join(".lock");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let body = format!("12345:cli:test-holder:{}", now_ms + 60_000);
    std::fs::write(&lock_path, &body).unwrap();

    let info = nexus_local_db::file_lock::read_lock_holder_info(&work_dir).unwrap();
    assert_eq!(info.pid, 12345);
    assert_eq!(info.holder_name, "cli:test-holder");
    assert!(!info.stale);
}

#[test]
fn lock_holder_null_when_no_lock_file() {
    let (_dir, work_dir) = test_work_dir();
    assert!(nexus_local_db::file_lock::read_lock_holder_info(&work_dir).is_none());
}
