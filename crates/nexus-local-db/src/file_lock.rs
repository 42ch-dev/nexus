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

/// Maximum prior lock metadata read for successful-acquire diagnostics.
const MAX_PRIOR_LOCK_BODY_BYTES: usize = 4096;
/// Render a prior lock holder as a non-reversible fingerprint for diagnostics.
///
/// A `.lock` body is neither provenance-authenticated nor validated by
/// `try_acquire`, which writes the caller-supplied holder verbatim, so the raw
/// text may be stale — or secret-bearing — content. Always emit a digest
/// stand-in rather than the holder text.
fn diagnostic_holder_name(holder_name: &str) -> String {
    let digest = sha256(holder_name.as_bytes());
    format!("<fp:{}>", &digest[..12])
}

fn sha256(input: &[u8]) -> String {
    // FIPS 180-4 round constants (first 32 bits of the fractional parts of the
    // cube roots of the first 64 primes).
    #[rustfmt::skip]
    const K: [u32; 64] = [
        0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
        0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
        0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
        0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
        0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
        0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
        0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
        0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
    ];
    let mut data = input.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    // Initial hash value: first 32 bits of the fractional parts of the square
    // roots of the first eight primes.
    let mut state = [
        0x6a09_e667u32,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    for chunk in data.as_chunks::<64>().0 {
        let mut schedule = [0u32; 64];
        for (index, bytes) in chunk.as_chunks::<4>().0.iter().enumerate() {
            schedule[index] = u32::from_be_bytes(*bytes);
        }
        for round in 16..64 {
            let sigma0 = schedule[round - 15].rotate_right(7)
                ^ schedule[round - 15].rotate_right(18)
                ^ (schedule[round - 15] >> 3);
            let sigma1 = schedule[round - 2].rotate_right(17)
                ^ schedule[round - 2].rotate_right(19)
                ^ (schedule[round - 2] >> 10);
            schedule[round] = schedule[round - 16]
                .wrapping_add(sigma0)
                .wrapping_add(schedule[round - 7])
                .wrapping_add(sigma1);
        }
        let [mut s0, mut s1, mut s2, mut s3, mut s4, mut s5, mut s6, mut s7] = state;
        for round in 0..64 {
            let upper = s4.rotate_right(6) ^ s4.rotate_right(11) ^ s4.rotate_right(25);
            let choose = (s4 & s5) ^ (!s4 & s6);
            let temp1 = s7
                .wrapping_add(upper)
                .wrapping_add(choose)
                .wrapping_add(K[round])
                .wrapping_add(schedule[round]);
            let lower = s0.rotate_right(2) ^ s0.rotate_right(13) ^ s0.rotate_right(22);
            let majority = (s0 & s1) ^ (s0 & s2) ^ (s1 & s2);
            let temp2 = lower.wrapping_add(majority);
            (s7, s6, s5, s4, s3, s2, s1, s0) = (
                s6,
                s5,
                s4,
                s3.wrapping_add(temp1),
                s2,
                s1,
                s0,
                temp1.wrapping_add(temp2),
            );
        }
        for (value, addend) in state.iter_mut().zip([s0, s1, s2, s3, s4, s5, s6, s7]) {
            *value = value.wrapping_add(addend);
        }
    }
    let mut out = String::with_capacity(64);
    for word in state {
        use std::fmt::Write as _;
        write!(out, "{word:08x}").unwrap();
    }
    out
}

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
    // Classify only a bounded prefix from the already-open locked descriptor.
    // The OS lock arbitrates takeover; this is diagnostic-only.
    let mut prior_body = Vec::with_capacity(MAX_PRIOR_LOCK_BODY_BYTES + 1);
    let read_result = fd.try_clone().and_then(|reader| {
        reader
            .take((MAX_PRIOR_LOCK_BODY_BYTES + 1) as u64)
            .read_to_end(&mut prior_body)
    });
    match read_result {
        Ok(_) if prior_body.is_empty() => {}
        Ok(_) if prior_body.len() > MAX_PRIOR_LOCK_BODY_BYTES => tracing::warn!(
            note = "prior lock body exceeded diagnostic read limit",
            "file_lock: unparseable prior holder metadata after successful acquire"
        ),
        Ok(_) => {
            let parsed = std::str::from_utf8(&prior_body)
                .ok()
                .and_then(parse_lock_body);
            if let Some((pid, previous_holder, expires_at_ms)) = parsed {
                let now = now_ms();
                if expires_at_ms > 0
                    && now.saturating_sub(expires_at_ms) > STALE_THRESHOLD_SECS * 1000
                {
                    let holder_name = diagnostic_holder_name(&previous_holder);
                    tracing::warn!(
                        pid,
                        holder_name = %holder_name,
                        expires_at_ms,
                        "file_lock: stale holder detected after successful acquire"
                    );
                }
            } else {
                tracing::warn!(
                    note = "prior lock body was not parseable",
                    "file_lock: unparseable prior holder metadata after successful acquire"
                );
            }
        }
        Err(_) => {}
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
        let body = format_lock_body("cli:cron-set", 1_718_700_000_000);

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
        assert_eq!(parsed.2, 1_718_700_000_000);

        // Test with a daemon-style holder name with multiple colons.
        let body2 = format_lock_body("daemon:schedule:SCH20260618120000", 1_718_800_000_000);
        let parsed2 = parse_lock_body(&body2).unwrap();
        assert_eq!(parsed2.1, "daemon:schedule:SCH20260618120000");
        assert_eq!(parsed2.2, 1_718_800_000_000);
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
        let (layer, captured) = crate::test_tracing::capture_layer();
        let subscriber = crate::test_tracing::subscriber_with(layer);
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);

        {
            let _guard = try_acquire(&work_dir, "cli:test-acquire").unwrap();
            let content = std::fs::read_to_string(lock_file_path(&work_dir)).unwrap();
            let (pid, holder, expires) = parse_lock_body(&content).unwrap();
            assert_eq!(pid, std::process::id());
            assert_eq!(holder, "cli:test-acquire");
            assert!(expires > 0);
        }

        let guard2 = try_acquire(&work_dir, "cli:test-acquire-2").unwrap();
        drop(guard2);
        crate::test_tracing::assert_warn_absent(&captured);
    }

    // ── Contention: second acquire fails ────────────────────────────

    #[tokio::test]
    async fn test_second_acquire_fails_with_locked_info() {
        let (_dir, work_dir) = sample_work_dir();

        let _guard = try_acquire(&work_dir, "cli:holder-a").unwrap();

        let err = try_acquire(&work_dir, "cli:holder-b").unwrap_err();
        let FileLockError::Locked(locked) = err else {
            panic!("expected FileLockError::Locked, got {err:?}")
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
        let (layer, captured) = crate::test_tracing::capture_layer();
        let subscriber = crate::test_tracing::subscriber_with(layer);

        let old_expires = now_ms().saturating_sub(120_000);
        let stale_body = format!("99999:daemon:schedule:old:{old_expires}");
        std::fs::write(&lock_path, &stale_body).unwrap();

        let _subscriber_guard = tracing::subscriber::set_default(subscriber);
        let guard = try_acquire(&work_dir, "cli:new-owner").unwrap();
        let content = std::fs::read_to_string(&lock_path).unwrap();
        let (pid, holder, expires) = parse_lock_body(&content).unwrap();
        assert_eq!(pid, std::process::id());
        assert_eq!(holder, "cli:new-owner");
        assert!(expires > old_expires);
        let expected_holder = format!("<fp:{}>", &sha256(b"daemon:schedule:old")[..12]);
        crate::test_tracing::assert_warn_emitted(
            &captured,
            &[
                "pid=99999",
                &format!("holder_name={expected_holder}"),
                "expires_at_ms=",
            ],
        );
        drop(guard);
    }

    #[tokio::test]
    async fn stale_acquire_emits_zombie_warning() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);
        let old_expires = now_ms().saturating_sub(120_000);
        std::fs::write(&lock_path, format!("99999:cli:stale-holder:{old_expires}")).unwrap();

        let (layer, captured) = crate::test_tracing::capture_layer();
        let subscriber = crate::test_tracing::subscriber_with(layer);
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);
        let guard = try_acquire(&work_dir, "fresh-holder").unwrap();
        let (_, holder, expires) =
            parse_lock_body(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(holder, "fresh-holder");
        assert!(expires > old_expires);
        let expected_holder = format!("<fp:{}>", &sha256(b"cli:stale-holder")[..12]);
        crate::test_tracing::assert_warn_emitted(
            &captured,
            &[
                "pid=99999",
                &format!("holder_name={expected_holder}"),
                "expires_at_ms=",
            ],
        );
        drop(guard);
    }

    #[tokio::test]
    async fn unparseable_prior_body_warns_and_proceeds() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);
        std::fs::write(&lock_path, "not:valid:metadata").unwrap();

        let (layer, captured) = crate::test_tracing::capture_layer();
        let subscriber = crate::test_tracing::subscriber_with(layer);
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);
        let guard = try_acquire(&work_dir, "fresh-holder").unwrap();
        let (_, holder, _) =
            parse_lock_body(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(holder, "fresh-holder");
        crate::test_tracing::assert_warn_emitted(&captured, &["not parseable"]);
        drop(guard);
    }

    #[tokio::test]
    async fn oversized_prior_body_warns_and_takeover_proceeds() {
        let (_dir, work_dir) = sample_work_dir();
        std::fs::write(
            lock_file_path(&work_dir),
            vec![b'x'; MAX_PRIOR_LOCK_BODY_BYTES + 1],
        )
        .unwrap();
        let (layer, captured) = crate::test_tracing::capture_layer();
        let subscriber = crate::test_tracing::subscriber_with(layer);
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);

        let guard = try_acquire(&work_dir, "fresh-holder").unwrap();
        let content = std::fs::read_to_string(lock_file_path(&work_dir)).unwrap();
        assert_eq!(parse_lock_body(&content).unwrap().1, "fresh-holder");
        crate::test_tracing::assert_warn_emitted(&captured, &["exceeded diagnostic read limit"]);
        drop(guard);
    }

    #[tokio::test]
    async fn stale_holder_warning_sanitizes_and_bounds_identity() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);
        let old_expires = now_ms().saturating_sub(120_000);
        let adversarial = format!("99999:bad\nholder{}:{old_expires}", "x".repeat(200));
        std::fs::write(&lock_path, adversarial).unwrap();
        let (layer, captured) = crate::test_tracing::capture_layer();
        let subscriber = crate::test_tracing::subscriber_with(layer);
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);

        let guard = try_acquire(&work_dir, "fresh-holder").unwrap();
        let warning = captured.lock().unwrap().join("\n");
        assert!(warning.contains("pid=99999"));
        assert!(warning.contains("<fp:"));
        assert!(!warning.contains("bad"));
        assert!(!warning.contains(&"x".repeat(200)));
        drop(guard);
    }

    #[tokio::test]
    async fn stale_holder_warning_redacts_secret_bearing_identity() {
        let (_dir, work_dir) = sample_work_dir();
        let lock_path = lock_file_path(&work_dir);
        // F-001: a grammar-conforming `cli:`-prefixed secret is the exact case
        // FX-A2 still echoed; it must be fingerprinted like any other holder.
        let holder = "cli:secret-token-value";
        let expires = now_ms().saturating_sub(120_000);
        std::fs::write(&lock_path, format!("99999:{holder}:{expires}")).unwrap();
        let (layer, captured) = crate::test_tracing::capture_layer();
        let subscriber = crate::test_tracing::subscriber_with(layer);
        let _subscriber_guard = tracing::subscriber::set_default(subscriber);

        let guard = try_acquire(&work_dir, "fresh-holder").unwrap();
        let warning = captured.lock().unwrap().join("\n");
        let stand_in = format!("<fp:{}>", &sha256(holder.as_bytes())[..12]);
        assert!(warning.contains(&format!("holder_name={stand_in}")));
        assert!(!warning.contains(holder));
        assert!(warning.contains("pid=99999"));
        assert!(warning.contains("expires_at_ms="));
        drop(guard);
    }

    #[test]
    fn diagnostic_holder_name_always_fingerprints() {
        // F-001: no holder text may reach diagnostics, even a grammar-conforming
        // `cli:`-prefixed secret — the case FX-A2 still echoed verbatim.
        for holder in [
            "cli:secret-token-value",
            "bad\nholder",
            "daemon:schedule:job",
        ] {
            let expected = format!("<fp:{}>", &sha256(holder.as_bytes())[..12]);
            assert_eq!(diagnostic_holder_name(holder), expected);
        }
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
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
        let FileLockError::Locked(locked) = err else {
            panic!("expected FileLockError::Locked, got {err:?}")
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
