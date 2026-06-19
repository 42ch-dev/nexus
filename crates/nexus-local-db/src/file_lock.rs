//! File-based advisory lock `Works/<work_ref>/.lock` (V1.51 T-B P0).
//!
//! Spec: `concurrency.md` §2-§6.
//!
//! Provides a cross-process mutual exclusion mechanism using `flock(LOCK_EX)`
//! on a `.lock` file. The lock file body carries metadata
//! `<pid>:<holder_name>:<expires_at_ms>` for visibility and zombie detection.
//!
//! ## Lock ordering
//!
//! - File lock BEFORE DB lock. Never the reverse.
//! - Never acquire two file locks simultaneously.
//!
//! ## Platform
//!
//! Unix-only (`flock`). The entire module is `#[cfg(unix)]`.

use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Heartbeat refresh interval in seconds.
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Stale threshold: a lock not refreshed for this many seconds is stale.
const STALE_THRESHOLD_SECS: u64 = 60;

/// Conflicting lock information returned when `try_acquire` fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    /// OS process ID of the holder.
    pub holder_pid: u32,
    /// Human-readable holder identity (e.g. `cli:cron-set`, `daemon:schedule:SCH...`).
    pub holder_name: String,
    /// Unix epoch milliseconds when the heartbeat expires.
    pub expires_at_ms: u64,
    /// `true` if the lock has not been refreshed in > 60 s (zombie).
    pub stale: bool,
}

impl Locked {
    /// Format a human-readable display line.
    #[must_use]
    pub fn display_line(&self) -> String {
        let stale_marker = if self.stale { " (STALE)" } else { "" };
        format!(
            "work is held by {} pid={}{}",
            self.holder_name, self.holder_pid, stale_marker
        )
    }
}

/// RAII guard for the advisory file lock.
///
/// Releases `flock` and cancels the heartbeat task on drop.
///
/// All fields are `Send` (`std::fs::File` on Unix, `tokio::task::JoinHandle`,
/// `tokio::sync::watch::Sender`), so `FileLockGuard` auto-derives
/// `Send` — no manual `unsafe impl` needed.
#[derive(Debug)]
pub struct FileLockGuard {
    /// The underlying lock file (holds the `flock`).
    fd: Option<std::fs::File>,
    /// Handle to the heartbeat task.
    heartbeat_handle: Option<tokio::task::JoinHandle<()>>,
    /// Cancel signal for the heartbeat task.
    heartbeat_cancel: tokio::sync::watch::Sender<bool>,
}

/// Error returned by [`try_acquire`].
///
/// Distinguishes real I/O failures (permission denied, disk full, missing
/// parent directory) from lock contention. Callers must map these to different
/// exit codes (e.g. 75 for temporary contention, 78 for configuration/I/O errors).
#[derive(Debug)]
pub enum FileLockError {
    /// Another process holds the lock (contention — retryable).
    Locked(Locked),
    /// An I/O error prevented lock acquisition (not retryable — configuration
    /// or environment problem).
    Io(std::io::Error),
}

impl std::fmt::Display for FileLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Locked(locked) => write!(f, "file lock held: {}", locked.display_line()),
            Self::Io(e) => write!(f, "file lock I/O error: {e}"),
        }
    }
}

impl std::error::Error for FileLockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Locked(_) => None,
        }
    }
}

impl From<std::io::Error> for FileLockError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Build the lock file path for a Work directory.
#[must_use]
fn lock_file_path(work_dir: &Path) -> PathBuf {
    work_dir.join(".lock")
}

/// Current time in Unix epoch milliseconds.
fn now_ms() -> u64 {
    #[allow(clippy::cast_possible_truncation)]
    // as_millis() returns u128 — u64 holds ~584 million years of milliseconds.
    {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Format the lock file body: `<pid>:<holder_name>:<expires_at_ms>`.
fn format_lock_body(holder_name: &str, expires_at_ms: u64) -> String {
    let pid = std::process::id();
    format!("{pid}:{holder_name}:{expires_at_ms}")
}

/// Parse the lock file body into its components.
///
/// Returns `None` if the content is empty or malformed.
///
/// Format: `<pid>:<holder_name>:<expires_at_ms>`. Since holder names may
/// contain colons (e.g. `cli:cron-set`, `daemon:schedule:SCH...`), we
/// split on the first `:` (pid) and the last `:` (expires) — everything
/// in between is the holder name.
fn parse_lock_body(content: &str) -> Option<(u32, String, u64)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first_colon = trimmed.find(':')?;
    let pid: u32 = trimmed[..first_colon].parse().ok()?;
    let rest = &trimmed[first_colon + 1..];
    let last_colon = rest.rfind(':')?;
    let holder_name = rest[..last_colon].to_string();
    let expires_at_ms: u64 = rest[last_colon + 1..].parse().ok()?;
    Some((pid, holder_name, expires_at_ms))
}

/// Read lock metadata from the `.lock` file (best-effort; does not acquire `flock`).
fn read_lock_metadata(work_dir: &Path) -> Option<(u32, String, u64)> {
    let path = lock_file_path(work_dir);
    let mut content = String::new();
    std::fs::File::open(&path)
        .ok()?
        .read_to_string(&mut content)
        .ok()?;
    parse_lock_body(&content)
}

/// Write lock metadata to the lock file path (best-effort; does not require flock).
fn write_lock_metadata_to_path(path: &Path, body: &str) {
    if let Err(e) = std::fs::write(path, body) {
        tracing::error!(
            lock_path = %path.display(),
            error = %e,
            "file_lock: failed to write lock metadata"
        );
    }
}

/// Attempt to acquire the advisory file lock for a Work.
///
/// Opens (or creates) `Works/<work_ref>/.lock`, tries `flock(LOCK_EX | LOCK_NB)`.
/// On success, writes the lock metadata, spawns a heartbeat task, and returns
/// a `FileLockGuard` that releases the lock on drop.
///
/// On conflict, reads the existing lock metadata from the file and returns
/// `FileLockError::Locked` with holder details and staleness.
///
/// # Errors
///
/// Returns `FileLockError::Locked` if another process holds the lock.
/// Returns `FileLockError::Io` if an I/O error prevents acquisition
/// (permission denied, disk full, missing parent directory, etc.).
pub fn try_acquire(work_dir: &Path, holder_name: &str) -> Result<FileLockGuard, FileLockError> {
    let lock_path = lock_file_path(work_dir);

    // Ensure the parent directory exists. Propagate I/O errors up — do NOT
    // silently swallow permission-denied or disk-full failures.
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Open or create the lock file.
    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;

    // Try non-blocking exclusive lock.
    let raw_fd = fd.as_raw_fd();
    // nix 0.28 deprecates `flock()` in favor of `Flock` struct, but the struct
    // API requires ownership of the inner file descriptor. Use the deprecated
    // function for now since we need to keep the `File` alive for heartbeat.
    #[allow(deprecated)]
    let locked = nix::fcntl::flock(raw_fd, nix::fcntl::FlockArg::LockExclusiveNonblock).is_err();
    if locked {
        // Lock held by another process — read metadata for conflict info.
        let metadata = read_lock_metadata(work_dir);
        let now = now_ms();
        let (holder_pid, holder_name, expires_at_ms) =
            metadata.unwrap_or_else(|| (0, "unknown".to_string(), 0));
        // NB: We intentionally do NOT mark a parse failure as stale here. If
        // another live process holds the flock, the lock is genuinely held;
        // reporting holder_name="unknown" with stale=false keeps the conflict
        // information conservative. The 60 s heartbeat window absorbs the small
        // risk of a partially-written metadata file (R-V151Q1-09).
        let stale =
            expires_at_ms > 0 && now.saturating_sub(expires_at_ms) > STALE_THRESHOLD_SECS * 1000;

        return Err(FileLockError::Locked(Locked {
            holder_pid,
            holder_name,
            expires_at_ms,
            stale,
        }));
    }

    // Lock acquired. Write metadata.
    let expires_at_ms = now_ms() + STALE_THRESHOLD_SECS * 1000;
    let body = format_lock_body(holder_name, expires_at_ms);
    write_lock_metadata_to_path(&lock_path, &body);

    // Spawn heartbeat task.
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    let heartbeat_holder = holder_name.to_string();

    let heartbeat_handle = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
        // Skip the immediate first tick (we already wrote metadata).
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let expires = now_ms() + STALE_THRESHOLD_SECS * 1000;
                    let body = format_lock_body(&heartbeat_holder, expires);
                    write_lock_metadata_to_path(&lock_path, &body);
                }
                _ = cancel_rx.changed() => {
                    break;
                }
            }
        }
    });

    Ok(FileLockGuard {
        fd: Some(fd),
        heartbeat_handle: Some(heartbeat_handle),
        heartbeat_cancel: cancel_tx,
    })
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        // Cancel the heartbeat task.
        let _ = self.heartbeat_cancel.send(true);
        if let Some(handle) = self.heartbeat_handle.take() {
            handle.abort();
        }

        // Release the flock.
        if let Some(fd) = &self.fd {
            let raw_fd = fd.as_raw_fd();
            #[allow(deprecated)]
            // nix 0.28 deprecates `flock()` — see acquire block above for rationale.
            {
                if let Err(e) = nix::fcntl::flock(raw_fd, nix::fcntl::FlockArg::Unlock) {
                    tracing::error!(
                        error = %e,
                        "file_lock: failed to release flock on drop"
                    );
                }
            }
        }
        // File is closed when `fd` is dropped.
    }
}

/// Snapshot of lock holder information for status display.
#[derive(Debug, Clone)]
pub struct LockHolderInfo {
    /// OS process ID.
    pub pid: u32,
    /// Human-readable holder identity.
    pub holder_name: String,
    /// Unix epoch milliseconds when the heartbeat expires.
    pub expires_at_ms: u64,
    /// Whether the lock is stale (> 60 s without refresh).
    pub stale: bool,
}

/// Read the lock holder information from the `.lock` file (best-effort, no `flock`).
///
/// Returns `None` if the file doesn't exist, is empty, or is malformed.
/// Used by informational commands like `creator works status --json`.
#[must_use]
pub fn read_lock_holder_info(work_dir: &Path) -> Option<LockHolderInfo> {
    let (pid, holder_name, expires_at_ms) = read_lock_metadata(work_dir)?;
    let stale = {
        let now = now_ms();
        expires_at_ms > 0 && now.saturating_sub(expires_at_ms) > STALE_THRESHOLD_SECS * 1000
    };
    Some(LockHolderInfo {
        pid,
        holder_name,
        expires_at_ms,
        stale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_work_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let work_dir = dir.path().join("Works").join("test-work");
        std::fs::create_dir_all(&work_dir).unwrap();
        (dir, work_dir)
    }

    // ── format_lock_body / parse_lock_body ──────────────────────────

    #[test]
    fn format_and_parse_roundtrip() {
        let body = format_lock_body("cli:cron-set", 1718700000000);

        // Verify the body uses the correct format with three colon-delimited segments.
        // Holder names may contain colons, so we use first/last colon parsing.
        let first_colon = body.find(':').unwrap();
        let pid: u32 = body[..first_colon].parse().unwrap();
        assert!(pid > 0);
        let rest = &body[first_colon + 1..];
        let last_colon = rest.rfind(':').unwrap();
        assert_eq!(&rest[..last_colon], "cli:cron-set");
        assert_eq!(&rest[last_colon + 1..], "1718700000000");

        // Roundtrip through parse_lock_body.
        let parsed = parse_lock_body(&body).unwrap();
        assert_eq!(parsed.0, std::process::id());
        assert_eq!(parsed.1, "cli:cron-set");
        assert_eq!(parsed.2, 1718700000000);

        // Test with a daemon-style holder name with multiple colons.
        let body2 = format_lock_body("daemon:schedule:SCH20260618120000", 1718800000000);
        let parsed2 = parse_lock_body(&body2).unwrap();
        assert_eq!(parsed2.1, "daemon:schedule:SCH20260618120000");
        assert_eq!(parsed2.2, 1718800000000);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(parse_lock_body("").is_none());
        assert!(parse_lock_body("  ").is_none());
    }

    #[test]
    fn parse_malformed_returns_none() {
        assert!(parse_lock_body("abc").is_none());
        assert!(parse_lock_body("123:holder").is_none()); // missing expires
        assert!(parse_lock_body("not_a_pid:holder:123").is_none());
    }

    // ── try_acquire + drop release ─────────────────────────────────

    #[tokio::test]
    async fn test_acquire_and_release_via_drop() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);

        {
            let _guard = try_acquire(&work_dir, "cli:test-acquire").unwrap();
            assert!(lock_path.exists());
            let content = std::fs::read_to_string(&lock_path).unwrap();
            let (pid, holder, expires) = parse_lock_body(&content).unwrap();
            assert_eq!(pid, std::process::id());
            assert_eq!(holder, "cli:test-acquire");
            assert!(expires > 0);
            // Guard drops here.
        }

        // After drop, another acquire should succeed.
        let guard2 = try_acquire(&work_dir, "cli:test-acquire-2").unwrap();
        drop(guard2);
    }

    // ── Contention: second acquire fails ────────────────────────────

    #[tokio::test]
    async fn test_second_acquire_fails_with_locked_info() {
        let (_dir, work_dir) = sample_work_dir();

        let _guard = try_acquire(&work_dir, "cli:holder-a").unwrap();

        let err = try_acquire(&work_dir, "cli:holder-b").unwrap_err();
        let locked = match err {
            FileLockError::Locked(locked) => locked,
            _ => panic!("expected FileLockError::Locked, got {err:?}"),
        };
        assert_eq!(locked.holder_name, "cli:holder-a");
        assert_eq!(locked.holder_pid, std::process::id());
        assert!(!locked.stale, "fresh lock should not be stale");
    }

    // ── Lock released after drop allows reacquire ──────────────────

    #[tokio::test]
    async fn test_lock_released_after_drop_allows_reacquire() {
        let (_dir, work_dir) = sample_work_dir();

        {
            let _g = try_acquire(&work_dir, "cli:scope-test").unwrap();
        }

        let guard = try_acquire(&work_dir, "cli:scope-test-2").unwrap();
        let content = std::fs::read_to_string(lock_file_path(&work_dir)).unwrap();
        let (_, holder, _) = parse_lock_body(&content).unwrap();
        assert_eq!(holder, "cli:scope-test-2");
        drop(guard);
    }

    // ── Zombie detection: stale lock file ──────────────────────────

    #[tokio::test]
    async fn test_stale_lock_file_overwritten_on_acquire() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);

        // Simulate a stale lock from a dead process:
        // Write old metadata, but don't hold flock.
        let old_expires = now_ms().saturating_sub(120_000); // 2 min ago
        let stale_body = format!("99999:daemon:schedule:old:{old_expires}");
        std::fs::write(&lock_path, &stale_body).unwrap();

        // Acquire should succeed (no flock held), overwriting stale metadata.
        let guard = try_acquire(&work_dir, "cli:new-owner").unwrap();

        let content = std::fs::read_to_string(&lock_path).unwrap();
        let (pid, holder, expires) = parse_lock_body(&content).unwrap();
        assert_eq!(pid, std::process::id());
        assert_eq!(holder, "cli:new-owner");
        assert!(expires > old_expires);
        drop(guard);
    }

    // ── read_lock_holder_info ─────────────────────────────────────

    #[test]
    fn test_read_lock_holder_info_stale() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);

        let old_expires = now_ms().saturating_sub(120_000);
        let body = format!("88888:cli:stale-holder:{old_expires}");
        std::fs::write(&lock_path, &body).unwrap();

        let info = read_lock_holder_info(&work_dir).unwrap();
        assert_eq!(info.pid, 88888);
        assert_eq!(info.holder_name, "cli:stale-holder");
        assert!(info.stale);
    }

    #[test]
    fn test_read_lock_holder_info_fresh() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);

        let future_expires = now_ms() + 120_000;
        let body = format!("77777:cli:fresh-holder:{future_expires}");
        std::fs::write(&lock_path, &body).unwrap();

        let info = read_lock_holder_info(&work_dir).unwrap();
        assert_eq!(info.holder_name, "cli:fresh-holder");
        assert!(!info.stale);
    }

    #[test]
    fn test_read_lock_holder_info_no_file() {
        let (_dir, work_dir) = sample_work_dir();
        assert!(read_lock_holder_info(&work_dir).is_none());
    }

    // ── Locked::display_line ────────────────────────────────────────

    #[test]
    fn test_locked_display_line() {
        let locked = Locked {
            holder_pid: 1234,
            holder_name: "daemon:schedule:X".to_string(),
            expires_at_ms: 0,
            stale: false,
        };
        assert_eq!(
            locked.display_line(),
            "work is held by daemon:schedule:X pid=1234"
        );

        let stale = Locked {
            stale: true,
            ..locked
        };
        assert!(stale.display_line().contains("STALE"));
    }

    // ── Concurrent scope isolation ─────────────────────────────────

    #[tokio::test]
    async fn test_concurrent_scope_isolation() {
        let (_dir, work_dir) = sample_work_dir();

        let guard_a = try_acquire(&work_dir, "cli:scope-a").unwrap();
        let err = try_acquire(&work_dir, "cli:scope-b").unwrap_err();
        let locked = match err {
            FileLockError::Locked(locked) => locked,
            _ => panic!("expected FileLockError::Locked, got {err:?}"),
        };
        assert_eq!(locked.holder_name, "cli:scope-a");

        drop(guard_a);
        let guard_c = try_acquire(&work_dir, "cli:scope-c").unwrap();
        drop(guard_c);
    }

    // ── I/O errors surface as FileLockError::Io ─────────────────────

    #[test]
    fn test_io_error_surfaces_not_locked() {
        // Use a path whose parent is a regular file, not a directory.
        // create_dir_all will fail with "Not a directory" → Io, not Locked.
        let dir = tempfile::tempdir().unwrap();
        // Create a regular file at the lock path's parent so create_dir_all fails.
        let work_dir = dir.path().join("Works").join("file-as-dir");
        let _lock_path = work_dir.join(".lock");
        // Create a regular file where a directory is expected.
        std::fs::create_dir_all(work_dir.parent().unwrap()).unwrap();
        std::fs::write(&work_dir, "block").unwrap();

        let err = try_acquire(&work_dir, "cli:io-test").unwrap_err();
        match err {
            FileLockError::Io(io_err) => {
                assert!(
                    io_err.to_string().contains("Not a directory")
                        || io_err.to_string().contains("File exists"),
                    "expected I/O error about directory, got: {io_err}"
                );
            }
            FileLockError::Locked(_) => {
                panic!("expected FileLockError::Io, got Locked — I/O errors must not be mapped to Locked");
            }
        }
        // Clean up for next test.
        std::fs::remove_file(&work_dir).ok();
    }
}
