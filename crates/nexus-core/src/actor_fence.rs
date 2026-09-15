//! Host-free per-Character activity/transition fencing (v1.190 P2-T1).
//!
//! Separated from the process Host session registry (`ActorSessionRegistry`):
//! every Character DB/file/provider/terminal effect holds a shared activity
//! lease, and lifecycle/binding transitions take the corresponding exclusive
//! lease with a busy refusal (never forced cancellation, durable §11.3).
//!
//! Two layers, per the P2-T1 contract:
//!
//! 1. **Same-process shared fence** — one [`tokio::sync::RwLock`] per
//!    Character, held by the owning [`crate::CoreService`] fence table. It
//!    serializes leases/transitions flowing through the service without
//!    touching the session registry. Global shared-kernel state is
//!    prohibited, so the table is per-service; sharing across independently
//!    opened cores (and across processes) is carried by layer 2.
//! 2. **Stable per-Character OS shared/exclusive resource lock** — a
//!    `flock(2)` shared/exclusive pair on
//!    `<state-db dir>/character_locks/<character_id>.lock`, one descriptor
//!    per live lease. Descriptors in the same process contend with each
//!    other (per-open-file-description semantics), so two independently
//!    opened cores race correctly, and a direct CLI transition fences
//!    against a live service. Unix-only, matching the existing
//!    resource-lock discipline (`nexus-local-db::file_lock`,
//!    `workspace::authority`); on non-unix targets the OS layer is a no-op
//!    and the shared-DB write path stays WAL-governed.
//!
//! Stored `lifecycle_epoch` and ownership are re-read after acquiring the
//! fence (both layers) so the lease witnesses an exact pre-transition epoch
//! and `actor_session_stale` detection stays possible after a material
//! transition.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use crate::error::{CoreError, CoreResult};

/// Per-Character fence state owned by a [`crate::CoreService`].
pub(crate) struct ActorFenceTable {
    process: Mutex<HashMap<String, Arc<RwLock<()>>>>,
    locks_dir: PathBuf,
}

/// Shared activity lease (durable §11.3.1): hold through every DB/file/
/// provider/terminal-capture effect; drop releases both fence layers.
pub struct ActorActivityLease {
    _read: OwnedRwLockReadGuard<()>,
    #[cfg(unix)]
    _os: Option<OsSharedLock>,
    owner_creator_id: String,
    character_id: String,
    epoch: i64,
}

/// Exclusive transition lease (durable §11.3.2): busy refusal, epoch re-read
/// under the fence, updated to the committed epoch by
/// [`crate::CoreService::commit_character_transition`] so the host can retire
/// old-epoch sessions while the fence is still held (durable §11.3.3).
pub struct CharacterTransitionLease {
    _write: OwnedRwLockWriteGuard<()>,
    #[cfg(unix)]
    _os: Option<OsExclusiveLock>,
    owner_creator_id: String,
    character_id: String,
    epoch: std::sync::atomic::AtomicI64,
}

impl ActorFenceTable {
    pub(crate) fn new(db_path: &Path) -> Self {
        Self {
            process: Mutex::new(HashMap::new()),
            locks_dir: db_path
                .parent()
                .unwrap_or(Path::new("."))
                .join("character_locks"),
        }
    }

    /// Stable per-Character OS lock file path.
    #[cfg(unix)]
    fn lock_path(&self, character_id: &str) -> PathBuf {
        self.locks_dir.join(format!("{character_id}.lock"))
    }

    /// Fetch or create the per-Character process fence, sweeping entries with
    /// no live guard (live-Arc reclaim, mirroring the daemon §11.3.6 sweep).
    fn fence_for(&self, character_id: &str) -> Arc<RwLock<()>> {
        let mut fences = self
            .process
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        fences.retain(|_, fence| Arc::strong_count(fence) > 1);
        Arc::clone(
            fences
                .entry(character_id.to_string())
                .or_insert_with(|| Arc::new(RwLock::new(()))),
        )
    }

    /// Acquire the shared activity fence for one Character: process read
    /// guard first, then the blocking OS shared lock (off the async runtime).
    pub(crate) async fn acquire_activity(
        &self,
        character_id: &str,
    ) -> CoreResult<ActivityFenceParts> {
        let read = Arc::clone(&self.fence_for(character_id)).read_owned().await;
        #[cfg(unix)]
        {
            let os = OsSharedLock::acquire(self.lock_path(character_id)).await?;
            Ok(ActivityFenceParts {
                read,
                os: Some(os),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = &self.locks_dir;
            Ok(ActivityFenceParts { read })
        }
    }

    /// Try to take the exclusive transition fence (busy refusal, never
    /// waiting): process write guard, then the non-blocking OS exclusive
    /// lock. Any outstanding activity lease refuses with `character_busy`.
    pub(crate) async fn try_acquire_transition(
        &self,
        character_id: &str,
    ) -> CoreResult<TransitionFenceParts> {
        let write = Arc::clone(&self.fence_for(character_id))
            .try_write_owned()
            .map_err(|_| busy_error(character_id))?;
        #[cfg(unix)]
        {
            let os = OsExclusiveLock::try_acquire(self.lock_path(character_id))
                .map_err(|err| {
                    if is_lock_busy(&err) {
                        busy_error(character_id)
                    } else {
                        CoreError::Internal {
                            category: format!("character_lock: {err}"),
                        }
                    }
                })?;
            Ok(TransitionFenceParts {
                write,
                os: Some(os),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = &self.locks_dir;
            Ok(TransitionFenceParts { write })
        }
    }
}

/// Raw fence halves handed to the lease constructors.
pub(crate) struct ActivityFenceParts {
    read: OwnedRwLockReadGuard<()>,
    #[cfg(unix)]
    os: Option<OsSharedLock>,
}

/// Raw transition fence halves handed to the lease constructors.
pub(crate) struct TransitionFenceParts {
    write: OwnedRwLockWriteGuard<()>,
    #[cfg(unix)]
    os: Option<OsExclusiveLock>,
}

/// The retained `409 character_busy` refusal (verbatim daemon wording).
pub(crate) fn busy_error(character_id: &str) -> CoreError {
    CoreError::ActorConflict {
        code: "character_busy".to_string(),
        message: format!(
            "character {character_id} has an in-flight activity; cancel it or retry after it drains"
        ),
    }
}

/// Classify a non-blocking `flock` failure as a held lease (`EWOULDBLOCK`/
/// `EAGAIN` on Linux, `EACCES` on some BSD paths) — one platform-independent
/// predicate, mirroring `workspace::authority`.
#[cfg(unix)]
fn is_lock_busy(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::PermissionDenied
    )
}

impl ActorActivityLease {
    pub(crate) fn new(
        parts: ActivityFenceParts,
        owner_creator_id: String,
        character_id: String,
        epoch: i64,
    ) -> Self {
        Self {
            _read: parts.read,
            #[cfg(unix)]
            _os: parts.os,
            owner_creator_id,
            character_id,
            epoch,
        }
    }

    /// Trusted owner (active Creator) admitted with this activity.
    #[must_use]
    pub fn owner_creator_id(&self) -> &str {
        &self.owner_creator_id
    }

    /// Character this activity lease covers.
    #[must_use]
    pub fn character_id(&self) -> &str {
        &self.character_id
    }

    /// Stored lifecycle epoch re-read under the fence at admission.
    #[must_use]
    pub const fn epoch(&self) -> i64 {
        self.epoch
    }
}

impl std::fmt::Debug for ActorActivityLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorActivityLease")
            .field("owner_creator_id", &self.owner_creator_id)
            .field("character_id", &self.character_id)
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

impl CharacterTransitionLease {
    pub(crate) fn new(
        parts: TransitionFenceParts,
        owner_creator_id: String,
        character_id: String,
        epoch: i64,
    ) -> Self {
        Self {
            _write: parts.write,
            #[cfg(unix)]
            _os: parts.os,
            owner_creator_id,
            character_id,
            epoch: std::sync::atomic::AtomicI64::new(epoch),
        }
    }

    /// Trusted owner (active Creator) admitted for the transition.
    #[must_use]
    pub fn owner_creator_id(&self) -> &str {
        &self.owner_creator_id
    }

    /// Character this transition lease covers.
    #[must_use]
    pub fn character_id(&self) -> &str {
        &self.character_id
    }

    /// Stored lifecycle epoch re-read under the exclusive fence at
    /// transition start; after a committed
    /// [`crate::CoreService::commit_character_transition`] this is the
    /// committed epoch, so hosts can detect material transitions by
    /// comparing against the pre-commit value.
    #[must_use]
    pub fn epoch(&self) -> i64 {
        self.epoch
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn set_epoch(&self, epoch: i64) {
        self.epoch
            .store(epoch, std::sync::atomic::Ordering::SeqCst);
    }
}

impl std::fmt::Debug for CharacterTransitionLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CharacterTransitionLease")
            .field("owner_creator_id", &self.owner_creator_id)
            .field("character_id", &self.character_id)
            .field("epoch", &self.epoch())
            .finish_non_exhaustive()
    }
}

#[cfg(unix)]
mod os {
    use std::os::unix::io::AsRawFd;

    /// One descriptor per live lease: `flock` locks ride on the open file
    /// description, so two descriptors in the same process contend exactly
    /// like two processes.
    fn open_lock_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .read(true)
            .open(path)
    }

    fn unlock(file: &std::fs::File) {
        // nix 0.28 deprecates `flock()` in favor of the `Flock` struct, but
        // that struct requires ownership of the descriptor; keep the
        // deprecated call alive for the guard path (same rationale as
        // `nexus-local-db::file_lock`).
        #[allow(deprecated)]
        let _ = nix::fcntl::flock(file.as_raw_fd(), nix::fcntl::FlockArg::Unlock);
    }

    /// Shared (`LOCK_SH`) OS lock released on drop.
    pub(crate) struct OsSharedLock {
        file: std::fs::File,
    }

    impl OsSharedLock {
        /// Acquire blocking; runs on the blocking pool so a cross-process
        /// exclusive holder delays admission without stalling the runtime.
        pub(crate) async fn acquire(path: std::path::PathBuf) -> crate::error::CoreResult<Self> {
            tokio::task::spawn_blocking(move || {
                let file = open_lock_file(&path).map_err(|err| crate::error::CoreError::Internal {
                    category: format!("character_lock: {err}"),
                })?;
                #[allow(deprecated)]
                nix::fcntl::flock(file.as_raw_fd(), nix::fcntl::FlockArg::LockShared).map_err(
                    |err| {
                        crate::error::CoreError::Internal {
                            category: format!("character_lock: {err}"),
                        }
                    },
                )?;
                Ok(Self { file })
            })
            .await
            .map_err(|err| crate::error::CoreError::Internal {
                category: format!("character_lock: {err}"),
            })?
        }
    }

    impl Drop for OsSharedLock {
        fn drop(&mut self) {
            unlock(&self.file);
        }
    }

    /// Exclusive (`LOCK_EX | LOCK_NB`) OS lock released on drop.
    pub(crate) struct OsExclusiveLock {
        file: std::fs::File,
    }

    impl OsExclusiveLock {
        /// Acquire non-blocking; a held lease surfaces as a busy I/O error.
        pub(crate) fn try_acquire(path: std::path::PathBuf) -> std::io::Result<Self> {
            let file = open_lock_file(&path)?;
            #[allow(deprecated)]
            nix::fcntl::flock(file.as_raw_fd(), nix::fcntl::FlockArg::LockExclusiveNonblock)?;
            Ok(Self { file })
        }
    }

    impl Drop for OsExclusiveLock {
        fn drop(&mut self) {
            unlock(&self.file);
        }
    }
}

#[cfg(unix)]
use os::{OsExclusiveLock, OsSharedLock};
