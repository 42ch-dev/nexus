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
//!    shared/exclusive pair acquired with `std::fs::File::try_lock_shared` /
//!    `try_lock` on
//!    `<state-db dir>/character_locks/<character_id>.lock`, one descriptor
//!    per live lease. OS locks ride on the open file description, so
//!    descriptors in the same process contend with each other exactly as two
//!    processes would: two independently opened cores race correctly, and a
//!    direct CLI transition fences against a live service. `std` file locking
//!    is used (not a Unix-only `flock(2)` binding) because it is the same
//!    primitive `nexus-local-db::writer_protocol` already relies on for its
//!    writer protocol, and it keeps this contract enforced on every supported
//!    target rather than degrading to a process-local no-op off Unix. One
//!    caveat that follows: Windows byte-range locks are enforced against
//!    other handles, so this fence must never be applied to a file that is
//!    also opened for content — `<character_id>.lock` carries no payload.
//!
//! **Both layers are non-blocking** (arch §2 / durable §11.3: "shared effect
//! leases/exclusive transition leases are nonblocking — busy is observable").
//! Every acquisition uses the `try_*` form and maps a held lease to the
//! retained `character_busy` refusal, so a contended admission reports busy
//! promptly instead of parking behind the holder — a transition never waits
//! out an in-flight effect and an effect never waits out a transition.
//!
//! Stored `lifecycle_epoch` and ownership are re-read after acquiring the
//! fence (both layers) so the lease witnesses an exact pre-transition epoch
//! and `actor_session_stale` detection stays possible after a material
//! transition.
//!
//! # Typed knowledge fences (v1.191 P1 T5, durable §4.3)
//!
//! The same two layers carry a second typed family: World and Character
//! knowledge-effect leases. An ActorView/management knowledge operation holds
//! a **shared** lease on every World it selects and then on every selected
//! Character; a World-governance edit takes the World **exclusive** lease and
//! a Character-global/binding-governance edit takes the Character exclusive
//! lease, so a disclosure edit cannot land while a stream is still reading,
//! and a read cannot start while a governance edit holds the subject. Each
//! kind has its own lock namespace (`world_locks/`, `character_locks/`) so a
//! World and a Character that happen to share a name never contend.
//!
//! Multi-subject acquisition is canonical — World ids first, then Character
//! ids, lexicographically within each kind, duplicates collapsed — so two
//! operations selecting the same subjects can never deadlock by inverting the
//! order. Acquisition stays non-blocking per subject and a failure releases
//! every lease already taken in that attempt (returning the partial set is a
//! bug: the caller never sees a half-held plan).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use crate::error::{CoreError, CoreResult};

/// Lock family for one fenced subject kind (§4.3).
///
/// The kind selects the lock namespace and the busy refusal code; it is not an
/// authority carrier and grants nothing on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActorFenceKind {
    World,
    Character,
}

impl ActorFenceKind {
    /// Lock subdirectory under the state DB directory.
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::World => "world_locks",
            Self::Character => "character_locks",
        }
    }
}

/// Per-subject fence state owned by a [`crate::CoreService`].
pub struct ActorFenceTable {
    process: Mutex<HashMap<(ActorFenceKind, String), Arc<RwLock<()>>>>,
    locks_dir: PathBuf,
}

/// Shared activity lease (durable §11.3.1): hold through every DB/file/
/// provider/terminal-capture effect; drop releases both fence layers.
pub struct ActorActivityLease {
    _read: OwnedRwLockReadGuard<()>,
    _os: OsSharedLock,
    owner_creator_id: String,
    character_id: String,
    epoch: i64,
}

/// Shared knowledge-effect leases for one admitted read/effect plan (§4.3).
///
/// The plan is acquired World ids then Character ids; the guards are held
/// together and dropped together, so a caller cannot hold a Character lease
/// whose World lease already drained.
pub struct KnowledgeEffectLeases {
    _world_read: Vec<OwnedRwLockReadGuard<()>>,
    _world_os: Vec<OsSharedLock>,
    _character_read: Vec<OwnedRwLockReadGuard<()>>,
    _character_os: Vec<OsSharedLock>,
}

/// Exclusive governance lease for one World or Character subject (§4.3).
///
/// Busy refusal against every in-flight shared knowledge lease of that exact
/// subject: a disclosure edit is refused while a read/stream is still running,
/// and a read is refused while the edit holds the subject.
pub struct KnowledgeGovernanceLease {
    kind: ActorFenceKind,
    subject_id: String,
    _write: OwnedRwLockWriteGuard<()>,
    _os: OsExclusiveLock,
}


/// Exclusive transition lease (durable §11.3.2).
///
/// Busy refusal, epoch re-read under the fence, updated to the committed
/// epoch by [`crate::CoreService::commit_character_transition`] so the host
/// can retire old-epoch sessions while the fence is still held (§11.3.3).
pub struct CharacterTransitionLease {
    _write: OwnedRwLockWriteGuard<()>,
    _os: OsExclusiveLock,
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
                .unwrap_or_else(|| Path::new("."))
                .join("character_locks"),
        }
    }

    /// Stable per-subject OS lock file path (its own namespace per kind).
    fn lock_path(&self, kind: ActorFenceKind, subject_id: &str) -> PathBuf {
        self.locks_dir
            .join(kind.dir_name())
            .join(format!("{subject_id}.lock"))
    }

    /// Fetch or create the per-subject process fence, sweeping entries with
    /// no live guard (live-Arc reclaim, mirroring the daemon §11.3.6 sweep).
    fn fence_for(&self, kind: ActorFenceKind, subject_id: &str) -> Arc<RwLock<()>> {
        let mut fences = self
            .process
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        fences.retain(|_, fence| Arc::strong_count(fence) > 1);
        Arc::clone(
            fences
                .entry((kind, subject_id.to_string()))
                .or_insert_with(|| Arc::new(RwLock::new(()))),
        )
    }

    /// Try to take one shared fence: process read guard first, then the
    /// non-blocking OS shared lock.
    fn try_shared(
        &self,
        kind: ActorFenceKind,
        subject_id: &str,
    ) -> CoreResult<(OwnedRwLockReadGuard<()>, OsSharedLock)> {
        let read = Arc::clone(&self.fence_for(kind, subject_id))
            .try_read_owned()
            .map_err(|_| busy(kind, subject_id))?;
        let os = OsSharedLock::try_acquire(self.lock_path(kind, subject_id))
            .map_err(|err| lock_error(kind, subject_id, err))?;
        Ok((read, os))
    }

    /// Try to take one exclusive fence: process write guard, then the
    /// non-blocking OS exclusive lock.
    fn try_exclusive(
        &self,
        kind: ActorFenceKind,
        subject_id: &str,
    ) -> CoreResult<(OwnedRwLockWriteGuard<()>, OsExclusiveLock)> {
        let write = Arc::clone(&self.fence_for(kind, subject_id))
            .try_write_owned()
            .map_err(|_| busy(kind, subject_id))?;
        let os = OsExclusiveLock::try_acquire(self.lock_path(kind, subject_id))
            .map_err(|err| lock_error(kind, subject_id, err))?;
        Ok((write, os))
    }

    /// Try to take the shared activity fence for one Character: process read
    /// guard first, then the non-blocking OS shared lock.
    ///
    /// Non-blocking by contract (arch §2 / durable §11.3: "shared effect
    /// leases/exclusive transition leases are nonblocking — busy is
    /// observable"). An activity admission never waits for an in-flight
    /// transition to release: it reports the same `character_busy` refusal a
    /// transition reports for an in-flight activity, so admission and cancel
    /// stay responsive under contention. No await point — the whole
    /// acquisition is a pair of `try_*` calls.
    pub(crate) fn try_acquire_activity(
        &self,
        character_id: &str,
    ) -> CoreResult<ActivityFenceParts> {
        let (read, os) = self.try_shared(ActorFenceKind::Character, character_id)?;
        Ok(ActivityFenceParts { read, os })
    }

    /// Try to take the exclusive transition fence (busy refusal, never
    /// waiting): process write guard, then the non-blocking OS exclusive
    /// lock. Any outstanding activity lease refuses with `character_busy`.
    pub(crate) fn try_acquire_transition(
        &self,
        character_id: &str,
    ) -> CoreResult<TransitionFenceParts> {
        let (write, os) = self.try_exclusive(ActorFenceKind::Character, character_id)?;
        Ok(TransitionFenceParts { write, os })
    }

    /// Try to take the whole shared knowledge-effect plan (§4.3): World ids
    /// then Character ids, lexicographically within each kind, duplicates
    /// collapsed.
    ///
    /// Non-blocking per subject. A failure at any subject drops the partial
    /// plan before returning, so a caller that observes an error observes no
    /// held lease from this attempt.
    pub(crate) fn try_acquire_knowledge_effect(
        &self,
        world_ids: &[String],
        character_ids: &[String],
    ) -> CoreResult<KnowledgeEffectLeases> {
        let worlds = canonical_subjects(world_ids);
        let characters = canonical_subjects(character_ids);
        let mut plan = KnowledgeEffectLeases {
            _world_read: Vec::with_capacity(worlds.len()),
            _world_os: Vec::with_capacity(worlds.len()),
            _character_read: Vec::with_capacity(characters.len()),
            _character_os: Vec::with_capacity(characters.len()),
        };
        for world_id in worlds {
            let (read, os) = self.try_shared(ActorFenceKind::World, world_id)?;
            plan._world_read.push(read);
            plan._world_os.push(os);
        }
        for character_id in characters {
            let (read, os) = self.try_shared(ActorFenceKind::Character, character_id)?;
            plan._character_read.push(read);
            plan._character_os.push(os);
        }
        Ok(plan)
    }

    /// Try to take the exclusive governance fence for one World/Character
    /// subject (§4.3). Non-blocking: any in-flight shared knowledge lease of
    /// that subject is the busy refusal.
    pub(crate) fn try_acquire_knowledge_governance(
        &self,
        kind: ActorFenceKind,
        subject_id: &str,
    ) -> CoreResult<KnowledgeGovernanceLease> {
        let (write, os) = self.try_exclusive(kind, subject_id)?;
        Ok(KnowledgeGovernanceLease {
            kind,
            subject_id: subject_id.to_string(),
            _write: write,
            _os: os,
        })
    }
}

/// Canonical acquisition order inside one kind: lexical, duplicates collapsed.
fn canonical_subjects(ids: &[String]) -> Vec<&str> {
    let mut unique: Vec<&str> = ids.iter().map(String::as_str).collect();
    unique.sort_unstable();
    unique.dedup();
    unique
}

/// Translate a non-blocking lock outcome: a held lease is the retained busy
/// refusal for that kind; anything else is a real fault.
fn lock_error(
    kind: ActorFenceKind,
    subject_id: &str,
    err: std::fs::TryLockError,
) -> CoreError {
    match err {
        std::fs::TryLockError::WouldBlock => busy(kind, subject_id),
        std::fs::TryLockError::Error(io) => CoreError::Internal {
            category: format!("{}: {io}", kind.dir_name()),
        },
    }
}

/// The retained refusal for a contended subject: `character_busy` keeps its
/// verbatim daemon wording, World contention is its own typed code.
fn busy(kind: ActorFenceKind, subject_id: &str) -> CoreError {
    match kind {
        ActorFenceKind::Character => busy_error(subject_id),
        ActorFenceKind::World => CoreError::ActorConflict {
            code: "world_busy".to_string(),
            message: format!(
                "world {subject_id} has an in-flight knowledge activity; retry after it drains"
            ),
        },
    }
}

/// Raw fence halves handed to the lease constructors.
pub struct ActivityFenceParts {
    read: OwnedRwLockReadGuard<()>,
    os: OsSharedLock,
}

/// Raw transition fence halves handed to the lease constructors.
pub struct TransitionFenceParts {
    write: OwnedRwLockWriteGuard<()>,
    os: OsExclusiveLock,
}

/// The retained `409 character_busy` refusal (verbatim daemon wording).
pub fn busy_error(character_id: &str) -> CoreError {
    CoreError::ActorConflict {
        code: "character_busy".to_string(),
        message: format!(
            "character {character_id} has an in-flight activity; cancel it or retry after it drains"
        ),
    }
}

/// One descriptor per live lease: OS locks ride on the open file description,
/// so two descriptors in the same process contend exactly like two processes.
fn open_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
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

/// Shared (read) OS lock released on drop.
struct OsSharedLock {
    file: std::fs::File,
}

impl OsSharedLock {
    /// Acquire non-blocking; a held exclusive lease surfaces as
    /// [`TryLockError::WouldBlock`] and is translated to the retained busy
    /// refusal by the caller. Never waits on the blocking pool: an admission
    /// blocked behind an in-flight transition would stall the caller instead
    /// of reporting busy, which the lease contract forbids.
    #[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
    fn try_acquire(path: PathBuf) -> Result<Self, std::fs::TryLockError> {
        let file = open_lock_file(&path).map_err(std::fs::TryLockError::Error)?;
        file.try_lock_shared()?;
        Ok(Self { file })
    }
}

impl Drop for OsSharedLock {
    fn drop(&mut self) {
        // Released with the descriptor; an explicit unlock keeps the release
        // independent of when the file handle is finally closed.
        let _ = self.file.unlock();
    }
}

/// Exclusive (write) OS lock released on drop.
struct OsExclusiveLock {
    file: std::fs::File,
}

impl OsExclusiveLock {
    /// Acquire non-blocking; a held lease surfaces as [`TryLockError::WouldBlock`]
    /// and is translated to the retained busy refusal by the caller.
    #[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
    fn try_acquire(path: PathBuf) -> Result<Self, std::fs::TryLockError> {
        let file = open_lock_file(&path).map_err(std::fs::TryLockError::Error)?;
        file.try_lock()?;
        Ok(Self { file })
    }
}

impl Drop for OsExclusiveLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
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
        self.epoch.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn set_epoch(&self, epoch: i64) {
        self.epoch.store(epoch, std::sync::atomic::Ordering::SeqCst);
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

impl KnowledgeEffectLeases {
    /// Number of held World shared leases (tests / diagnostics).
    #[must_use]
    pub fn world_lease_count(&self) -> usize {
        self._world_read.len()
    }

    /// Number of held Character shared leases (tests / diagnostics).
    #[must_use]
    pub fn character_lease_count(&self) -> usize {
        self._character_read.len()
    }
}

impl std::fmt::Debug for KnowledgeEffectLeases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Non-exhaustive: the held subject ids are not retained here, and the
        // guards must not be introspected.
        f.debug_struct("KnowledgeEffectLeases")
            .field("world_leases", &self.world_lease_count())
            .field("character_leases", &self.character_lease_count())
            .finish_non_exhaustive()
    }
}

impl KnowledgeGovernanceLease {
    /// Fenced subject kind.
    #[must_use]
    pub const fn kind(&self) -> ActorFenceKind {
        self.kind
    }

    /// Fenced World/Character id.
    #[must_use]
    pub fn subject_id(&self) -> &str {
        &self.subject_id
    }
}

impl std::fmt::Debug for KnowledgeGovernanceLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnowledgeGovernanceLease")
            .field("kind", &self.kind)
            .field("subject_id", &self.subject_id)
            .finish_non_exhaustive()
    }
}
