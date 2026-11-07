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
//!
//! I-000291: mutual exclusion alone is not enough. A reader that could not
//! create the lock (a readable but not writable `.nexus42`) used to fail the
//! whole open, while an in-place `std::fs::write` exposed a truncated document
//! to any reader that skipped the lock. [`publish_document`] is the second half
//! of the contract: every whole-document writer replaces `config.toml` by an
//! atomic rename, so a reader always opens one complete version — the old or
//! the new one — and needs no directory write of its own.
//!
//! I-000299: an atomic replacement must not lose the old document's access
//! policy or expose staging bytes. A private staging directory is created and
//! sealed first, the staging file is created owner-only inside it before any
//! byte, the captured owner/group/mode/ACL/extended attributes are applied
//! before the rename (through the audited `nexus-config-metadata` bridge), and a
//! policy that cannot be captured or applied refuses the publication instead of
//! silently widening access.

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
/// Bounded link-following for a symlinked `config.toml`, matching the common OS
/// limit so a link loop fails instead of spinning.
const MAX_SYMLINK_HOPS: usize = 40;

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

/// Publish the complete `config.toml` document atomically.
///
/// I-000291: readers must see one whole document without taking the lock or
/// writing anything, so an already-provisioned but not writable `.nexus42` can
/// still serve a `ReadOnly` open. Writing a sibling staging file and renaming it
/// over `config.toml` makes the replacement atomic: a reader opens either the
/// previous complete version or the new one, never a truncated document that
/// would read as "no `[memory_provider]`" (the implicit local default).
///
/// The caller still holds [`acquire`]: publication replaces in-place
/// truncation, it does not replace writer exclusion (read-modify-write
/// lost-update safety).
///
/// # Access policy
///
/// The replacement keeps the old document's access policy. A private staging
/// directory is created and sealed first (no inherited ACL or default ACL can
/// reach the file), then the staging file is created owner-only (`0600`, the
/// narrowest mode that still allows writing), so no document byte is staged
/// under a policy broader than the target's; the captured owner, group, mode,
/// ACL and extended attributes are then applied before the rename. When the
/// policy cannot be captured or applied — an unreadable document, a write-denied
/// document, an ownership this process cannot reproduce, an attribute it cannot
/// set — publication refuses and the old bytes stay in place.
///
/// A rename is authorized by the containing directory, not by the file, so the
/// previous write boundary is rechecked by opening an existing target for
/// writing without truncating before anything is published. A symlinked document
/// is followed (the link survives and its real target is replaced), including a
/// dangling link whose destination is creatable, as the in-place write this
/// replaces did. A hard-linked document is refused on every platform — `stat` on
/// Unix, the file handle on Windows — because replacing it would detach the other
/// path instead of updating it; an unreadable link count refuses as well.
///
/// # Errors
/// The I/O error that prevented the policy capture, the authorization check, the
/// write, the sync, the policy application or the rename. The staged file and its
/// directory are removed on every failure path; when that cleanup also fails, the
/// returned error keeps the original kind and names every leftover path instead
/// of discarding the cleanup failure.
pub fn publish_document(nexus_home: &Path, contents: &str) -> std::io::Result<()> {
    let requested = nexus_home.join("config.toml");
    let target = resolve_publication_target(&requested)?;
    let existing = existing_document(&target)?;

    if existing.is_some() {
        // A hard link shares its inode with another path; an atomic replacement
        // would silently detach that path instead of updating it. The count is
        // read natively on every platform, and an indeterminate count refuses.
        if nexus_config_metadata::link_count(&target)? > 1 {
            return Err(std::io::Error::other(format!(
                "refusing to replace the hard-linked document {}",
                target.display()
            )));
        }
        // Retain the previous write boundary: the rename is authorized by the
        // directory and must not replace a document this process may not write.
        let _authorized = std::fs::OpenOptions::new().write(true).open(&target)?;
    }

    let policy = match existing {
        Some(_) => Some(nexus_config_metadata::capture(&target)?),
        None => None,
    };

    // The staging directory must share the target's directory: a symlinked
    // document can live outside the nexus home, and a cross-filesystem rename
    // would not be atomic (or would fail outright).
    let parent = target.parent().unwrap_or(nexus_home);
    let staging = nexus_config_metadata::create_staging(
        &parent.join(format!(".nexus42-{}.tmp", uuid::Uuid::new_v4())),
    )?;
    let staging_path = staging.path().to_path_buf();
    let staging_directory = staging.directory().to_path_buf();

    #[cfg(test)]
    after_staging(&staging_directory);

    let staged = stage_document(staging.file(), contents, policy.as_ref(), &staging_path);
    // Close the staging handle before the rename publishes the bytes.
    drop(staging);
    if let Err(error) = staged {
        return Err(cleanup_staging(&staging_path, &staging_directory, error));
    }

    if let Err(error) = std::fs::rename(&staging_path, &target) {
        return Err(cleanup_staging(&staging_path, &staging_directory, error));
    }

    // The target now owns the document, so the staging directory is empty. A
    // leftover empty directory is cosmetic: report it, never fail the publish.
    if let Err(error) = std::fs::remove_dir(&staging_directory) {
        tracing::warn!(
            %error,
            path = %staging_directory.display(),
            "the config was published but its empty staging directory could not be removed"
        );
    }
    Ok(())
}

/// Remove the staged file and its directory after a failed publication.
///
/// The primary failure keeps its kind and message; every cleanup failure and
/// its exact leftover path is appended so the caller sees both. A failed
/// cleanup is never discarded: the whole document may still exist there.
fn cleanup_staging(staging: &Path, directory: &Path, primary: std::io::Error) -> std::io::Error {
    let mut failures = Vec::new();
    for path in [staging, directory] {
        let outcome = if path == directory {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        };
        match outcome {
            Ok(()) => {}
            // Nothing was written at this path (e.g. the directory is gone).
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                failures.push(format!("{} could not be removed: {error}", path.display()));
            }
        }
    }
    if failures.is_empty() {
        return primary;
    }
    tracing::warn!(
        error = %primary,
        cleanup = %failures.join("; "),
        "config.toml publication failed and its staging files could not all be removed"
    );
    std::io::Error::new(
        primary.kind(),
        format!("{primary}; {}", failures.join("; ")),
    )
}

/// Write the staged document and apply the captured access policy.
fn stage_document(
    file: &std::fs::File,
    contents: &str,
    policy: Option<&nexus_config_metadata::AccessPolicy>,
    staging: &Path,
) -> std::io::Result<()> {
    let mut file = file;
    file.write_all(contents.as_bytes())?;
    // Complete the bytes before the rename publishes them: a crash after the
    // rename must not leave a shorter, empty-looking document.
    file.sync_all()?;
    nexus_config_metadata::apply_after_write(policy, file, staging)
}

/// The existing document's metadata, refusing a path that is not a regular file.
fn existing_document(target: &Path) -> std::io::Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Some(metadata)),
        Ok(_) => Err(std::io::Error::other(format!(
            "refusing to replace the non-regular document {}",
            target.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Resolve the document a publication replaces.
///
/// A symlinked `config.toml` is followed like the in-place write did, so the
/// link keeps pointing at the real document and the real document is replaced.
/// A dangling link's destination is used (the in-place write created it); every
/// other metadata error is propagated rather than treated as "absent". At most
/// [`MAX_SYMLINK_HOPS`] links are followed; the terminal path after the last
/// allowed hop is still inspected, so a valid chain of exactly that length is
/// accepted and only another hop is refused.
fn resolve_publication_target(requested: &Path) -> std::io::Result<PathBuf> {
    let mut current = requested.to_path_buf();
    let mut followed = 0_usize;
    loop {
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if followed == MAX_SYMLINK_HOPS {
                    return Err(std::io::Error::other(
                        "too many symbolic links in the config document path",
                    ));
                }
                followed += 1;
                let destination = std::fs::read_link(&current)?;
                current = if destination.is_absolute() {
                    destination
                } else {
                    current.parent().unwrap_or(requested).join(destination)
                };
            }
            Ok(_) => return Ok(current),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(current),
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only fault injection: the lock file is created, but its ownership
    /// token never reaches the disk.
    static FAIL_TOKEN_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The armed side of the post-staging rendezvous: the parent directory to
/// match, the arrival signal and the release signal.
#[cfg(test)]
type StagingRendezvous = (
    PathBuf,
    std::sync::mpsc::Sender<()>,
    std::sync::mpsc::Receiver<()>,
);

#[cfg(test)]
static AFTER_STAGING: std::sync::Mutex<Option<StagingRendezvous>> = std::sync::Mutex::new(None);

/// Serializes the post-staging regressions: they share one process-wide
/// rendezvous slot and must not arm it concurrently.
#[cfg(test)]
static STAGING_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Test-only rendezvous after the staging file exists and before any byte is
/// written: a regression can move the target at a real publication boundary
/// without a timing sleep. Armed by the target's parent directory (the staging
/// directory's own parent) and disarmed once fired.
#[cfg(test)]
fn after_staging(staging_directory: &Path) {
    let armed = {
        // A panic elsewhere must not turn a publication into a second panic: the
        // slot is advisory test state, so a poisoned lock is still usable.
        let mut slot = AFTER_STAGING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let matches = matches!(
            slot.as_ref(),
            Some((parent, _, _)) if staging_directory.parent() == Some(parent.as_path())
        );
        if matches {
            slot.take()
        } else {
            None
        }
    };
    if let Some((_, arrived, release)) = armed {
        arrived.send(()).expect("staging rendezvous arrival");
        release.recv().expect("staging rendezvous release");
    }
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
    use std::sync::{mpsc, Arc};
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

    /// I-000291: the publication half replaces the whole document by an atomic
    /// rename, keeps the existing mode instead of loosening it, and leaves no
    /// temporary file behind for a reader to mistake for the document.
    #[cfg(unix)]
    #[test]
    fn a_published_document_replaces_the_whole_file_and_keeps_its_mode() {
        use std::os::unix::fs::PermissionsExt as _;

        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(&home).expect("home");
        let path = config_path(&home);
        std::fs::write(&path, "active_creator_id = \"ctr_old\"\n").expect("old document");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("mode");

        super::publish_document(&home, "active_creator_id = \"ctr_new\"\n").expect("publish");

        assert_eq!(
            std::fs::read_to_string(&path).expect("published document"),
            "active_creator_id = \"ctr_new\"\n"
        );
        let mode = std::fs::metadata(&path)
            .expect("published metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640, "the existing document mode is preserved");
        let mut entries: Vec<_> = std::fs::read_dir(&home)
            .expect("home listing")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            ["config.toml"],
            "only the published document remains"
        );
    }

    /// A symlinked `config.toml` is followed like the in-place write was: the
    /// link survives, the document it points at is replaced atomically, and the
    /// temporary file is created beside that document.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_document_is_replaced_through_its_link() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().join("home");
        let real_dir = tmp.path().join("real");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&real_dir).expect("real directory");
        let real = real_dir.join("config.toml");
        std::fs::write(&real, "active_creator_id = \"ctr_old\"\n").expect("document");
        symlink(&real, config_path(&home)).expect("link");

        super::publish_document(&home, "active_creator_id = \"ctr_new\"\n").expect("publish");

        assert!(
            std::fs::symlink_metadata(config_path(&home))
                .expect("link metadata")
                .file_type()
                .is_symlink(),
            "the symlink survives publication"
        );
        assert_eq!(
            std::fs::read_to_string(&real).expect("real document"),
            "active_creator_id = \"ctr_new\"\n"
        );
        let mut entries: Vec<_> = std::fs::read_dir(&real_dir)
            .expect("real directory listing")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            ["config.toml"],
            "the temporary file lives beside the real document"
        );
    }

    /// SEC-01: a rename is authorized by the directory, so publishing a
    /// write-denied document would bypass the in-place write boundary. The
    /// publication must refuse it and leave the document (and no temporary
    /// file) behind.
    #[cfg(unix)]
    #[test]
    fn a_write_denied_document_is_not_replaced_by_publication() {
        use std::os::unix::fs::PermissionsExt as _;

        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(&home).expect("home");
        let path = config_path(&home);
        std::fs::write(&path, "active_creator_id = \"ctr_locked\"\n").expect("document");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("seal");

        let error = super::publish_document(&home, "active_creator_id = \"ctr_new\"\n")
            .expect_err("a write-denied document is not replaced");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("unseal");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::read_to_string(&path).expect("document"),
            "active_creator_id = \"ctr_locked\"\n",
            "the refused document keeps its content"
        );
        let mut entries: Vec<_> = std::fs::read_dir(&home)
            .expect("home listing")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(entries, ["config.toml"], "the temporary file was removed");
    }

    /// A document path that is not a regular file is refused before any staging
    /// file is created.
    #[test]
    fn a_non_regular_document_is_refused_before_staging() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(config_path(&home)).expect("directory at the document path");

        let error = super::publish_document(&home, "active_creator_id = \"ctr_new\"\n")
            .expect_err("a directory cannot be published over");

        assert!(
            error.to_string().contains("non-regular"),
            "the refusal names the document kind: {error}"
        );
        let mut entries: Vec<_> = std::fs::read_dir(&home)
            .expect("home listing")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(entries, ["config.toml"], "no staging file was created");
    }

    /// SEC-02: the staging file is created owner-only, before any byte is
    /// written, so the document is never exposed under a broader policy.
    #[cfg(unix)]
    #[test]
    fn the_staging_file_is_created_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let tmp = tempfile::tempdir().expect("temp home");
        let staging = nexus_config_metadata::create_staging(&tmp.path().join(".nexus42-test.tmp"))
            .expect("staging file");
        let mode = staging
            .file()
            .metadata()
            .expect("staging metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the staging file is owner-only");
        let directory_mode = std::fs::metadata(staging.directory())
            .expect("staging directory metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode, 0o700, "the staging directory is private");
    }

    /// SEC-03: the replacement keeps the target's owner, group, mode and access
    /// policy (ACL and extended attributes) instead of installing a fresh inode
    /// with the directory's defaults.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_published_document_keeps_the_target_access_policy() {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(&home).expect("home");
        let path = config_path(&home);
        std::fs::write(&path, "active_creator_id = \"ctr_old\"\n").expect("document");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("mode");
        // A named deny entry plus a user attribute: both belong to the policy an
        // atomic replacement must carry over.
        let acl = std::process::Command::new("chmod")
            .arg("+a")
            .arg("nobody deny read")
            .arg(&path)
            .status()
            .expect("run chmod");
        assert!(acl.success(), "the deny ACL was installed");
        let attribute = std::process::Command::new("xattr")
            .arg("-w")
            .arg("com.nexus.config-test")
            .arg("kept")
            .arg(&path)
            .status()
            .expect("run xattr");
        assert!(attribute.success(), "the extended attribute was installed");

        let before = std::fs::metadata(&path).expect("metadata");
        let acl_before = nexus_config_metadata::access_policy_blob(&path).expect("acl before");
        assert!(!acl_before.is_empty(), "the target really has an ACL");

        super::publish_document(&home, "active_creator_id = \"ctr_new\"\n").expect("publish");

        let after = std::fs::metadata(&path).expect("metadata");
        let acl_after = nexus_config_metadata::access_policy_blob(&path).expect("acl after");
        assert_eq!(acl_before, acl_after, "the access ACL survives");
        assert_eq!(before.uid(), after.uid(), "the owner survives");
        assert_eq!(before.gid(), after.gid(), "the group survives");
        assert_eq!(
            after.permissions().mode() & 0o777,
            0o640,
            "the mode survives"
        );
        let value = std::process::Command::new("xattr")
            .arg("-p")
            .arg("com.nexus.config-test")
            .arg(&path)
            .output()
            .expect("read the attribute");
        assert!(value.status.success(), "the extended attribute is present");
        assert_eq!(
            String::from_utf8_lossy(&value.stdout).trim(),
            "kept",
            "the extended attribute value survives"
        );
    }

    /// A hard-linked document is refused: an atomic replacement would detach the
    /// other path instead of updating it.
    #[cfg(unix)]
    #[test]
    fn a_hard_linked_document_is_refused_unchanged() {
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(&home).expect("home");
        let path = config_path(&home);
        std::fs::write(&path, "active_creator_id = \"ctr_old\"\n").expect("document");
        let other = home.join("other.toml");
        std::fs::hard_link(&path, &other).expect("hard link");

        let error = super::publish_document(&home, "active_creator_id = \"ctr_new\"\n")
            .expect_err("a hard-linked document is refused");

        assert!(
            error.to_string().contains("hard-linked"),
            "the refusal names the hard link: {error}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("document"),
            "active_creator_id = \"ctr_old\"\n"
        );
        assert_eq!(
            std::fs::read_to_string(&other).expect("other link"),
            "active_creator_id = \"ctr_old\"\n",
            "the other link keeps its bytes"
        );
    }

    /// A dangling symlink whose destination is creatable publishes to the
    /// destination and keeps the link (the selected contract keeps ordinary and
    /// creatable links, only refusing hard links and unsafe atomic paths).
    #[cfg(unix)]
    #[test]
    fn a_creatable_dangling_symlink_is_published_through() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().join("home");
        let real_directory = tmp.path().join("real");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&real_directory).expect("real directory");
        let real = real_directory.join("config.toml");
        symlink(&real, config_path(&home)).expect("dangling link");

        super::publish_document(&home, "active_creator_id = \"ctr_new\"\n").expect("publish");

        assert!(
            std::fs::symlink_metadata(config_path(&home))
                .expect("link metadata")
                .file_type()
                .is_symlink(),
            "the link survives"
        );
        assert_eq!(
            std::fs::read_to_string(&real).expect("destination"),
            "active_creator_id = \"ctr_new\"\n"
        );
    }

    /// The single staging directory in `home`.
    fn only_staging_directory(home: &Path) -> PathBuf {
        let mut staging: Vec<PathBuf> = std::fs::read_dir(home)
            .expect("home listing")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".nexus42-"))
            })
            .collect();
        assert_eq!(staging.len(), 1, "exactly one staging directory exists");
        staging.pop().expect("staging directory")
    }

    /// Arm the post-staging rendezvous for a publisher whose target is in
    /// `home`: it blocks after creating its staging file until released.
    fn arm_after_staging(home: &Path) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (arrived, arrival) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let mut slot = super::AFTER_STAGING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(slot.is_none(), "no other test armed the rendezvous");
        *slot = Some((home.to_path_buf(), arrived, released));
        drop(slot);
        (arrival, release)
    }

    /// A failure after the staging file exists must keep the old document and
    /// remove the staged file and its directory: ordinary cleanup, at a real
    /// publication boundary instead of an injected error.
    ///
    /// The original document stays a regular file with its old bytes: a cleanup
    /// branch that unlinked the target instead of the owned stage is then
    /// observable, which a fixture that replaces the target cannot show.
    #[cfg(unix)]
    #[test]
    fn a_post_staging_failure_keeps_the_old_document_and_removes_the_stage() {
        let _serial = super::STAGING_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(&home).expect("home");
        let path = config_path(&home);
        std::fs::write(&path, "active_creator_id = \"ctr_old\"\n").expect("document");

        let (arrived, release) = arm_after_staging(&home);
        let publisher = std::thread::spawn({
            let home = home.clone();
            move || super::publish_document(&home, "active_creator_id = \"ctr_new\"\n")
        });

        arrived
            .recv_timeout(Duration::from_secs(30))
            .expect("the publisher reached the staging boundary");
        // The staged file disappears under the publisher, so the rename is the
        // failing operation while the original document is still in place.
        let staging_directory = only_staging_directory(&home);
        std::fs::remove_file(staging_directory.join("config.toml"))
            .expect("the staged file was removed at the boundary");
        release.send(()).expect("release the publisher");

        let error = publisher
            .join()
            .expect("publisher thread")
            .expect_err("the rename has no staged file to publish");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("document"),
            "active_creator_id = \"ctr_old\"\n",
            "the original document keeps its bytes"
        );
        let entries: Vec<_> = std::fs::read_dir(&home)
            .expect("home listing")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            entries,
            ["config.toml"],
            "the original document survives and the owned staging directory was removed"
        );
    }

    /// A post-staging failure whose cleanup also fails reports both, at a real
    /// publication boundary instead of constructed errors.
    ///
    /// The directory stops accepting writes, so the rename fails on the
    /// destination and removing the owned staging directory fails too. The
    /// original document stays a regular file: the reported error must keep the
    /// primary kind and message and still name the leftover path.
    #[cfg(unix)]
    #[test]
    fn a_post_staging_failure_reports_a_failed_cleanup() {
        use std::os::unix::fs::PermissionsExt as _;

        let _serial = super::STAGING_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path().to_path_buf();
        std::fs::create_dir_all(&home).expect("home");
        let path = config_path(&home);
        std::fs::write(&path, "active_creator_id = \"ctr_old\"\n").expect("document");

        let (arrived, release) = arm_after_staging(&home);
        let publisher = std::thread::spawn({
            let home = home.clone();
            move || super::publish_document(&home, "active_creator_id = \"ctr_new\"\n")
        });

        arrived
            .recv_timeout(Duration::from_secs(30))
            .expect("the publisher reached the staging boundary");
        let staging_directory = only_staging_directory(&home);
        // The sample source is created while the home is still writable; only
        // the rename into the home is the native failure being sampled.
        let sample_source = tmp.path().join("sample-source");
        std::fs::write(&sample_source, "sample").expect("sample source");
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500))
            .expect("the home stops accepting directory writes");
        // Sample the native rename failure this home really produces, without
        // moving the staged file or the document.
        let primary = std::fs::rename(&sample_source, home.join("sample-target"))
            .expect_err("the home does not accept a rename");
        release.send(()).expect("release the publisher");

        let error = publisher
            .join()
            .expect("publisher thread")
            .expect_err("the rename cannot write into the home");
        // The cleanup failure is sampled from the same leftover directory while
        // the home is still non-writable.
        let cleanup = std::fs::remove_dir(&staging_directory)
            .expect_err("the leftover directory cannot be removed yet");
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755))
            .expect("restore the home");
        assert_eq!(
            error.kind(),
            primary.kind(),
            "the primary failure keeps its kind"
        );
        let rendered = error.to_string();
        assert!(
            rendered.starts_with(&primary.to_string()),
            "the primary message is retained: {rendered}"
        );
        let suffix = rendered
            .strip_prefix(&primary.to_string())
            .expect("the primary message is the prefix");
        assert!(
            suffix.contains(&cleanup.to_string()),
            "the cleanup failure is reported after the primary message: {rendered}"
        );
        assert!(
            rendered.contains(&staging_directory.display().to_string()),
            "{rendered}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("document"),
            "active_creator_id = \"ctr_old\"\n",
            "the original document keeps its bytes"
        );
        assert!(
            staging_directory.is_dir(),
            "the staging directory is the reported leftover"
        );
        assert!(
            !staging_directory.join("config.toml").exists(),
            "the staged file was removed"
        );
    }

    /// A symlink chain of exactly the allowed length publishes; one more hop and
    /// a loop are refused before any staged byte exists.
    #[cfg(unix)]
    #[test]
    fn the_symlink_hop_limit_is_enforced_at_its_boundary() {
        use std::os::unix::fs::symlink;

        /// Build `links` symlinks, `config.toml` first, ending at `real.toml`.
        fn chain(home: &Path, links: usize) {
            for index in 0..links {
                let name = if index == 0 {
                    "config.toml".to_string()
                } else {
                    format!("link-{index}")
                };
                let target = if index + 1 == links {
                    "real.toml".to_string()
                } else {
                    format!("link-{}", index + 1)
                };
                symlink(target, home.join(name)).expect("chain link");
            }
        }

        let tmp = tempfile::tempdir().expect("temp home");
        let at_limit = tmp.path().join("at-limit");
        std::fs::create_dir_all(&at_limit).expect("home");
        std::fs::write(
            at_limit.join("real.toml"),
            "active_creator_id = \"ctr_old\"\n",
        )
        .expect("document");
        chain(&at_limit, super::MAX_SYMLINK_HOPS);

        super::publish_document(&at_limit, "active_creator_id = \"ctr_new\"\n")
            .expect("a chain at the hop limit is followed");
        assert_eq!(
            std::fs::read_to_string(at_limit.join("real.toml")).expect("document"),
            "active_creator_id = \"ctr_new\"\n"
        );

        let beyond = tmp.path().join("beyond");
        std::fs::create_dir_all(&beyond).expect("home");
        std::fs::write(
            beyond.join("real.toml"),
            "active_creator_id = \"ctr_old\"\n",
        )
        .expect("document");
        chain(&beyond, super::MAX_SYMLINK_HOPS + 1);

        let _error = super::publish_document(&beyond, "active_creator_id = \"ctr_new\"\n")
            .expect_err("one hop past the limit is refused");
        assert_eq!(
            std::fs::read_to_string(beyond.join("real.toml")).expect("document"),
            "active_creator_id = \"ctr_old\"\n",
            "the document past an over-long chain keeps its bytes"
        );

        let looping = tmp.path().join("loop");
        std::fs::create_dir_all(&looping).expect("home");
        symlink("a.toml", looping.join("config.toml")).expect("first link");
        symlink("config.toml", looping.join("a.toml")).expect("second link");

        let _error = super::publish_document(&looping, "active_creator_id = \"ctr_new\"\n")
            .expect_err("a link loop is refused");
        assert_eq!(
            std::fs::read_dir(&looping).expect("home listing").count(),
            2,
            "the loop is refused without creating a document or staging directory"
        );
    }

    /// SEC-01: an inheritance-only `writesecurity` grant on the containing
    /// directory must not leave the staging directory writable. Sealing removes
    /// the inherited authority and re-establishes the owner-only mode before the
    /// child file exists.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_inherited_writesecurity_grant_does_not_leave_the_stage_writable() {
        use std::os::unix::fs::PermissionsExt as _;

        let tmp = tempfile::tempdir().expect("temp home");
        let home = tmp.path();
        let status = std::process::Command::new("chmod")
            .arg("+a")
            .arg("everyone allow read,writesecurity,file_inherit,directory_inherit")
            .arg(home)
            .status()
            .expect("run chmod");
        assert!(status.success(), "the inheritable grant is installed");

        let staging = nexus_config_metadata::create_staging(&home.join(".nexus42-test.tmp"))
            .expect("staging file");

        assert_eq!(
            std::fs::metadata(staging.directory())
                .expect("staging directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "the inherited grant cannot leave the staging directory writable"
        );
        assert!(
            nexus_config_metadata::access_policy_blob(staging.directory())
                .expect("staging ACL")
                .is_empty(),
            "the inherited ACE was removed before the child existed"
        );
    }
}
