//! I-000065: the daemon-home `config.toml` advisory lock.
//!
//! `<user_home>/.nexus42/config.toml` has four cross-process writers: this
//! core, the CLI (`apps/nexus42/src/config.rs`), the nexus-service profile
//! writer (`apps/nexus-service/src/agent-profile.ts`) and the Electron desktop
//! main (`apps/desktop-electron/src/desktop-config.ts`). They share ONE protocol:
//! exclusive-create of `<daemon home>/config.toml.lock` for the whole
//! read-modify-write, stale steal after 10 s, release by unlink. No `flock`:
//! Node's stdlib has none, and mixing lockfile-existence with flock breaks
//! mutual exclusion.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{CoreError, CoreResult};

const LOCK_FILE: &str = "config.toml.lock";
/// A normal hold is milliseconds (a small TOML document, at most two fsyncs).
/// Ten seconds is a stale heuristic, not proof of death: a paused or suspended
/// live holder can look stale, and stealing can transiently leave two holders.
/// Safe only because all four writers are user-paced — nothing enforces that.
const STALE_AFTER: Duration = Duration::from_secs(10);
const BACKOFF_START: Duration = Duration::from_millis(25);
const BACKOFF_CAP: Duration = Duration::from_millis(500);

/// The held lock: unlinking on drop releases it on every return path.
struct ConfigLock {
    path: PathBuf,
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn lock_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("config_lock: {error}"),
    }
}

/// Take the lock: exclusive create, stale steal, exponential backoff.
fn acquire(nexus_home: &Path) -> CoreResult<ConfigLock> {
    let path = nexus_home.join(LOCK_FILE);
    std::fs::create_dir_all(nexus_home).map_err(lock_error)?;
    let mut backoff = BACKOFF_START;
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                // Diagnostics only (who/what); the age comes from the mtime.
                let _ = writeln!(
                    file,
                    "{} {}",
                    std::process::id(),
                    chrono::Utc::now().to_rfc3339()
                );
                return Ok(ConfigLock { path });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let age = std::fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok());
                match age {
                    // Released between the create and the stat: retry at once.
                    None => continue,
                    Some(age) if age > STALE_AFTER => {
                        // Stale age is a heuristic: two reclaimers can both
                        // unlink and create, leaving two live holders, and a
                        // live slow holder can lose its lock here. Accepted only
                        // because all four writers are user-paced.
                        let _ = std::fs::remove_file(&path);
                        backoff = BACKOFF_START;
                    }
                    Some(_) => {
                        std::thread::sleep(backoff);
                        backoff = (backoff * 2).min(BACKOFF_CAP);
                    }
                }
            }
            Err(error) => return Err(lock_error(error)),
        }
    }
}

/// Run one `config.toml` read-modify-write under the daemon-home advisory
/// lock. Every writer of this file takes it — the service
/// (`apps/nexus-service/src/agent-profile.ts` `writeAgentProfile`), the
/// desktop main (`apps/desktop-electron/src/desktop-config.ts`
/// `withConfigLock`) and the CLI (`apps/nexus42/src/config.rs`
/// `CliConfig::mutate`, including its legacy-JSON migration) use the same
/// `<daemon home>/config.toml.lock` protocol, so the lock excludes writers in
/// other processes, not just in this one.
///
/// # Errors
/// Returns the mapped internal carrier when the lock cannot be taken, then
/// whatever `run` returns.
pub fn with_config_lock<T>(
    nexus_home: &Path,
    run: impl FnOnce() -> CoreResult<T>,
) -> CoreResult<T> {
    let _lock = acquire(nexus_home)?;
    run()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    fn lock_path(home: &Path) -> PathBuf {
        home.join("config.toml.lock")
    }

    fn config_path(home: &Path) -> PathBuf {
        home.join("config.toml")
    }

    /// A foreign process holding the lock blocks the core writer for the whole
    /// hold; nothing about the in-process caller is visible to that holder.
    #[test]
    fn config_write_waits_while_a_foreign_process_holds_the_lock() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);
        std::fs::write(&lock, format!("{} foreign\n", std::process::id())).expect("foreign lock");

        let done = Arc::new(AtomicBool::new(false));
        let writer = {
            let done = Arc::clone(&done);
            let home = home.clone();
            std::thread::spawn(move || {
                crate::home::write_active_selection(&home, "ctr_localtest0001", None)
                    .expect("selection write");
                done.store(true, Ordering::SeqCst);
            })
        };

        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !done.load(Ordering::SeqCst),
            "the config write must wait for config.toml.lock"
        );

        std::fs::remove_file(&lock).expect("release foreign lock");
        writer.join().expect("writer thread");
        assert!(done.load(Ordering::SeqCst));
        assert!(!lock.exists(), "the writer released its own lock");
        assert!(config_path(&home).exists());
    }

    /// A lock older than 10 s is treated as stale, so the next writer steals it
    /// instead of blocking behind a presumed-dead holder.
    #[test]
    fn a_stale_lock_is_stolen() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);
        std::fs::write(&lock, "dead\n").expect("stale lock");
        let stale = std::fs::File::options()
            .write(true)
            .open(&lock)
            .expect("open stale lock");
        stale
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .expect("age the stale lock");

        crate::home::write_active_selection(&home, "ctr_localtest0001", None)
            .expect("selection write behind a stale lock");

        assert!(!lock.exists());
        let stored = std::fs::read_to_string(config_path(&home)).expect("config");
        assert!(stored.contains("ctr_localtest0001"));
    }

    /// Two of the four RMW units — `set_active_creator_id` (writes
    /// `active_creator_id`) and `write_active_selection` (writes both that key
    /// and the slug table) — interleaved on the same document. A lost update
    /// shows as the slug table disappearing from a later reader's snapshot.
    #[test]
    fn interleaved_rmw_units_do_not_lose_each_others_keys() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();

        for round in 0..40 {
            let first = {
                let home = home.clone();
                std::thread::spawn(move || {
                    crate::creators::set_active_creator_id(&home, "ctr_localcreatora")
                        .expect("set active creator");
                })
            };
            let second = {
                let home = home.clone();
                std::thread::spawn(move || {
                    crate::home::write_active_selection(&home, "ctr_localcreatorb", Some("ws1"))
                        .expect("selection write");
                })
            };
            first.join().expect("first writer");
            second.join().expect("second writer");

            let stored = std::fs::read_to_string(config_path(&home)).expect("config");
            let document: toml::Table = stored.parse().expect("valid toml");
            assert!(
                document.get("active_creator_id").is_some(),
                "round {round}: active_creator_id lost"
            );
            assert!(
                document.get("active_workspace_slug_by_creator").is_some(),
                "round {round}: the slug table was dropped by a lost update"
            );
        }
    }
}
