//! I-000065: the daemon-home `config.toml` advisory lock.
//!
//! `<user_home>/.nexus42/config.toml` has four cross-process writers: this
//! core, the CLI (`apps/nexus42/src/config.rs`), the nexus-service profile
//! writer (`apps/nexus-service/src/agent-profile.ts`) and the Electron desktop
//! main (`apps/desktop-electron/src/desktop-config.ts`). They share ONE protocol:
//! exclusive-create of `<daemon home>/config.toml.lock` for the whole
//! read-modify-write, stale steal after 10 s, release by unlink of the file
//! this holder still owns (a per-holder token inside it is compared first). No
//! `flock`: Node's stdlib has none, and mixing lockfile-existence with flock
//! breaks mutual exclusion.

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
/// A steal retries while the stale file cannot be unlinked (a root-owned or
/// immutable file: EACCES/EPERM). Past this deadline the acquisition fails
/// instead of hot-looping — the same 2× stale window the service and desktop
/// writers use.
const STEAL_DEADLINE: Duration = Duration::from_secs(20);

/// The held lock: unlinking on drop releases it on every return path.
struct ConfigLock {
    path: PathBuf,
    /// Ownership token written into the lock file; release only unlinks a lock
    /// this holder still owns.
    token: String,
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        release(&self.path, &self.token);
    }
}

/// Release the lock, but only when this holder still owns it.
///
/// A holder stalled past [`STALE_AFTER`] is stolen; unlinking on resume would
/// delete the *new* owner's lock and admit a third writer into the second
/// writer's read-modify-write.
fn release(path: &Path, token: &str) {
    match std::fs::read_to_string(path) {
        Ok(contents) if contents.trim_end().ends_with(token) => {
            if let Err(error) = std::fs::remove_file(path) {
                tracing::warn!(%error, "config.toml.lock release failed");
            }
        }
        // Stolen or rewritten: the file belongs to another writer now.
        Ok(_) => tracing::warn!("config.toml.lock was stolen; leaving the new owner's lock"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "config.toml.lock release read failed"),
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
    let token = uuid::Uuid::new_v4().to_string();
    let mut backoff = BACKOFF_START;
    let mut steal_since: Option<SystemTime> = None;
    let mut logged_unlink_error = false;
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                // Diagnostics only (who/what/when); the age comes from the
                // mtime. The trailing token is the ownership proof release
                // checks before unlinking.
                let _ = writeln!(
                    file,
                    "{} {} {token}",
                    std::process::id(),
                    chrono::Utc::now().to_rfc3339()
                );
                return Ok(ConfigLock { path, token });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let age = std::fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok());
                match age {
                    // Released between the create and the stat: retry at once.
                    None => {}
                    Some(age) if age > STALE_AFTER => {
                        // Stale age is a heuristic: two reclaimers can both
                        // unlink and create, leaving two live holders, and a
                        // live slow holder can lose its lock here. Accepted only
                        // because all four writers are user-paced.
                        let started = *steal_since.get_or_insert_with(SystemTime::now);
                        match std::fs::remove_file(&path) {
                            Ok(()) => {}
                            // Another reclaimer got there first.
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => {
                                if !logged_unlink_error {
                                    logged_unlink_error = true;
                                    tracing::warn!(
                                        %error,
                                        "cannot remove the stale config.toml.lock"
                                    );
                                }
                                if started.elapsed().unwrap_or_default() > STEAL_DEADLINE {
                                    return Err(lock_error(format!(
                                        "stale {LOCK_FILE} cannot be removed: {error}"
                                    )));
                                }
                            }
                        }
                        // Back off here too: an unremovable stale lock must not
                        // hot-loop the create/stat cycle.
                        std::thread::sleep(backoff);
                        backoff = (backoff * 2).min(BACKOFF_CAP);
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
/// lock.
///
/// Every writer of this file takes it — the service
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
                crate::home::write_active_selection(&home, "ctr_localtest0001", None, None)
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

        crate::home::write_active_selection(&home, "ctr_localtest0001", None, None)
            .expect("selection write behind a stale lock");

        assert!(!lock.exists());
        let stored = std::fs::read_to_string(config_path(&home)).expect("config");
        assert!(stored.contains("ctr_localtest0001"));
    }

    /// Two RMW units with different key sets — a creator-only selection write
    /// and a full `write_active_selection` (that key plus the slug table) —
    /// interleaved on the same document. A lost update shows as the other
    /// writer's keys disappearing from a later reader's snapshot.
    #[test]
    fn interleaved_rmw_units_do_not_lose_each_others_keys() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();

        for round in 0..40 {
            let first = {
                let home = home.clone();
                std::thread::spawn(move || {
                    crate::home::write_active_selection(&home, "ctr_localcreatora", None, None)
                        .expect("creator-only selection write");
                })
            };
            let second = {
                let home = home.clone();
                std::thread::spawn(move || {
                    crate::home::write_active_selection(
                        &home,
                        "ctr_localcreatorb",
                        Some("ws1"),
                        None,
                    )
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

    /// MF-2 (I-000091): a stale lock that cannot be unlinked (EACCES/EPERM on
    /// a root-owned or immutable file) must fail the acquisition instead of
    /// hot-looping create/stat forever. A directory at the lock path is the
    /// portable stand-in: exclusive-create reports `EEXIST` for it and
    /// `remove_file` never removes it.
    #[cfg(unix)]
    #[test]
    fn an_unremovable_stale_lock_fails_instead_of_spinning() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);
        std::fs::create_dir(&lock).expect("unremovable stale lock");
        std::fs::File::open(&lock)
            .expect("open the lock directory")
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .expect("age the stale lock");

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(crate::home::write_active_selection(
                &home,
                "ctr_localtest0001",
                None,
                None,
            ));
        });

        let error = receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("the writer must give up instead of spinning")
            .expect_err("an unremovable stale lock is an error");
        assert!(matches!(error, crate::error::CoreError::Internal { .. }));
    }

    /// SF-1 (I-000092): a holder stalled past the stale window is stolen; the
    /// release on its resume must not unlink the *new* owner's lock.
    #[test]
    fn a_stolen_holder_leaves_the_new_owners_lock_in_place() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);

        super::with_config_lock(&home, || {
            // Inside the read-modify-write our lock was stolen and replaced by
            // the new owner's file.
            std::fs::write(&lock, "999999 stolen\n").expect("new owner's lock");
            Ok(())
        })
        .expect("the RMW itself succeeds");

        assert!(
            lock.exists(),
            "a stolen holder must not unlink the new owner's lock"
        );
    }
}
