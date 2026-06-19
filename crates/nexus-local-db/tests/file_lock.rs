//! Integration tests for `nexus_local_db::file_lock` (V1.51 T-B P0).
//!
//! Tests file lock acquire/release across concurrent async tasks within
//! the same process, exercising the `flock` semantics through separate
//! `FileLockGuard` scopes.

use std::path::PathBuf;

use nexus_local_db::file_lock;
use nexus_local_db::file_lock::FileLockError;

fn test_work_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let work_dir = dir.path().join("Works").join("test-int-work");
    std::fs::create_dir_all(&work_dir).unwrap();
    (dir, work_dir)
}

#[tokio::test]
async fn concurrent_tasks_serialise_via_file_lock() {
    let (_dir, work_dir) = test_work_dir();

    let g1 = file_lock::try_acquire(&work_dir, "cli:task-1").unwrap();

    // Task 2 fails to acquire while task 1 holds the lock.
    let err = file_lock::try_acquire(&work_dir, "cli:task-2").unwrap_err();
    let locked = match err {
        FileLockError::Locked(locked) => locked,
        _ => panic!("expected FileLockError::Locked, got {err:?}"),
    };
    assert_eq!(locked.holder_name, "cli:task-1");

    drop(g1);

    // After drop, task 3 can acquire.
    let g3 = file_lock::try_acquire(&work_dir, "cli:task-3").unwrap();
    drop(g3);
}

#[tokio::test]
async fn lock_holder_info_reflects_current_state() {
    let (_dir, work_dir) = test_work_dir();

    // No lock file → None.
    assert!(file_lock::read_lock_holder_info(&work_dir).is_none());

    let _guard = file_lock::try_acquire(&work_dir, "cli:info-test").unwrap();

    // Lock file exists with metadata.
    let info = file_lock::read_lock_holder_info(&work_dir).unwrap();
    assert_eq!(info.holder_name, "cli:info-test");
    assert_eq!(info.pid, std::process::id());
    assert!(!info.stale);
}

#[tokio::test]
async fn zombie_lock_overwritten_on_reacquire() {
    let (_dir, work_dir) = test_work_dir();

    // Acquire and immediately write stale metadata (simulating a dead process).
    {
        let _g = file_lock::try_acquire(&work_dir, "cli:zombie").unwrap();
        // Write stale expires_at_ms directly to the file.
        let lock_path = work_dir.join(".lock");
        let stale_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 120_000; // 2 min ago
        let stale_body = format!("99999:cli:zombie-stale:{}", stale_ms);
        std::fs::write(&lock_path, &stale_body).unwrap();
    }
    // Guard dropped → flock released. Stale metadata remains.

    // Re-acquire — should succeed and overwrite stale metadata.
    let guard = file_lock::try_acquire(&work_dir, "cli:fresh").unwrap();
    let info = file_lock::read_lock_holder_info(&work_dir).unwrap();
    assert_eq!(info.holder_name, "cli:fresh");
    assert_eq!(info.pid, std::process::id());
    assert!(!info.stale);
    drop(guard);
}
