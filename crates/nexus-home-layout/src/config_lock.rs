//! I-000065/I-000113: the daemon-home `config.toml` advisory lock.
//!
//! `<user_home>/.nexus42/config.toml` has five cross-process writers: the core
//! (`nexus-core`), the CLI (`apps/nexus42/src/config.rs`), the nexus-service
//! profile writer (`apps/nexus-service/src/agent-profile.ts`), the Electron
//! desktop main (`apps/desktop-electron/src/desktop-config.ts`) and this
//! crate's startup skeleton writer [`crate::ensure_system_layout`]. They share
//! ONE protocol: exclusive-create of `<daemon home>/config.toml.lock` for the
//! whole read-modify-write, stale steal after 10 s, release by unlink of the
//! file this holder still owns (the whole token line inside it is compared
//! first). No `flock`: Node's stdlib has none, and mixing lockfile-existence
//! with flock breaks mutual exclusion.
//!
//! The protocol lives here — the lowest crate every writer can depend on —
//! rather than in `nexus-core` (which depends on this crate): the startup
//! writer needs it too.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const LOCK_FILE: &str = "config.toml.lock";
/// A normal hold is milliseconds (a small TOML document, at most two fsyncs).
/// Ten seconds is a stale heuristic, not proof of death: a paused or suspended
/// live holder can look stale, and stealing can transiently leave two holders.
/// Safe only because all five writers are user-paced — nothing enforces that.
const STALE_AFTER: Duration = Duration::from_secs(10);
const BACKOFF_START: Duration = Duration::from_millis(25);
const BACKOFF_CAP: Duration = Duration::from_millis(500);
/// A steal retries while the stale file cannot be unlinked (a root-owned or
/// immutable file: EACCES/EPERM). Past this deadline the acquisition fails
/// instead of hot-looping — the same 2× stale window the service and desktop
/// writers use.
const STEAL_DEADLINE: Duration = Duration::from_secs(20);

/// The held lock: unlinking on drop releases it on every return path.
pub struct ConfigLock {
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
/// writer's read-modify-write. The whole line is the token, so the compare is
/// byte-for-byte: a file that merely contains or ends with our token belongs to
/// another writer.
fn release(path: &Path, token: &str) {
    match std::fs::read_to_string(path) {
        Ok(contents) if contents.strip_suffix('\n').unwrap_or(&contents) == token => {
            // Residual TOCTOU (accepted, contract §2): the compare above and
            // this unlink are not atomic, so a stale-steal interleaving can
            // delete a new owner's lock. Atomizing (rename-then-verify) is
            // deferred while every writer stays user-paced.
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

#[cfg(test)]
thread_local! {
    /// Test-only fault injection: the lock file is created, but its ownership
    /// token never reaches the disk.
    static FAIL_TOKEN_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Exclusive-create options for the lock file.
///
/// Owner-only mode is applied at creation (umask can only tighten it), never a
/// post-create chmod, so the lock is never observable at a permissive mode.
/// win32 has no POSIX mode to set; the default DACL applies.
fn lock_options() -> std::fs::OpenOptions {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
}

/// Write the holder's line into the freshly created lock file.
fn write_token(file: &mut std::fs::File, token: &str) -> std::io::Result<()> {
    #[cfg(test)]
    {
        if FAIL_TOKEN_WRITE.with(|slot| slot.replace(false)) {
            return Err(std::io::Error::other("injected token write failure"));
        }
    }
    writeln!(file, "{token}")
}

/// Take the lock: exclusive create, stale steal, exponential backoff.
///
/// # Errors
/// The I/O error that prevented acquisition (permission, unremovable stale
/// lock after the deadline, or an unwritable ownership token).
pub fn acquire(nexus_home: &Path) -> std::io::Result<ConfigLock> {
    let path = nexus_home.join(LOCK_FILE);
    std::fs::create_dir_all(nexus_home)?;
    // The whole line, not just the UUID: release compares it byte-for-byte.
    let token = format!(
        "{} {} {}",
        std::process::id(),
        chrono::Utc::now().to_rfc3339(),
        uuid::Uuid::new_v4()
    );
    let mut backoff = BACKOFF_START;
    let mut steal_since: Option<SystemTime> = None;
    let mut logged_unlink_error = false;
    loop {
        match lock_options().open(&path) {
            Ok(mut file) => {
                // Diagnostics only (who/what/when); the age comes from the
                // mtime. The whole line is the ownership proof release checks
                // before unlinking.
                if let Err(write_error) = write_token(&mut file, &token) {
                    // Contract §3: a tokenless lock is unusable. Close the
                    // handle and unlink the file we just created instead of
                    // leaving the next writer to wait out the stale threshold;
                    // the mtime is fresh, so no other writer owns it yet.
                    drop(file);
                    if let Err(unlink_error) = std::fs::remove_file(&path) {
                        tracing::warn!(
                            %unlink_error,
                            "config.toml.lock cleanup after a failed token write failed"
                        );
                    }
                    tracing::warn!(
                        %write_error,
                        "config.toml.lock ownership token not written; lock unlinked"
                    );
                    return Err(write_error);
                }
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
                        // because all five writers are user-paced.
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
                                    return Err(std::io::Error::other(format!(
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
            Err(error) => return Err(error),
        }
    }
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

    /// One lock-scoped read-modify-write: insert `key` into the config document.
    fn add_key(home: &Path, key: &str) {
        let _lock = super::acquire(home).expect("config lock");
        let path = config_path(home);
        let stored = std::fs::read_to_string(&path).unwrap_or_default();
        let mut document: toml::Table = stored.parse().unwrap_or_default();
        document.insert(key.to_string(), toml::Value::String("kept".into()));
        std::fs::write(&path, document.to_string()).expect("config write");
    }

    /// A foreign process holding the lock blocks the writer for the whole hold;
    /// nothing about the in-process caller is visible to that holder.
    #[test]
    fn config_write_waits_while_a_foreign_process_holds_the_lock() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);
        std::fs::create_dir_all(&home).expect("home");
        std::fs::write(&lock, format!("{} foreign\n", std::process::id())).expect("foreign lock");

        let done = Arc::new(AtomicBool::new(false));
        let writer = {
            let done = Arc::clone(&done);
            let home = home.clone();
            std::thread::spawn(move || {
                add_key(&home, "active_creator_id");
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
        std::fs::create_dir_all(&home).expect("home");
        std::fs::write(&lock, "dead\n").expect("stale lock");
        let stale = std::fs::File::options()
            .write(true)
            .open(&lock)
            .expect("open stale lock");
        stale
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .expect("age the stale lock");

        add_key(&home, "active_creator_id");

        assert!(!lock.exists());
        let stored = std::fs::read_to_string(config_path(&home)).expect("config");
        assert!(stored.contains("active_creator_id"));
    }

    /// Two RMW units with different key sets interleaved on the same document.
    /// A lost update shows as the other writer's key disappearing.
    #[test]
    fn interleaved_rmw_units_do_not_lose_each_others_keys() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(&home).expect("home");

        for round in 0..40 {
            let first = {
                let home = home.clone();
                std::thread::spawn(move || add_key(&home, "active_creator_id"))
            };
            let second = {
                let home = home.clone();
                std::thread::spawn(move || add_key(&home, "active_workspace_slug_by_creator"))
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
                "round {round}: the other writer's key was dropped by a lost update"
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
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir(&lock).expect("unremovable stale lock");
        std::fs::File::open(&lock)
            .expect("open the lock directory")
            .set_modified(SystemTime::now() - Duration::from_secs(60))
            .expect("age the stale lock");

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(super::acquire(&home).map(|_| ()));
        });

        receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("the writer must give up instead of spinning")
            .expect_err("an unremovable stale lock is an error");
    }

    /// I-000107: a token write that fails after the exclusive create leaves an
    /// unusable lock. Acquire must close it, unlink the file it just created,
    /// and return the lock error at once — never strand a tokenless lock.
    #[test]
    fn a_token_that_cannot_be_written_unlinks_the_lock_and_fails() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);
        super::FAIL_TOKEN_WRITE.with(|slot| slot.set(true));
        let started = std::time::Instant::now();
        let result = super::acquire(&home);
        let elapsed = started.elapsed();
        super::FAIL_TOKEN_WRITE.with(|slot| slot.set(false));

        assert!(result.is_err(), "an unwritten ownership token is an error");
        assert!(
            !lock.exists(),
            "the tokenless lock must be unlinked before the error is returned"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "the caller must fail fast instead of waiting out the stale steal"
        );
    }

    /// I-000106: the lock is created owner-only, with the mode applied at
    /// creation (`OpenOptions::mode`), so it is never observable as 0644.
    #[cfg(unix)]
    #[test]
    fn the_lock_file_is_created_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);

        let guard = super::acquire(&home).expect("lock");
        let mode = std::fs::metadata(&lock)
            .expect("the held lock")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the lock must be created 0600");
        drop(guard);
        assert!(!lock.exists(), "the writer released its own lock");
    }

    /// I-000105: a lock whose line only *ends with* our token (a rewritten or
    /// foreign file) is not byte-identical to the held token and must never be
    /// unlinked.
    #[test]
    fn a_release_never_unlinks_a_lock_that_only_ends_with_our_token() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);

        let guard = super::acquire(&home).expect("lock");
        let owned = std::fs::read_to_string(&lock).expect("held lock");
        // Another writer's line prefixed onto ours: same trailing token,
        // different line.
        std::fs::write(&lock, format!("999999 foreign {owned}")).expect("foreign lock");
        drop(guard);

        assert!(
            lock.exists(),
            "release must compare the whole line, never a token suffix"
        );
    }

    /// SF-1 (I-000092): a holder stalled past the stale window is stolen; the
    /// release on its resume must not unlink the *new* owner's lock.
    #[test]
    fn a_stolen_holder_leaves_the_new_owners_lock_in_place() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        let lock = lock_path(&home);

        let guard = super::acquire(&home).expect("lock");
        // Inside the read-modify-write our lock was stolen and replaced by the
        // new owner's file.
        std::fs::write(&lock, "999999 stolen\n").expect("new owner's lock");
        drop(guard);

        assert!(
            lock.exists(),
            "a stolen holder must not unlink the new owner's lock"
        );
    }
}
