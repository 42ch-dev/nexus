//! Cross-process workspace writer protocol: OS locks, epochs, registration, guarded pools.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::task::{Context, Waker};
use std::time::{Duration, Instant};

use nexus_storage_guard::{
    install_writer_functions, WriterAdmission, WriterConnectionContext, WriterMode as GuardMode,
};
use sqlx::pool::CloseEvent;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::LocalDbError;

const LOCK_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
const SUPPORTED_PROTOCOL_VERSION: i64 = 1;
pub const BOOTSTRAP_CREATOR_ID: &str = "bootstrap";

/// One cooperative pool this process opened over a database.
///
/// Tracked per POOL, not per handle: clones share one pool and one OS
/// admission until the last of them is closed. The registry holds no strong
/// pool handle — only the pool's close event — so a pool whose last handle went
/// away is not kept alive by this bookkeeping.
struct RegisteredPool {
    id: u64,
    /// Resolves when the pool closes; `None` once it has.
    close_event: Option<CloseEvent>,
}

/// One database's live admission: the guard that holds its OS locks and the
/// number of holders claiming it.
/// How the registry relates to one admission's guard.
enum AdmissionOwner {
    /// A live handle owns the admission; the registry only observes it. The OS
    /// locks are shared by the guard and every real connection, so nothing of
    /// ours keeps them past the last real user.
    Observed(Weak<WorkspaceWriterGuard>),
    /// The caller deliberately retained this admission for the process
    /// (`retain_writer_guard`, the raw-pool entry point). The registry really
    /// owns it until an explicit path-wide release or a guarded migration
    /// clears it — that is the documented raw-pool policy, not a leak.
    Retained(Arc<WorkspaceWriterGuard>),
}

impl AdmissionOwner {
    fn upgrade(&self) -> Option<Arc<WorkspaceWriterGuard>> {
        match self {
            Self::Observed(weak) => weak.upgrade(),
            Self::Retained(guard) => Some(Arc::clone(guard)),
        }
    }
}

struct RetainedGuardEntry {
    owner: AdmissionOwner,
    /// Identity of the published admission, kept here so a claim can be matched
    /// even after the guard itself is gone.
    writer_id: String,
    /// Live holders of this admission: the open that published it and every
    /// open that joined it.
    claims: usize,
}

static POOL_REG_ID: AtomicU64 = AtomicU64::new(1);

/// This process's live writer admissions, keyed by
/// `(canonical db path, mode)`.
///
/// Keyed per mode so releasing a direct claim never drops the engine owner's
/// admission, and identity-checked by `writer_id` so a stale holder can never
/// release an entry a later acquisition replaced.
static ACTIVE_GUARDS: LazyLock<Mutex<HashMap<(PathBuf, WriterMode), RetainedGuardEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// This process's cooperative pools, keyed by `(canonical db path, mode)`.
///
/// Deliberately separate from [`ACTIVE_GUARDS`]: a cloned pool handle
/// ([`GuardedPool::clone_pool`]) can outlive the handle that opened it, so
/// migration quiescence and admission lifetime follow the pools themselves,
/// not the handles. No strong pool handle is kept here.
static ACTIVE_POOLS: LazyLock<Mutex<HashMap<(PathBuf, WriterMode), Vec<RegisteredPool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterMode {
    Direct,
    Engine,
    Migration,
}

impl WriterMode {
    const fn to_guard(self) -> GuardMode {
        match self {
            Self::Direct => GuardMode::Direct,
            Self::Engine => GuardMode::Engine,
            Self::Migration => GuardMode::Migration,
        }
    }
}

/// Opaque workspace writer guard.
///
/// The `File` fields are RAII: they are never read, but dropping the guard
/// releases the OS admission locks (`state.db.migration.lock` shared /
/// `state.db.engine.lock` exclusive). The random `writer_id` token is
/// intentionally absent from `Debug`.
#[allow(dead_code)]
pub struct WorkspaceWriterGuard {
    pub(crate) db_path: PathBuf,
    writer_id: String,
    mode: WriterMode,
    migration_epoch: i64,
    engine_epoch: Option<i64>,
    /// The complete OS admission this guard acquired.
    ///
    /// Shared with every connection opened under this guard, so the OS locks
    /// outlive the guard itself and are released only when the last real
    /// guarded connection is disposed by SQLite.
    admission: Arc<WriterAdmission>,
}

impl std::fmt::Debug for WorkspaceWriterGuard {
    /// Deliberately omits `writer_id` (a fresh per-process token, not for logs).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceWriterGuard")
            .field("db_path", &self.db_path)
            .field("mode", &self.mode)
            .field("migration_epoch", &self.migration_epoch)
            .field("engine_epoch", &self.engine_epoch)
            .finish_non_exhaustive()
    }
}

/// Tunables for a guarded pool.
///
/// Defaults match the pre-protocol factory (8 connections, sqlx default
/// acquire timeout); callers that documented their own sizing (the daemon's
/// `PoolConfig`) keep it by passing this through.
#[derive(Debug, Clone, Copy)]
pub struct GuardedPoolOptions {
    /// Upper bound on pooled connections.
    pub max_connections: u32,
    /// How long a caller waits for a free pooled connection.
    pub acquire_timeout: Option<Duration>,
}

impl Default for GuardedPoolOptions {
    fn default() -> Self {
        Self {
            max_connections: 8,
            acquire_timeout: Some(Duration::from_secs(5)),
        }
    }
}

/// One holder's live claim on the retained admission entry for its
/// `(path, mode)`.
///
/// A claim is reserved AT THE SAME TIME as the retained guard is obtained or
/// published, and is held across awaited pool construction and seeding, so a
/// joiner in flight is protected before its pool exists and an initialization
/// error or cancellation gives the claim back. The creator and every joining
/// holder register their own claim, so one holder's failure releases only its
/// own use; the LAST claim removes the entry, and with it the OS admission the
/// entry's guard carries.
struct RetainedAdmissionClaim {
    key: (PathBuf, WriterMode),
    writer_id: String,
    held: AtomicBool,
}

impl RetainedAdmissionClaim {
    /// Wrap an already-registered claim on `guard`'s admission.
    fn held(key: (PathBuf, WriterMode), writer_id: String) -> Self {
        Self {
            key,
            writer_id,
            held: AtomicBool::new(true),
        }
    }

    /// Give up THIS claim exactly once; the last claim removes the entry.
    fn release(&self) {
        if !self.held.swap(false, Ordering::AcqRel) {
            return;
        }
        let mut guards = ACTIVE_GUARDS
            .lock()
            .expect("writer guard registry poisoned");
        let Some(entry) = guards.get_mut(&self.key) else {
            return;
        };
        // A later acquisition may have replaced the entry: a claim names the
        // admission it was registered on, never whichever one is there now.
        if entry.writer_id != self.writer_id {
            return;
        }
        entry.claims = entry.claims.saturating_sub(1);
        // The admission outlives the last claim while a pool opened under it
        // can still be used: a cloned handle is a real cross-process writer.
        let released = entry.claims == 0 && admission_pools_quiesced(&self.key);
        if released {
            guards.remove(&self.key);
        }
    }

    /// Park this claim for the rest of the process.
    ///
    /// The raw-pool entry point (`retain_writer_guard`) hands its caller no
    /// lifetime handle, so its admission is deliberately retained until an
    /// explicit path-wide release or a guarded migration clears it.
    fn persist(self) {
        self.held.store(false, Ordering::Release);
    }
}

impl Drop for RetainedAdmissionClaim {
    /// Every final lifetime end gives the claim back — a dropped
    /// [`GuardedPool`], an initialization error, a cancelled construction.
    fn drop(&mut self) {
        self.release();
    }
}

pub struct GuardedPool {
    guard: Arc<WorkspaceWriterGuard>,
    /// Present until the handle is dropped: dropping gives it up first, so the
    /// pool can report itself closed before the claim settles.
    pool: Option<SqlitePool>,
    /// This pool's cooperative registration; clones share it.
    registration: u64,
    /// This handle's live claim on its retained admission entry.
    ///
    /// Declared last so the pool and its registration are dropped before the
    /// claim can remove the entry that keeps the OS admission alive; dropping
    /// the handle releases it, whichever holder's lifetime ends.
    claim: RetainedAdmissionClaim,
}

impl GuardedPool {
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        self.pool
            .as_ref()
            .expect("the pool is present until the handle is dropped")
    }

    #[must_use]
    pub fn guard(&self) -> &WorkspaceWriterGuard {
        &self.guard
    }

    /// Clone the underlying pool.
    ///
    /// The clone shares this handle's cooperative registration and admission:
    /// closing any one of them closes them all, so the OS admission stays held
    /// until the last clone is closed or dropped.
    #[must_use]
    pub fn clone_pool(&self) -> SqlitePool {
        self.pool().clone()
    }

    fn pool_key(&self) -> (PathBuf, WriterMode) {
        (canonical_db_path(&self.guard.db_path), self.guard.mode)
    }

    /// Withdraw this pool's cooperative registration once it can no longer be
    /// used, so the admission settles without waiting for a watcher.
    fn settle_closed_pool(&self) {
        if self.pool().is_closed() {
            unregister_pool_handle(&self.pool_key(), self.registration);
        }
    }

    /// Withdraw this handle's registration if the pool it just gave up has
    /// closed — i.e. no clone of it is left usable.
    fn settle_dropped_pool(&self) {
        let mut pools = ACTIVE_POOLS.lock().expect("pool registry poisoned");
        let Some(registered) = pools.get_mut(&self.pool_key()) else {
            return;
        };
        registered.retain_mut(|reg| reg.id != self.registration || registration_open(reg));
    }

    /// Give up THIS handle's live claim on its retained admission entry.
    ///
    /// The handle's own pool must be closed first: the registry entry keeps the
    /// guard (and therefore the OS admission) alive, and the entry is removed
    /// only when the LAST claim is released. Another open's admission — the
    /// creator's or a joiner's, in this mode or any other — is never evicted,
    /// and a second call is a no-op.
    pub fn release_retained_admission(&self) {
        self.settle_closed_pool();
        self.claim.release();
    }
}

fn canonical_db_path(db_path: &Path) -> PathBuf {
    if db_path.exists() {
        db_path
            .canonicalize()
            .unwrap_or_else(|_| db_path.to_path_buf())
    } else {
        let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = db_path
            .file_name()
            .map_or_else(|| db_path.as_os_str().into(), PathBuf::from);
        let canonical_parent = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());
        canonical_parent.join(file_name)
    }
}

/// Publish `guard` as this process's live admission for its `(path, mode)`
/// and reserve one claim on it, atomically under the registry lock.
///
/// Returns the admission the claim belongs to TOGETHER with that claim, both
/// taken in the same critical section: the caller must keep that `Arc` and hand
/// it to the pool, its owner slot or the RAW retention, so the admission it
/// reserved cannot end before it is installed. A weak lookup afterwards would
/// let a supplied guard whose identity lost the publication race become the
/// pool's identity, which is exactly the mismatch this shape prevents.
///
/// Two acquisitions can both miss the lookup and take their own OS locks
/// before either publishes. The loser of that race JOINS the published
/// admission instead of replacing its identity: a claim is never carried
/// across two `writer_id`s, so the published guard's last release still frees
/// the OS admission.
fn reserve_retained_admission(
    guard: &Arc<WorkspaceWriterGuard>,
) -> (Arc<WorkspaceWriterGuard>, RetainedAdmissionClaim) {
    let key = (canonical_db_path(&guard.db_path), guard.mode);
    let mut guards = ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned");
    let (published, writer_id) = match guards.get_mut(&key) {
        // A live published admission is joined, never replaced (F7/F11). The
        // strong `Arc` is held from here on, so it cannot end under the claim.
        Some(entry) => match entry.owner.upgrade() {
            Some(published) => {
                entry.claims += 1;
                (published, entry.writer_id.clone())
            }
            // A dead entry names no admission any more: publish this guard.
            None => {
                *entry = RetainedGuardEntry {
                    owner: AdmissionOwner::Observed(Arc::downgrade(guard)),
                    writer_id: guard.writer_id.clone(),
                    claims: 1,
                };
                (Arc::clone(guard), guard.writer_id.clone())
            }
        },
        None => {
            guards.insert(
                key.clone(),
                RetainedGuardEntry {
                    owner: AdmissionOwner::Observed(Arc::downgrade(guard)),
                    writer_id: guard.writer_id.clone(),
                    claims: 1,
                },
            );
            (Arc::clone(guard), guard.writer_id.clone())
        }
    };
    drop(guards);
    (published, RetainedAdmissionClaim::held(key, writer_id))
}

/// Claim this process's live admission for `(db_path, mode)` atomically with
/// the lookup, so the admission cannot be released between joining it and
/// owning a claim on it.
fn claim_live_admission(
    db_path: &Path,
    mode: WriterMode,
) -> Option<(Arc<WorkspaceWriterGuard>, RetainedAdmissionClaim)> {
    let key = (canonical_db_path(db_path), mode);
    let mut guards = ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned");
    let Some(published) = guards.get(&key).and_then(|entry| entry.owner.upgrade()) else {
        // The last real holder is gone: this entry names no live admission.
        guards.remove(&key);
        return None;
    };
    let entry = guards.get_mut(&key)?;
    entry.claims += 1;
    Some((
        published,
        RetainedAdmissionClaim::held(key, entry.writer_id.clone()),
    ))
}

/// Retain a live writer guard in this process's registry so published pools
/// keep their admission after the caller drops its own `Arc`.
///
/// The retention itself is the claim here: callers of this raw-pool entry
/// point never take a [`GuardedPool`] handle back, so nothing ever releases it.
///
/// # Panics
///
/// Panics if the writer-guard registry mutex is poisoned (a prior
/// registration panicked while holding the lock).
pub fn retain_writer_guard(guard: Arc<WorkspaceWriterGuard>) {
    let (published, claim) = reserve_retained_admission(&guard);
    // The raw-pool entry point hands back no handle, so the registry is the
    // owner: the admission survives until an explicit path-wide release or a
    // guarded migration, exactly as documented.
    //
    // Exactly the admission this claim reserved is retained — the strong guard
    // from the same critical section, never a later weak lookup and never the
    // supplied guard, which may have lost the publication race.
    if install_retained_admission(&published, &claim) {
        claim.persist();
    }
    // Otherwise a concurrent explicit release already replaced this entry: the
    // newer identity stays authoritative, and dropping `guard`/`claim` here
    // releases the superseded admission instead of installing a second one.
}

/// Install the reserved admission as the process-retained owner of its entry.
///
/// Runs on the strong guard the reservation returned, so no weak upgrade can
/// substitute a different identity. Returns `false` only when a concurrent
/// explicit release already republished a different identity for this
/// `(path, mode)`; the caller must then keep its hands off that entry.
fn install_retained_admission(
    published: &Arc<WorkspaceWriterGuard>,
    claim: &RetainedAdmissionClaim,
) -> bool {
    let mut guards = ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned");
    match guards.get_mut(&claim.key) {
        Some(entry) if entry.writer_id == published.writer_id => {
            entry.owner = AdmissionOwner::Retained(Arc::clone(published));
            true
        }
        Some(_) => false,
        None => {
            guards.insert(
                claim.key.clone(),
                RetainedGuardEntry {
                    owner: AdmissionOwner::Retained(Arc::clone(published)),
                    writer_id: published.writer_id.clone(),
                    claims: 1,
                },
            );
            true
        }
    }
}

fn register_pool_handle(guard: &Arc<WorkspaceWriterGuard>, pool: &SqlitePool) -> u64 {
    let id = POOL_REG_ID.fetch_add(1, Ordering::Relaxed);
    let key = (canonical_db_path(&guard.db_path), guard.mode);
    ACTIVE_POOLS
        .lock()
        .expect("pool registry poisoned")
        .entry(key.clone())
        .or_default()
        .push(RegisteredPool {
            id,
            close_event: Some(pool.close_event()),
        });

    // Every pool is opened from async code, so a driver is present. The
    // watcher settles an admission whose last claim is already gone when this
    // pool finally closes; it holds only the close event, never the pool.
    if let Ok(driver) = tokio::runtime::Handle::try_current() {
        let closed = pool.close_event();
        driver.spawn(async move {
            closed.await;
            settle_released_admission(&key);
        });
    }
    id
}

fn unregister_pool_handle(key: &(PathBuf, WriterMode), id: u64) {
    let mut pools = ACTIVE_POOLS.lock().expect("pool registry poisoned");
    if let Some(registered) = pools.get_mut(key) {
        registered.retain(|reg| reg.id != id);
        if registered.is_empty() {
            pools.remove(key);
        }
    }
}

/// Whether a registration's pool can still be used.
///
/// The close event is polled in place, so no strong pool handle is needed: an
/// explicitly closed pool has fired it, and a pool whose last handle went away
/// fires it as it is dropped.
fn registration_open(registered: &mut RegisteredPool) -> bool {
    let Some(event) = registered.close_event.as_mut() else {
        return false;
    };
    let mut cx = Context::from_waker(Waker::noop());
    if Pin::new(event).poll(&mut cx).is_ready() {
        registered.close_event = None;
        return false;
    }
    true
}

/// Whether the admission `key` has no still-open cooperative pool.
///
/// Called while the admission registry lock is held, so the two registries are
/// always taken in the same order.
fn admission_pools_quiesced(key: &(PathBuf, WriterMode)) -> bool {
    let mut pools = ACTIVE_POOLS.lock().expect("pool registry poisoned");
    let Some(registered) = pools.get_mut(key) else {
        return true;
    };
    registered.retain_mut(registration_open);
    let quiesced = registered.is_empty();
    if quiesced {
        pools.remove(key);
    }
    quiesced
}

/// Whether every cooperative pool this process opened on `db_path` — in any
/// mode — can no longer be used.
fn cooperative_pools_quiesced(db_path: &Path) -> bool {
    let target = canonical_db_path(db_path);
    let mut pools = ACTIVE_POOLS.lock().expect("pool registry poisoned");
    pools.retain(|(path, _), registered| {
        if path != &target {
            return true;
        }
        registered.retain_mut(registration_open);
        !registered.is_empty()
    });
    pools.keys().all(|(path, _)| path != &target)
}

/// Release an admission whose last claim and last usable pool are both gone,
/// dropping the guard and with it the OS locks.
///
/// Callers hold the admission registry lock, so the two registries are always
/// taken in the same order.
fn settle_released_admission(key: &(PathBuf, WriterMode)) {
    let mut guards = ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned");
    let Some(entry) = guards.get(key) else {
        return;
    };
    if entry.claims == 0 && admission_pools_quiesced(key) {
        guards.remove(key);
    }
}

/// This process's live guard for `(db_path, mode)`, if any.
///
/// A registry inspection seam for the in-crate tests; production callers never
/// need a weak re-lookup — they carry the strong guard a reservation returned.
#[cfg(test)]
fn retained_guard(db_path: &Path, mode: WriterMode) -> Option<Arc<WorkspaceWriterGuard>> {
    let key = (canonical_db_path(db_path), mode);
    ACTIVE_GUARDS
        .lock()
        .ok()?
        .get(&key)
        .and_then(|entry| entry.owner.upgrade())
}

/// Drop one retained guard, releasing its OS admission locks.
fn release_guard(db_path: &Path, mode: WriterMode) {
    let key = (canonical_db_path(db_path), mode);
    ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned")
        .remove(&key);
}

/// Drop retained guards after every cooperative pool has closed.
fn release_retained_guards(db_path: &Path) {
    let target = canonical_db_path(db_path);
    let mut guards = ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned");
    guards.retain(|(path, _), _| path != &target);
}

/// Release every retained writer guard for `db_path` in this process.
///
/// Cooperative pools on that path must be closed first; otherwise the OS
/// locks remain held until those pools are dropped.
pub fn release_retained_writer_guards(db_path: &Path) {
    release_retained_guards(db_path);
}

async fn await_cooperative_quiescence(db_path: &Path) -> Result<(), LocalDbError> {
    let target = canonical_db_path(db_path);
    let start = Instant::now();
    while start.elapsed() < LOCK_ACQUIRE_TIMEOUT {
        if cooperative_pools_quiesced(&target) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(LocalDbError::OwnerBusy {
        resource: format!("{}: cooperative pools still active", target.display()),
    })
}

fn migration_lock_path(db_path: &Path) -> PathBuf {
    let canonical = canonical_db_path(db_path);
    PathBuf::from(format!("{}.migration.lock", canonical.display()))
}

fn engine_lock_path(db_path: &Path) -> PathBuf {
    let canonical = canonical_db_path(db_path);
    PathBuf::from(format!("{}.engine.lock", canonical.display()))
}

fn workspace_identity(db_path: &Path) -> String {
    let canonical = db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf());
    canonical.display().to_string()
}

fn open_lock_file(path: &Path) -> Result<File, LocalDbError> {
    // `truncate(false)` is load-bearing: admission lock files are shared
    // across processes for the workspace's lifetime and must never be cut.
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| LocalDbError::IoWithPath {
            path: path.display().to_string(),
            source,
        })
}

fn acquire_shared_lock(path: &Path) -> Result<File, LocalDbError> {
    let file = open_lock_file(path)?;
    let start = Instant::now();
    while file.try_lock_shared().is_err() {
        if start.elapsed() >= LOCK_ACQUIRE_TIMEOUT {
            return Err(LocalDbError::OwnerBusy {
                resource: path.display().to_string(),
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(file)
}

fn acquire_exclusive_lock(path: &Path) -> Result<File, LocalDbError> {
    let file = open_lock_file(path)?;
    let start = Instant::now();
    while file.try_lock().is_err() {
        if start.elapsed() >= LOCK_ACQUIRE_TIMEOUT {
            return Err(LocalDbError::OwnerBusy {
                resource: path.display().to_string(),
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(file)
}

/// Fail-fast exclusive acquisition for the engine owner.
///
/// Engine ownership is an OS lock, not a stealable lease (§4.1): a live owner
/// is never replaced by a contender that merely out-waits it. A second
/// acquirer is refused immediately with [`LocalDbError::OwnerBusy`]; the OS
/// releases the lock on process death, and only then does the next owner
/// acquire (incrementing the durable engine epoch).
fn try_exclusive_lock(path: &Path) -> Result<File, LocalDbError> {
    let file = open_lock_file(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(_) => Err(LocalDbError::OwnerBusy {
            resource: path.display().to_string(),
        }),
    }
}

/// Exclusive admission fence held while one store's files are deleted.
///
/// The `File` is RAII and never read: dropping the fence releases the OS
/// migration lock. The lock file itself is deliberately *not* deleted — it is
/// the store's stable admission fence for the workspace's lifetime.
#[allow(dead_code)]
pub struct StoreResetFence {
    migration_lock: File,
}

/// Take the exclusive migration fence for `db_path`, refusing `db_path`'s store
/// immediately when any writer or migration is live on it.
///
/// This is the same lock file every writer must hold — `Direct` / `Engine` take
/// it shared and `Migration` exclusive (see [`acquire_writer_guard`]) — so an
/// exclusive acquisition is proof that no writer is live on the store. It
/// deliberately does not go through [`acquire_writer_guard`]: that path
/// registers a writer row inside the database being reset and requires the
/// protocol tables, while a reset must also be able to clear a pre-protocol or
/// corrupt store. Acquisition is fail-fast, like the engine owner's (§4.1): a
/// live writer is refused rather than out-waited, and the caller deletes no
/// bytes.
///
/// # Errors
///
/// Returns [`LocalDbError::OwnerBusy`] when the fence is held by a live writer
/// or migration, and [`LocalDbError::IoWithPath`] when the lock file cannot be
/// opened.
pub fn acquire_store_reset_fence(db_path: &Path) -> Result<StoreResetFence, LocalDbError> {
    Ok(StoreResetFence {
        migration_lock: try_exclusive_lock(&migration_lock_path(db_path))?,
    })
}

fn sqlite_url(db_path: &Path, read_only: bool) -> String {
    if read_only {
        format!("sqlite://{}?mode=ro", db_path.display())
    } else {
        format!("sqlite://{}?mode=rwc", db_path.display())
    }
}

fn writer_context(
    writer_id: &str,
    mode: WriterMode,
    migration_epoch: i64,
    engine_epoch: Option<i64>,
) -> WriterConnectionContext {
    WriterConnectionContext {
        writer_id: writer_id.to_string(),
        protocol_version: 1,
        mode: mode.to_guard(),
        migration_epoch,
        engine_epoch,
    }
}

/// The admission one new connection must pin.
///
/// Three outcomes, deliberately distinct:
/// * no slot at all is an intentional internal registration pool (no admission),
/// * a live slot yields the pool's admission,
/// * a slot that already retired is a hard error — a guarded pool must never
///   hand out a connection without the admission it was opened under.
fn pinned_admission(
    slot: Option<&Arc<PoolAdmissionSlot>>,
) -> Result<Option<Arc<WriterAdmission>>, sqlx::Error> {
    match slot.map(|slot| slot.admission()) {
        None => Ok(None),
        Some(Some(pinned)) => Ok(Some(pinned)),
        Some(None) => Err(sqlx::Error::Protocol(
            "guarded pool admission was retired while the pool was still open".to_string(),
        )),
    }
}

/// The published admission one cooperative pool keeps for its whole OPEN
/// lifetime.
///
/// The pool owns this slot — the `after_connect` factory lives in the pool's
/// own options — so the slot's lifetime is exactly the pool's, independent of
/// how many connections SQLite currently has. The close-event waker holds only
/// a `Weak` of the slot, so the listener can never keep the slot, or the guard
/// it owns, alive: the observer graph has no cycle.
struct PoolAdmissionSlot {
    /// Taken exactly once: on close-start, or when the slot itself drops.
    guard: Mutex<Option<Arc<WorkspaceWriterGuard>>>,
    /// The pool's close listener, kept registered for the slot's whole life.
    closed: Mutex<Option<Pin<Box<CloseEvent>>>>,
}

impl PoolAdmissionSlot {
    /// Own `guard` until this slot retires.
    fn new(guard: Arc<WorkspaceWriterGuard>) -> Arc<Self> {
        Arc::new(Self {
            guard: Mutex::new(Some(guard)),
            closed: Mutex::new(None),
        })
    }

    /// The admission a connection opened under this pool must pin.
    ///
    /// `None` only after the slot retired — a guarded pool treats that as a
    /// hard error rather than serving a connection without its admission.
    fn admission(&self) -> Option<Arc<WriterAdmission>> {
        self.guard
            .lock()
            .expect("pool admission slot poisoned")
            .as_ref()
            .map(|guard| Arc::clone(&guard.admission))
    }

    /// Give the admission back, exactly once, and drop it outside the lock.
    ///
    /// Runs inside `Pool::close()` (from the registered waker) or from the
    /// slot's own drop; it never awaits and never panics.
    fn retire(&self) {
        let retired = self
            .guard
            .lock()
            .expect("pool admission slot poisoned")
            .take();
        drop(retired);
    }

    /// Register the pool's close event against this slot.
    ///
    /// The listener is polled once with OUR waker, so SQLx's close-start
    /// notification calls it synchronously, with no task and no runtime. The
    /// polled listener is kept in the slot so nothing overwrites that waker.
    fn watch(self: &Arc<Self>, pool: &SqlitePool) {
        let waker = Waker::from(Arc::new(PoolCloseWaker(Arc::downgrade(self))));
        let mut context = Context::from_waker(&waker);
        let mut event: Pin<Box<CloseEvent>> = Box::pin(pool.close_event());
        if event.as_mut().poll(&mut context).is_ready() {
            // The pool was already closed when this slot was attached.
            self.retire();
        }
        *self.closed.lock().expect("pool admission slot poisoned") = Some(event);
    }
}

/// The close listener's waker: a weak view of the slot, so the listener never
/// owns what it notifies.
struct PoolCloseWaker(Weak<PoolAdmissionSlot>);

impl std::task::Wake for PoolCloseWaker {
    fn wake(self: Arc<Self>) {
        if let Some(slot) = self.0.upgrade() {
            slot.retire();
        }
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(slot) = self.0.upgrade() {
            slot.retire();
        }
    }
}

async fn pool_with_context(
    db_path: &Path,
    context: WriterConnectionContext,
    owner: Option<Arc<WorkspaceWriterGuard>>,
    options: GuardedPoolOptions,
) -> Result<SqlitePool, LocalDbError> {
    let db_path = db_path.to_path_buf();
    // Two independent owners of the same admission:
    //
    // * each connection pins it for as long as SQLite keeps that connection's
    //   userdata (until real disposal), and
    // * the pool itself owns the published guard through `PoolAdmissionSlot`,
    //   which the `after_connect` factory keeps alive for the whole pool, so an
    //   OPEN pool with zero current connections still holds both OS locks.
    //
    // An intentional internal `None` (protocol bookkeeping pools) installs no
    // admission; a guarded pool that requires one fails closed instead of
    // silently installing nothing.
    let slot = owner.map(PoolAdmissionSlot::new);
    let slot_for_factory = slot.clone();
    let context_for_hook = context;
    let url = sqlite_url(&db_path, false);
    let mut builder = SqlitePoolOptions::new().max_connections(options.max_connections);
    if let Some(timeout) = options.acquire_timeout {
        builder = builder.acquire_timeout(timeout);
    }
    let pool = builder
        .after_connect(move |conn, _meta| {
            // Holding this slot is what ties the admission to the POOL's
            // lifetime rather than to any single connection.
            let slot = slot_for_factory.clone();
            let context = context_for_hook.clone();
            Box::pin(async move {
                let pinned = pinned_admission(slot.as_ref())?;
                install_writer_functions(conn, context, pinned).await?;
                sqlx::query("PRAGMA journal_mode = WAL")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA busy_timeout = 2000")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .map_err(LocalDbError::from)?;
    if let Some(slot) = slot.as_ref() {
        // Close-start, not last-drop: a closed handle that is still referenced
        // must not keep the admission.
        slot.watch(&pool);
    }
    Ok(pool)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateState {
    protocol_version: i64,
    migration_epoch: i64,
    engine_epoch: i64,
}

fn validate_protocol_version(protocol_version: i64) -> Result<(), LocalDbError> {
    if protocol_version > SUPPORTED_PROTOCOL_VERSION {
        return Err(LocalDbError::SchemaMismatch {
            reason: format!("workspace protocol_version={protocol_version} is newer than this binary supports ({SUPPORTED_PROTOCOL_VERSION})"),
        });
    }
    Ok(())
}

async fn read_gate_state(db_path: &Path) -> Result<GateState, LocalDbError> {
    if !db_path.exists() {
        return Ok(GateState {
            protocol_version: 0,
            migration_epoch: 0,
            engine_epoch: 0,
        });
    }
    let url = sqlite_url(db_path, true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(LocalDbError::from)?;
    let present: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='core_workspace_gate'",
    )
    .fetch_optional(&pool)
    .await?;
    if present.is_none() {
        pool.close().await;
        return Ok(GateState {
            protocol_version: 0,
            migration_epoch: 0,
            engine_epoch: 0,
        });
    }
    let row: Option<(i64, i64, i64)> = sqlx::query_as("SELECT protocol_version, migration_epoch, engine_epoch FROM core_workspace_gate WHERE pk = 1").fetch_optional(&pool).await?;
    pool.close().await;
    Ok(row.map_or(
        GateState {
            protocol_version: 0,
            migration_epoch: 0,
            engine_epoch: 0,
        },
        |(protocol_version, migration_epoch, engine_epoch)| GateState {
            protocol_version,
            migration_epoch,
            engine_epoch,
        },
    ))
}

async fn needs_activation_recovery(db_path: &Path) -> Result<bool, LocalDbError> {
    if !protocol_tables_present(db_path).await {
        return Ok(false);
    }
    Ok(read_gate_state(db_path).await?.migration_epoch == 0)
}

/// True only when the workspace is fully provisioned *and* its schema already
/// matches this binary — i.e. a guarded migration run would be a no-op.
///
/// Fail-closed by construction: any unexpected, dirty, unknown, or missing
/// state returns `false`, which routes the caller through the guarded path
/// (quiescence + exclusive lock) where the strict checks live.
async fn migrations_are_current(db_path: &Path, gate: &GateState) -> Result<bool, LocalDbError> {
    // 1. Protocol initialized and at exactly the version this binary ships.
    //    `validate_protocol_version` already rejected *newer*; equality (not
    //    `<=`) also rejects a pre-activation workspace (`protocol_version = 0`).
    if gate.protocol_version != SUPPORTED_PROTOCOL_VERSION || gate.migration_epoch < 1 {
        return Ok(false);
    }
    if !protocol_tables_present(db_path).await {
        return Ok(false);
    }
    // 2. Schema matches the embedded migrator exactly: not dirty, every source
    //    migration applied, checksums identical, no extra applied rows.
    let migrator = sqlx::migrate!("./migrations");
    let url = sqlite_url(db_path, true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(LocalDbError::from)?;
    let verdict = current_schema_matches(&pool, &migrator).await;
    pool.close().await;
    verdict
}

async fn current_schema_matches(
    pool: &SqlitePool,
    migrator: &sqlx::migrate::Migrator,
) -> Result<bool, LocalDbError> {
    use sqlx::migrate::Migrate as _;

    let table_name = migrator.table_name.as_ref();
    let present: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?")
            .bind(table_name)
            .fetch_optional(pool)
            .await?;
    // Never migrated here: the guarded path owns creating the bookkeeping table.
    if present.is_none() {
        return Ok(false);
    }

    let mut conn = pool.acquire().await.map_err(LocalDbError::from)?;
    if conn.dirty_version(table_name).await?.is_some() {
        return Ok(false);
    }
    let applied = conn.list_applied_migrations(table_name).await?;
    drop(conn);

    if applied.len() != migrator.iter().count() {
        return Ok(false);
    }
    for known in migrator.iter() {
        match applied.iter().find(|row| row.version == known.version) {
            None => return Ok(false),
            Some(row) if row.checksum != known.checksum => return Ok(false),
            Some(_) => {}
        }
    }
    Ok(true)
}

async fn read_gate_epochs(db_path: &Path) -> Result<(i64, i64), LocalDbError> {
    let gate = read_gate_state(db_path).await?;
    Ok((gate.migration_epoch, gate.engine_epoch))
}

async fn protocol_tables_present(db_path: &Path) -> bool {
    if !db_path.exists() {
        return false;
    }
    let url = sqlite_url(db_path, true);
    if let Ok(pool) = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
    {
        let present: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='core_workspace_gate'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);
        pool.close().await;
        return present.is_some();
    }
    false
}

async fn register_writer(
    db_path: &Path,
    context: &WriterConnectionContext,
    creator_id: &str,
    workspace_identity: &str,
) -> Result<(), LocalDbError> {
    let pool = pool_with_context(db_path, context.clone(), None, single_connection()).await?;
    let mut tx = crate::begin_immediate(&pool).await?;
    sqlx::query(
        "INSERT OR REPLACE INTO core_writer_registration (writer_id, migration_epoch, engine_epoch, mode, creator_id, workspace_identity) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&context.writer_id)
    .bind(context.migration_epoch)
    .bind(context.engine_epoch)
    .bind(context.mode.as_sql())
    .bind(creator_id)
    .bind(workspace_identity)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    pool.close().await;
    Ok(())
}

async fn activate_protocol_epoch(
    db_path: &Path,
    context: &WriterConnectionContext,
) -> Result<i64, LocalDbError> {
    let pool = pool_with_context(db_path, context.clone(), None, single_connection()).await?;
    let mut tx = crate::begin_immediate(&pool).await?;
    let current: Option<(i64,)> =
        sqlx::query_as("SELECT migration_epoch FROM core_workspace_gate WHERE pk = 1")
            .fetch_optional(&mut *tx)
            .await?;
    let current_epoch = current.map_or(0, |row| row.0);
    let next = if current_epoch == 0 { 1 } else { current_epoch };
    if current_epoch == 0 {
        sqlx::query(
            "INSERT OR IGNORE INTO core_workspace_gate (pk, protocol_version, migration_epoch, engine_epoch, owner_id) VALUES (1, 1, 0, 0, NULL)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE core_workspace_gate SET migration_epoch = 1, protocol_version = 1 WHERE pk = 1 AND migration_epoch = 0",
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    pool.close().await;
    Ok(next)
}

async fn acquire_engine_epoch(
    db_path: &Path,
    context: &WriterConnectionContext,
    owner_id: &str,
) -> Result<i64, LocalDbError> {
    let pool = pool_with_context(db_path, context.clone(), None, single_connection()).await?;
    let mut tx = crate::begin_immediate(&pool).await?;
    let gate: (i64, i64) = sqlx::query_as(
        "SELECT migration_epoch, engine_epoch FROM core_workspace_gate WHERE pk = 1",
    )
    .fetch_one(&mut *tx)
    .await?;
    let next_engine = gate.1 + 1;
    let updated = sqlx::query(
        "UPDATE core_workspace_gate SET engine_epoch = ?, owner_id = ? WHERE pk = 1 AND engine_epoch = ? AND migration_epoch = ?",
    )
    .bind(next_engine)
    .bind(owner_id)
    .bind(gate.1)
    .bind(gate.0)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(LocalDbError::OwnerBusy {
            resource: "engine_epoch".to_string(),
        });
    }
    tx.commit().await?;
    pool.close().await;
    Ok(next_engine)
}

async fn advance_migration_epoch(
    db_path: &Path,
    context: &WriterConnectionContext,
) -> Result<i64, LocalDbError> {
    let pool = pool_with_context(db_path, context.clone(), None, single_connection()).await?;
    let mut tx = crate::begin_immediate(&pool).await?;
    let current: (i64,) =
        sqlx::query_as("SELECT migration_epoch FROM core_workspace_gate WHERE pk = 1")
            .fetch_one(&mut *tx)
            .await?;
    let next = current.0 + 1;
    let updated = sqlx::query(
        "UPDATE core_workspace_gate SET migration_epoch = ? WHERE pk = 1 AND migration_epoch = ?",
    )
    .bind(next)
    .bind(current.0)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(LocalDbError::OwnerBusy {
            resource: "migration_epoch".to_string(),
        });
    }
    tx.commit().await?;
    pool.close().await;
    Ok(next)
}

/// Acquire the OS admission locks for `mode` and return the process-local
/// guard, reusing this process's live guard when it is still current (§4.2).
///
/// # Errors
///
/// Returns [`LocalDbError::OwnerBusy`] when a contender holds the mode's OS
/// lock past the 5s budget (shared migration lock) or owns the engine
/// (fail-fast exclusive lock), [`LocalDbError::SchemaMismatch`] when the
/// workspace protocol version is newer than this binary supports,
/// [`LocalDbError::IoWithPath`] when a lock file cannot be opened, and
/// [`LocalDbError::Sqlx`] when the gate read, engine-epoch transaction, or
/// writer registration fails.
pub async fn acquire_writer_guard(
    db_path: &Path,
    creator_id: &str,
    mode: WriterMode,
) -> Result<Arc<WorkspaceWriterGuard>, LocalDbError> {
    let (guard, joined) = acquire_claimed_guard(db_path, creator_id, mode).await?;
    // This entry point hands its caller no lifetime handle for the admission it
    // joined, so that claim is kept for the process (the raw-pool rule).
    if let Some(claim) = joined {
        claim.persist();
    }
    Ok(guard)
}

/// Acquire the OS admission locks for `mode`, joining this process's live
/// admission when it is current.
///
/// A joined admission comes back with a claim already reserved, so the caller
/// holds the admission from the moment it is resolved and can release it
/// however its own lifetime ends.
async fn acquire_claimed_guard(
    db_path: &Path,
    creator_id: &str,
    mode: WriterMode,
) -> Result<(Arc<WorkspaceWriterGuard>, Option<RetainedAdmissionClaim>), LocalDbError> {
    // Reuse this process's live guard when it is still current: a re-opened
    // pool (the daemon's lazy creator-DB open, a restarted in-process test)
    // must neither deadlock on its own OS lock nor be refused as a second
    // owner. Cross-process contenders have no retained guard and still fail
    // closed. A stale epoch (a migration advanced the gate) releases first.
    if mode != WriterMode::Migration {
        if let Some((existing, claim)) = claim_live_admission(db_path, mode) {
            let (gate_migration, gate_engine) = read_gate_epochs(db_path).await?;
            let still_current = existing.migration_epoch == gate_migration.max(1)
                && existing.engine_epoch == (mode == WriterMode::Engine).then_some(gate_engine);
            if still_current {
                return Ok((existing, Some(claim)));
            }
            // The claim held the admission across the gate read; give it back
            // before publishing the superseding one.
            drop(claim);
            release_guard(db_path, mode);
        }
    }

    let workspace_identity = workspace_identity(db_path);
    let writer_id = Uuid::new_v4().to_string();
    let migration_lock_path = migration_lock_path(db_path);
    let engine_lock_path = engine_lock_path(db_path);

    let (migration_lock, engine_lock) = match mode {
        WriterMode::Migration => (acquire_exclusive_lock(&migration_lock_path)?, None),
        WriterMode::Direct => (acquire_shared_lock(&migration_lock_path)?, None),
        WriterMode::Engine => (
            acquire_shared_lock(&migration_lock_path)?,
            Some(try_exclusive_lock(&engine_lock_path)?),
        ),
    };

    let gate = read_gate_state(db_path).await?;
    validate_protocol_version(gate.protocol_version)?;
    let migration_epoch = gate.migration_epoch.max(1);

    let engine_epoch = if mode == WriterMode::Engine {
        let ctx = writer_context(&writer_id, mode, migration_epoch, None);
        Some(acquire_engine_epoch(db_path, &ctx, &writer_id).await?)
    } else {
        None
    };

    let context = writer_context(&writer_id, mode, migration_epoch, engine_epoch);
    register_writer(db_path, &context, creator_id, &workspace_identity).await?;

    Ok((
        Arc::new(WorkspaceWriterGuard {
            db_path: db_path.to_path_buf(),
            writer_id,
            mode,
            migration_epoch,
            engine_epoch,
            admission: Arc::new(WriterAdmission::new(migration_lock, engine_lock)),
        }),
        None,
    ))
}

/// One pooled connection: used by the protocol's own short transactions.
const fn single_connection() -> GuardedPoolOptions {
    GuardedPoolOptions {
        max_connections: 1,
        acquire_timeout: None,
    }
}

/// Open a default-sized pool admitted by `guard`.
///
/// This raw entry point takes no claim, so the pool is opened on exactly the
/// guard the caller passes and the caller keeps that guard alive; the
/// claim-bearing constructors ([`open_guarded_pool_arc`] and
/// [`open_guarded_pool_arc_with`]) are the ones that adopt the published
/// admission when a concurrent open won it.
///
/// # Errors
///
/// Same as [`open_guarded_pool_with`].
pub async fn open_guarded_pool(
    guard: &Arc<WorkspaceWriterGuard>,
) -> Result<SqlitePool, LocalDbError> {
    open_guarded_pool_with(guard, GuardedPoolOptions::default()).await
}

/// Open a guarded pool honouring caller sizing (the daemon's `PoolConfig`).
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] when the pool cannot connect or the
/// per-connection writer context (protocol scalar functions, WAL /
/// foreign-key PRAGMAs) fails to install.
pub async fn open_guarded_pool_with(
    guard: &Arc<WorkspaceWriterGuard>,
    options: GuardedPoolOptions,
) -> Result<SqlitePool, LocalDbError> {
    let context = writer_context(
        &guard.writer_id,
        guard.mode,
        guard.migration_epoch,
        guard.engine_epoch,
    );
    // Every connection of this pool shares the guard's complete OS admission:
    // the locks outlive the guard until SQLite disposes the last connection.
    let pool =
        pool_with_context(&guard.db_path, context, Some(Arc::clone(guard)), options)
            .await?;
    register_pool_handle(guard, &pool);
    Ok(pool)
}

/// Open a default-sized [`GuardedPool`] keeping `guard` alive.
///
/// # Errors
///
/// Same as [`open_guarded_pool_arc_with`].
pub async fn open_guarded_pool_arc(
    guard: Arc<WorkspaceWriterGuard>,
) -> Result<GuardedPool, LocalDbError> {
    open_guarded_pool_arc_with(guard, GuardedPoolOptions::default()).await
}

/// Open a [`GuardedPool`] keeping the guard alive, honouring caller sizing.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] when the pool cannot connect or the
/// per-connection writer context (protocol scalar functions, WAL /
/// foreign-key PRAGMAs) fails to install.
pub async fn open_guarded_pool_arc_with(
    guard: Arc<WorkspaceWriterGuard>,
    options: GuardedPoolOptions,
) -> Result<GuardedPool, LocalDbError> {
    // The reservation may have joined an admission a concurrent open published
    // first: the claim then belongs to THAT identity, so this pool must be
    // opened on the same guard — otherwise one identity's claim would guard
    // another identity's writer context and neither release would line up with
    // the pool it protects. The losing guard is given up here, which releases
    // its own superseded OS locks.
    // The reservation returns the admission AND its claim together: this pool
    // is opened on exactly that guard, so its writer context, its claim and the
    // registry entry can never name different identities.
    let (guard, claim) = reserve_retained_admission(&guard);
    open_guarded_pool_with_claim(guard, claim, options).await
}

/// Open a [`GuardedPool`] while holding an already-reserved claim.
///
/// The claim is held across the awaited pool construction, so it keeps the
/// admission alive while a concurrent holder closes and releases it; a failed
/// or cancelled construction drops the claim and leaves no orphan entry.
async fn open_guarded_pool_with_claim(
    guard: Arc<WorkspaceWriterGuard>,
    claim: RetainedAdmissionClaim,
    options: GuardedPoolOptions,
) -> Result<GuardedPool, LocalDbError> {
    let context = writer_context(
        &guard.writer_id,
        guard.mode,
        guard.migration_epoch,
        guard.engine_epoch,
    );
    // The claim-bearing pool shares the guard's complete OS admission with
    // every one of its connections, and keeps the published guard itself for
    // as long as the pool is open.
    let pool = pool_with_context(&guard.db_path, context, Some(Arc::clone(&guard)), options).await?;
    let registration = register_pool_handle(&guard, &pool);
    Ok(GuardedPool {
        guard,
        pool: Some(pool),
        registration,
        claim,
    })
}

/// Run pending schema migrations under the guarded protocol.
///
/// A workspace whose schema already matches this binary is a validated no-op
/// fast path (no cooperative quiescence, no exclusive lock). Otherwise the
/// run waits for cooperative pools to quiesce, closes them, takes the
/// exclusive migration lock, applies pending migrations, and advances /
/// activates the durable migration epoch.
///
/// # Errors
///
/// Returns [`LocalDbError::SchemaMismatch`] when the workspace protocol
/// version is unsupported (checked before and after the run),
/// [`LocalDbError::OwnerBusy`] when cooperative pools stay active past the
/// quiescence budget or the exclusive migration lock stays contended past
/// the 5s budget, [`LocalDbError::Migrate`] when a migration fails (a
/// transient cross-process race — `SQLITE_BUSY` or a bookkeeping collision —
/// is retried), and [`LocalDbError::Sqlx`] / [`LocalDbError::IoWithPath`]
/// when gate reads, writer registration, or lock-file IO fail.
pub async fn run_guarded_migrations(db_path: &Path) -> Result<(), LocalDbError> {
    let gate_before = read_gate_state(db_path).await?;
    validate_protocol_version(gate_before.protocol_version)?;
    // v1.189 P1 fix round 1: no-op fast path.
    //
    // The exclusive migration lock exists to serialize *schema changes*; a
    // caller that finds the schema already current has nothing to serialize
    // and must not require cooperative quiescence. Without this, an engine
    // owner opening on a live cooperative pool (daemon boot pool + World KB
    // handler) is refused with `cooperative pools still active` even though
    // no migration is pending.
    //
    // Fail-closed: every predicate must hold, and each is an equality/absence
    // check, so an unknown or newer workspace state falls through to the
    // guarded path rather than being silently accepted.
    if migrations_are_current(db_path, &gate_before).await? {
        return Ok(());
    }
    await_cooperative_quiescence(db_path).await?;
    if !cooperative_pools_quiesced(db_path) {
        return Err(LocalDbError::OwnerBusy {
            resource: format!(
                "{}: cooperative pools still active after close",
                canonical_db_path(db_path).display()
            ),
        });
    }
    release_retained_guards(db_path);
    let _migration_lock = acquire_exclusive_lock(&migration_lock_path(db_path))?;
    let writer_id = Uuid::new_v4().to_string();
    let workspace_identity = workspace_identity(db_path);
    let ctx = writer_context(
        &writer_id,
        WriterMode::Migration,
        gate_before.migration_epoch,
        None,
    );
    if protocol_tables_present(db_path).await {
        register_writer(db_path, &ctx, BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
    }
    let pool = pool_with_context(db_path, ctx.clone(), None, single_connection()).await?;
    let before_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    crate::run_migrations_with_retry(&pool, crate::run_migrations).await?;
    let after_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap_or(before_count);
    pool.close().await;
    let gate_after = read_gate_state(db_path).await?;
    validate_protocol_version(gate_after.protocol_version)?;
    if gate_before.migration_epoch == 0 && gate_after.migration_epoch == 0 {
        register_writer(db_path, &ctx, BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
        let activated = activate_protocol_epoch(db_path, &ctx).await?;
        register_writer(
            db_path,
            &writer_context(&writer_id, WriterMode::Migration, activated, None),
            BOOTSTRAP_CREATOR_ID,
            &workspace_identity,
        )
        .await?;
    } else if after_count > before_count {
        let advanced = advance_migration_epoch(
            db_path,
            &writer_context(
                &writer_id,
                WriterMode::Migration,
                gate_after.migration_epoch,
                None,
            ),
        )
        .await?;
        register_writer(
            db_path,
            &writer_context(&writer_id, WriterMode::Migration, advanced, None),
            BOOTSTRAP_CREATOR_ID,
            &workspace_identity,
        )
        .await?;
    } else if protocol_tables_present(db_path).await {
        register_writer(
            db_path,
            &writer_context(
                &writer_id,
                WriterMode::Migration,
                gate_after.migration_epoch,
                None,
            ),
            BOOTSTRAP_CREATOR_ID,
            &workspace_identity,
        )
        .await?;
    }
    Ok(())
}

/// Open a default-sized admitted pool (migrations ensured, `mode` admitted).
///
/// # Errors
///
/// Same as [`open_admitted_pool_with`].
pub async fn open_admitted_pool(
    db_path: &Path,
    creator_id: &str,
    mode: WriterMode,
) -> Result<SqlitePool, LocalDbError> {
    open_admitted_pool_with(db_path, creator_id, mode, GuardedPoolOptions::default()).await
}

/// Open an admitted pool honouring caller sizing.
///
/// Migrates first when the workspace predates the protocol (a cooperative
/// caller must not have to remember the order), then admits `mode` and opens
/// the pool with the installed scalar functions on every connection.
///
/// # Errors
///
/// Returns [`LocalDbError::SchemaMismatch`] for an unsupported protocol
/// version, [`LocalDbError::OwnerBusy`] when the admission locks are
/// contended or a guarded migration run cannot quiesce cooperative pools,
/// and [`LocalDbError::Sqlx`] / [`LocalDbError::Migrate`] /
/// [`LocalDbError::IoWithPath`] when the gate reads, guarded migration,
/// writer registration, or pool open fail.
pub async fn open_admitted_pool_with(
    db_path: &Path,
    creator_id: &str,
    mode: WriterMode,
    options: GuardedPoolOptions,
) -> Result<SqlitePool, LocalDbError> {
    let gate = read_gate_state(db_path).await?;
    validate_protocol_version(gate.protocol_version)?;
    if !protocol_tables_present(db_path).await || needs_activation_recovery(db_path).await? {
        run_guarded_migrations(db_path).await?;
    }
    // One strong pair for both fresh and joined opens. A joined acquisition
    // already returns the published guard; a fresh one publishes (or joins a
    // concurrent winner) inside `reserve_retained_admission`, which hands back
    // the admission and its claim together. Nothing here re-selects an identity.
    let (guard, joined) = acquire_claimed_guard(db_path, creator_id, mode).await?;
    let (published, claim) = match joined {
        Some(claim) => (guard, claim),
        None => reserve_retained_admission(&guard),
    };
    // A raw pool keeps its admission for the process: the entry becomes the
    // retained owner, whether this open published it or joined it.
    if !install_retained_admission(&published, &claim) {
        return Err(LocalDbError::OwnerBusy {
            resource: format!(
                "{}: a newer admission replaced the one this raw open reserved",
                canonical_db_path(db_path).display()
            ),
        });
    }
    claim.persist();
    open_guarded_pool_with(&published, options).await
}

/// Open another cooperative pool on a live in-process engine guard when one
/// already exists (e.g. daemon `DbPool` + `CoreService` co-hosted in one process).
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] when the cooperative pool cannot be
/// opened. `Ok(None)` means this process holds no live engine guard.
pub async fn join_live_engine_pool(
    db_path: &Path,
    options: GuardedPoolOptions,
) -> Result<Option<GuardedPool>, LocalDbError> {
    // The claim is reserved in the same registry critical section as the
    // lookup, so the admission cannot be released between joining it and
    // owning a claim on it, and it is held across the awaited pool open.
    let Some((guard, claim)) = claim_live_admission(db_path, WriterMode::Engine) else {
        return Ok(None);
    };
    Ok(Some(
        open_guarded_pool_with_claim(guard, claim, options).await?,
    ))
}

/// Migrate, take the single engine ownership, and open a guarded pool.
///
/// This is the engine-owner entry point (the daemon's `DbPool`): the returned
/// guard holds `state.db.engine.lock` exclusively and persists the advanced
/// engine epoch, so no second process can become an effect owner.
///
/// # Errors
///
/// Returns [`LocalDbError::OwnerBusy`] when another live writer owns the
/// engine (refused immediately, §4.1), the guarded-migration / admission /
/// pool-open errors documented on [`run_guarded_migrations`] and
/// [`acquire_writer_guard`], and [`LocalDbError::Sqlx`] when version
/// seeding fails.
pub async fn init_engine_pool(
    db_path: &Path,
    creator_id: &str,
    options: GuardedPoolOptions,
) -> Result<GuardedPool, LocalDbError> {
    run_guarded_migrations(db_path).await?;
    // The claim is reserved with the publication, before the awaited pool
    // open: a concurrent open can join this admission, and an initialization
    // error or a cancelled future drops the claim instead of leaving an
    // ownerless entry and its OS locks behind.
    let (guard, joined) = acquire_claimed_guard(db_path, creator_id, WriterMode::Engine).await?;
    let (guard, claim) = match joined {
        Some(claim) => (guard, claim),
        None => reserve_retained_admission(&guard),
    };
    let guarded = open_guarded_pool_with_claim(guard, claim, options).await?;
    if let Err(error) = crate::seed_versions(guarded.pool()).await {
        // A failed seed ends this open: close the pool it just built (no other
        // holder exists yet) so the claim released below leaves no admission
        // and no cooperative pool behind.
        guarded.pool().close().await;
        return Err(error);
    }
    Ok(guarded)
}

/// Migrate, admit a direct writer, and open a [`GuardedPool`].
///
/// The cooperative entry point (CLI tools, direct adapters): the returned
/// guard holds a shared `state.db.migration.lock`, never the exclusive
/// engine lock.
///
/// # Errors
///
/// Returns the guarded-migration / admission / pool-open errors documented
/// on [`run_guarded_migrations`] and [`acquire_writer_guard`], and
/// [`LocalDbError::Sqlx`] when version seeding fails.
pub async fn init_guarded_pool(
    db_path: &Path,
    creator_id: &str,
) -> Result<GuardedPool, LocalDbError> {
    run_guarded_migrations(db_path).await?;
    let (guard, joined) = acquire_claimed_guard(db_path, creator_id, WriterMode::Direct).await?;
    let (guard, claim) = match joined {
        Some(claim) => (guard, claim),
        None => reserve_retained_admission(&guard),
    };
    let guarded =
        open_guarded_pool_with_claim(guard, claim, GuardedPoolOptions::default()).await?;
    if let Err(error) = crate::seed_versions(guarded.pool()).await {
        // See `init_engine_pool`: a failed seed closes the pool it built so the
        // released claim leaves nothing behind.
        guarded.pool().close().await;
        return Err(error);
    }
    Ok(guarded)
}

impl Drop for GuardedPool {
    /// Give up this handle's pool FIRST: if it was the last handle, the pool
    /// marks itself closed here, so the settlement below already sees a pool no
    /// other holder can use. A pool a clone still uses is left registered, and
    /// the admission stays held with it.
    fn drop(&mut self) {
        self.pool.take();
        self.settle_dropped_pool();
    }
}

impl WorkspaceWriterGuard {
    #[must_use]
    pub fn writer_id(&self) -> &str {
        &self.writer_id
    }

    #[must_use]
    pub const fn mode(&self) -> WriterMode {
        self.mode
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::future::Future;

    use super::*;

    /// A creator whose open fails after another open already joined gives up
    /// only its OWN claim: the adopter keeps the admission, a third open still
    /// joins it, and the LAST claim removes the entry instead of leaking it.
    #[tokio::test]
    async fn failed_creator_release_keeps_a_joined_adopter_joinable() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        let creator =
            init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
                .await
                .unwrap();
        // Published before the awaited pool open finishes, so a concurrent
        // open joins the creator's admission instead of contending for the OS
        // lock.
        let adopter = join_live_engine_pool(&db_path, GuardedPoolOptions::default())
            .await
            .unwrap()
            .expect("a concurrent open joins the creator's live admission");

        // The creator's memory pin fails: its pool is closed and it gives up
        // its own claim while the adopter keeps using the admission.
        creator.pool().close().await;
        creator.release_retained_admission();

        let third = join_live_engine_pool(&db_path, GuardedPoolOptions::default())
            .await
            .unwrap()
            .expect("a third open still joins the live admission");
        sqlx::query("SELECT 1").execute(third.pool()).await.unwrap();
        assert_eq!(third.guard().writer_id(), adopter.guard().writer_id());

        // The last claim removes the entry and its OS admission.
        adopter.pool().close().await;
        adopter.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Engine).is_some());
        let third_writer = third.guard().writer_id().to_string();
        third.pool().close().await;
        third.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Engine).is_none());
        // The released handles must be dropped for the OS lock file to close.
        drop((creator, adopter, third));
        assert!(join_live_engine_pool(&db_path, GuardedPoolOptions::default())
            .await
            .unwrap()
            .is_none());

        // A later open takes a fresh admission rather than joining a settled
        // generation.
        let fresh =
            init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
                .await
                .unwrap();
        assert_ne!(fresh.guard().writer_id(), third_writer);
        fresh.pool().close().await;
        fresh.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Engine).is_none());
    }

    /// Releasing one mode's last claim leaves the other mode's admission and
    /// its OS lock in place; the other mode's own release still cleans up.
    #[tokio::test]
    async fn mode_claims_release_independently() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        let engine =
            init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
                .await
                .unwrap();
        let direct = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();

        direct.pool().close().await;
        direct.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Direct).is_none());
        assert!(
            retained_guard(&db_path, WriterMode::Engine).is_some(),
            "the engine owner's admission survives a direct release"
        );

        let joined = join_live_engine_pool(&db_path, GuardedPoolOptions::default())
            .await
            .unwrap()
            .expect("the engine owner's admission is still joinable");
        assert_eq!(joined.guard().writer_id(), engine.guard().writer_id());
        joined.pool().close().await;
        joined.release_retained_admission();

        engine.pool().close().await;
        engine.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Engine).is_none());
    }

    /// A confirmed close of one direct user keeps another direct user's live
    /// admission: the next open still joins it instead of taking a new one.
    #[tokio::test]
    async fn joined_direct_release_keeps_the_other_users_admission() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        let creator = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        let joiner = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        assert_eq!(
            joiner.guard().writer_id(),
            creator.guard().writer_id(),
            "a second direct open joins the live admission"
        );

        creator.pool().close().await;
        creator.release_retained_admission();
        // A repeated release is a no-op and never consumes the joiner's claim.
        creator.release_retained_admission();

        let third = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        assert_eq!(
            third.guard().writer_id(),
            joiner.guard().writer_id(),
            "a later open joins the still-live direct admission"
        );

        joiner.pool().close().await;
        joiner.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Direct).is_some());
        third.pool().close().await;
        third.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Direct).is_none());
    }

    /// Two fresh same-mode acquisitions can both miss the live lookup and take
    /// their own OS locks before either publishes. Only one publishes; the
    /// other JOINS it, so no claim is ever carried across two writer
    /// identities and the last release frees the single OS admission.
    #[tokio::test]
    async fn concurrent_fresh_direct_acquisitions_keep_one_identity() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let bootstrap = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        bootstrap.pool().close().await;
        bootstrap.release_retained_admission();
        drop(bootstrap);

        let (first, first_joined) =
            acquire_claimed_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
                .await
                .unwrap();
        let (second, second_joined) =
            acquire_claimed_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
                .await
                .unwrap();
        assert!(
            first_joined.is_none() && second_joined.is_none(),
            "both acquisitions are fresh: neither saw a published admission"
        );
        let first_id = first.writer_id().to_string();
        let second_id = second.writer_id().to_string();
        assert_ne!(first_id, second_id, "two separate OS admissions were taken");

        let first_claim = reserve_retained_admission(&first);
        let second_claim = reserve_retained_admission(&second);
        let published = retained_guard(&db_path, WriterMode::Direct).expect("published");
        assert_eq!(
            published.writer_id(),
            first_id,
            "the first publisher keeps its identity; the other joins it"
        );

        // The loser's own guard goes; its claim is on the published admission.
        drop(second);
        drop(first_claim);
        assert!(
            retained_guard(&db_path, WriterMode::Direct).is_some(),
            "the joiner's claim still holds the published admission"
        );
        drop(second_claim);
        assert!(
            retained_guard(&db_path, WriterMode::Direct).is_none(),
            "the last claim released the single admission"
        );
        // The published guard's own handles go with it.
        drop(published);
        drop(first);
        let fence = acquire_store_reset_fence(&db_path)
            .expect("both shared admissions are gone");
        drop(fence);
    }

    /// F10: a real in-flight `init_guarded_pool` future is cancelled after its
    /// admission is registered (and while the seed write may already have run),
    /// and the OS admission is released on its own.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_open_future_releases_its_admission() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let bootstrap = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        bootstrap.pool().close().await;
        bootstrap.release_retained_admission();
        drop(bootstrap);

        let opening_path = db_path.clone();
        let opening = tokio::spawn(async move {
            init_guarded_pool(&opening_path, BOOTSTRAP_CREATOR_ID).await
        });
        let mut registered = false;
        for _ in 0..500_000 {
            if retained_guard(&db_path, WriterMode::Direct).is_some() {
                registered = true;
                break;
            }
            if opening.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(registered, "the open published its admission before cancel");
        opening.abort();
        let _ = opening.await;

        let mut fence = None;
        for _ in 0..500_000 {
            if let Ok(acquired) = acquire_store_reset_fence(&db_path) {
                fence = Some(acquired);
                break;
            }
            tokio::task::yield_now().await;
        }
        drop(fence.expect("the cancelled open released its own OS admission"));
        assert!(retained_guard(&db_path, WriterMode::Direct).is_none());
    }

    /// F10: the OS admission belongs to real resources, not to a driver task. A
    /// pool built on a runtime and dropped after that runtime is gone releases
    /// its lock once the native SQLite workers finish disposing.
    #[test]
    fn admission_is_released_without_a_driver() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let guarded = runtime
            .block_on(init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID))
            .unwrap();
        let clone = guarded.clone_pool();
        drop(runtime);
        drop(clone);
        drop(guarded);

        // No runtime left: wait for the real OS lock with a bounded, non-sleeping
        // acquire loop while the native SQLite workers dispose asynchronously.
        let mut fence = None;
        for _ in 0..200_000 {
            if let Ok(acquired) = acquire_store_reset_fence(&db_path) {
                fence = Some(acquired);
                break;
            }
            std::thread::yield_now();
        }
        drop(fence.expect("the OS admission is released without a driver"));
    }

    /// F10: an open that was registered and seeded but never delivered (the
    /// exact state a cancellation leaves behind) releases its own OS admission
    /// without a purge, a helper or a later local poll.
    #[tokio::test]
    async fn undelivered_open_releases_its_admission() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let bootstrap = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        bootstrap.pool().close().await;
        bootstrap.release_retained_admission();
        drop(bootstrap);

        let opening = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .expect("registration and seeding complete");
        assert!(retained_guard(&db_path, WriterMode::Direct).is_some());
        // Never delivered: the caller goes away (cancel/error) and nothing else
        // runs — no close call, no path-wide release.
        drop(opening);

        // Retirement follows the pool itself: dropping the undelivered handle
        // drops the pool's owner slot, and SQLx tears the pool inner down on
        // the driver, so the admission settles promptly rather than instantly.
        // The OS signal is what matters, and it is polled without a purge or a
        // helper.
        let mut admission = retained_guard(&db_path, WriterMode::Direct);
        let mut fence = None;
        for _ in 0..4096 {
            admission = retained_guard(&db_path, WriterMode::Direct);
            if admission.is_none() {
                if let Ok(acquired) = acquire_store_reset_fence(&db_path) {
                    fence = Some(acquired);
                    break;
                }
            }
            tokio::task::yield_now().await;
        }
        assert!(
            admission.is_none(),
            "an undelivered open leaves no live admission"
        );
        drop(fence.expect("the undelivered open's OS admission settles on its own"));
    }

    /// The PUBLIC guarded constructor must pair every claim with the admission
    /// it actually belongs to. Two genuinely fresh same-mode Direct guards are
    /// taken before either publishes; the second public open used to open a
    /// pool on its own losing guard while holding a claim on the published one.
    #[tokio::test]
    async fn public_guarded_open_uses_the_published_admission() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let bootstrap = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        bootstrap.pool().close().await;
        bootstrap.release_retained_admission();
        drop(bootstrap);

        let (first, first_joined) =
            acquire_claimed_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
                .await
                .unwrap();
        let (second, second_joined) =
            acquire_claimed_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
                .await
                .unwrap();
        assert!(
            first_joined.is_none() && second_joined.is_none(),
            "both guards are fresh: neither saw a published admission"
        );
        assert_ne!(
            first.writer_id(),
            second.writer_id(),
            "two separate OS admissions were taken"
        );

        // The first public open publishes its own identity.
        let opened = open_guarded_pool_arc(first.clone()).await.unwrap();
        let published = opened.guard().writer_id().to_string();
        assert_eq!(published, first.writer_id());

        // The second supplies its losing guard; the returned pool must belong
        // to the published admission it now holds a claim on.
        let joined = open_guarded_pool_arc(second.clone()).await.unwrap();
        assert_eq!(
            joined.guard().writer_id(),
            published,
            "the public constructor adopts the published guard, not the supplied loser"
        );
        let (context,): (String,) = sqlx::query_as("SELECT nexus_writer_id()")
            .fetch_one(joined.pool())
            .await
            .unwrap();
        assert_eq!(
            context, published,
            "the pool's real connection context is the published admission"
        );

        // Both public consumers end; the single admission goes with the last.
        opened.pool().close().await;
        opened.release_retained_admission();
        joined.pool().close().await;
        joined.release_retained_admission();
        drop((opened, joined, first, second));
        assert!(
            retained_guard(&db_path, WriterMode::Direct).is_none(),
            "the last claim released the published admission"
        );
        let fence = acquire_store_reset_fence(&db_path)
            .expect("no writer admission survives the public consumers");
        drop(fence);
    }

    /// The provider-shaped close: closing each holder's own pool releases its
    /// admission, so the last one frees the OS locks instead of leaving them to
    /// a dead pool. Dropping a pool without closing keeps the admission, which
    /// is the same rule that protects a cloned handle.
    #[tokio::test]
    async fn closed_handles_release_the_last_admission() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        let engine =
            init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
                .await
                .unwrap();
        let direct = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        direct.pool().close().await;
        drop(direct);
        assert!(retained_guard(&db_path, WriterMode::Direct).is_none());
        assert!(retained_guard(&db_path, WriterMode::Engine).is_some());

        engine.pool().close().await;
        drop(engine);
        assert!(retained_guard(&db_path, WriterMode::Engine).is_none());
        let fence = acquire_store_reset_fence(&db_path)
            .expect("no writer admission survives the closed holders");
        drop(fence);
    }

    /// A pool-open failure after the claim was reserved must not leave an
    /// ownerless admission or its lock behind.
    #[tokio::test]
    async fn failed_pool_open_releases_the_reserved_claim() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let bootstrap = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        bootstrap.pool().close().await;
        bootstrap.release_retained_admission();
        drop(bootstrap);

        let guard = acquire_writer_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
            .await
            .unwrap();
        // A directory where the pool expects its database file: the awaited
        // pool construction fails for real.
        fs::remove_file(&db_path).unwrap();
        fs::create_dir(&db_path).unwrap();
        let error = open_guarded_pool_arc(guard.clone())
            .await
            .err()
            .expect("a pool cannot open a directory");
        assert!(matches!(error, LocalDbError::Sqlx(_)), "got {error:?}");

        drop(guard);
        let fence = acquire_store_reset_fence(&db_path)
            .expect("the failed construction released its claim");
        drop(fence);
    }

    /// A cancelled construction — the future dropped after the claim was
    /// reserved — gives the claim back too.
    #[tokio::test]
    async fn cancelled_construction_releases_the_reserved_claim() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let bootstrap = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        bootstrap.pool().close().await;
        bootstrap.release_retained_admission();
        drop(bootstrap);

        let guard = acquire_writer_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
            .await
            .unwrap();
        {
            let mut opening = std::pin::pin!(open_guarded_pool_arc(guard.clone()));
            std::future::poll_fn(|cx| {
                // The first poll reserves the claim and then waits on the
                // database connect, so the future is cancelled mid-open.
                assert!(
                    opening.as_mut().poll(cx).is_pending(),
                    "the construction must yield at its awaited connect"
                );
                std::task::Poll::Ready(())
            })
            .await;
        }

        drop(guard);
        let fence = acquire_store_reset_fence(&db_path)
            .expect("the cancelled construction released its claim");
        drop(fence);
    }

    /// A joining open reserves its claim with the lookup, so it keeps the
    /// admission alive while its own pool is still being constructed, and the
    /// creator going away in that window does not cost a third open the join.
    #[tokio::test]
    async fn in_flight_join_survives_the_creators_release() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        let creator =
            init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
                .await
                .unwrap();
        let mut joining = std::pin::pin!(join_live_engine_pool(
            &db_path,
            GuardedPoolOptions::default()
        ));
        std::future::poll_fn(|cx| {
            assert!(
                joining.as_mut().poll(cx).is_pending(),
                "the join must yield at its awaited pool open"
            );
            std::task::Poll::Ready(())
        })
        .await;

        // The creator's open ends while the joiner is still constructing.
        creator.pool().close().await;
        creator.release_retained_admission();

        let third = join_live_engine_pool(&db_path, GuardedPoolOptions::default())
            .await
            .unwrap()
            .expect("a third open still joins the admission");
        let joined = joining.await.unwrap().expect("the pending join completes");
        assert_eq!(joined.guard().writer_id(), third.guard().writer_id());

        joined.pool().close().await;
        joined.release_retained_admission();
        third.pool().close().await;
        third.release_retained_admission();
        assert!(retained_guard(&db_path, WriterMode::Engine).is_none());
    }

    /// The seed step runs after the pool exists; its failure drops that handle,
    /// which must give the claim back instead of leaving an ownerless
    /// admission behind.
    #[tokio::test]
    async fn failed_seed_releases_the_constructed_admission() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let first = init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
            .await
            .unwrap();
        first.pool().close().await;
        first.release_retained_admission();
        drop(first);

        // A real SQL failure at the seed step: the version table rejects the
        // value the seeding writes.
        let pool = SqlitePool::connect(&sqlite_url(&db_path, false))
            .await
            .expect("raw pool");
        sqlx::query("DROP TABLE workspace_meta")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE workspace_meta (\
                 key TEXT NOT NULL PRIMARY KEY, \
                 value TEXT NOT NULL CHECK (value = '')\
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let error = init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
            .await
            .err()
            .expect("the seed step must fail");
        assert!(matches!(error, LocalDbError::Sqlx(_)), "got {error:?}");
        let fence = acquire_store_reset_fence(&db_path)
            .expect("a failed initialization released its locks");
        drop(fence);
    }
    /// F13: the per-connection pin decision keeps three outcomes distinct — an
    /// intentional internal registration pool pins nothing, a live pool pins
    /// the published admission, and a pool whose admission was already retired
    /// fails closed instead of silently installing nothing.
    #[tokio::test]
    async fn lost_pool_admission_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        run_guarded_migrations(&db_path).await.unwrap();
        let guard = acquire_writer_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
            .await
            .unwrap();

        let slot = PoolAdmissionSlot::new(Arc::clone(&guard));
        assert!(
            pinned_admission(Some(&slot)).unwrap().is_some(),
            "a live pool hands out its own admission"
        );

        // Close-start retirement is once-only and really gives the admission up.
        slot.retire();
        slot.retire();
        assert!(
            slot.admission().is_none(),
            "a retired slot holds no admission"
        );
        let error = pinned_admission(Some(&slot)).expect_err("a lost admission fails closed");
        assert!(
            format!("{error}").contains("retired while the pool was still open"),
            "the refusal names the lost admission, got {error}"
        );
        assert!(
            pinned_admission(None).unwrap().is_none(),
            "an intentional internal registration pool pins nothing"
        );
    }

    /// F15: a reservation hands back the published admission it selected,
    /// strongly, from the registry critical section through installation.
    ///
    /// The window that used to break this: B reserves published A, then A's
    /// previous final owner really ends before the pool is installed. The
    /// installed pool must still carry A's identity and context, stay joinable,
    /// and keep the real OS admission until its own end.
    #[tokio::test]
    async fn reservation_keeps_the_published_guard_until_installed() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        // A is published and really owns the OS locks through an open pool.
        let a_pool = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        let key = (canonical_db_path(&db_path), WriterMode::Direct);
        let a_guard = retained_guard(&db_path, WriterMode::Direct).expect("A is published");
        let a_id = a_guard.writer_id().to_string();

        // B is a distinct real guard that has published nothing: take its OS
        // locks while the entry is out of the way, then restore A's publication
        // so that B is the loser at reservation time.
        ACTIVE_GUARDS.lock().unwrap().remove(&key);
        let b_guard = acquire_writer_guard(&db_path, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
            .await
            .unwrap();
        assert_ne!(b_guard.writer_id(), a_id, "B is a different identity");
        ACTIVE_GUARDS.lock().unwrap().insert(
            key.clone(),
            RetainedGuardEntry {
                owner: AdmissionOwner::Observed(Arc::downgrade(&a_guard)),
                writer_id: a_id.clone(),
                claims: 1,
            },
        );

        // The reservation selects A and holds it.
        let (published, claim) = reserve_retained_admission(&b_guard);
        assert_eq!(
            published.writer_id(),
            a_id,
            "the reservation selected the published admission"
        );

        // A's previous final owner REALLY ends here, before any installation —
        // and so does B's supplied guard, which held its own superseded locks.
        // From here the reservation is the ONLY thing that can keep the
        // published admission, so the fence probe is about A alone.
        a_pool.pool().close().await;
        a_pool.release_retained_admission();
        drop(a_pool);
        drop(a_guard);
        drop(b_guard);
        assert!(
            acquire_store_reset_fence(&db_path).is_err(),
            "the reservation alone still keeps the published OS locks"
        );

        // Installation under exactly that admission.
        let installed =
            open_guarded_pool_with_claim(published, claim, GuardedPoolOptions::default())
                .await
                .expect("installed pool");
        let mut conn = installed.pool().acquire().await.unwrap();
        let (writer_id,): (String,) = sqlx::query_as("SELECT nexus_writer_id()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(writer_id, a_id, "the pool carries A's identity");
        drop(conn);

        // The existing join path still resolves that same admission.
        let (joined, joined_claim) =
            claim_live_admission(&db_path, WriterMode::Direct).expect("joinable while open");
        assert_eq!(joined.writer_id(), a_id, "the join names the published admission");
        drop(joined_claim);
        drop(joined);

        installed.pool().close().await;
        installed.release_retained_admission();
        drop(installed);
        for _ in 0..4096 {
            if let Ok(fence) = acquire_store_reset_fence(&db_path) {
                drop(fence);
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("the installed admission's OS lock was never released");
    }

}
