//! Cross-process workspace writer protocol: OS locks, epochs, registration, guarded pools.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use nexus_storage_guard::{
    install_writer_functions, WriterConnectionContext, WriterMode as GuardMode,
};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::LocalDbError;

const LOCK_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
const SUPPORTED_PROTOCOL_VERSION: i64 = 1;
pub const BOOTSTRAP_CREATOR_ID: &str = "bootstrap";

/// One cooperative pool handle this process opened over a database.
///
/// Kept for migration quiescence: a pending schema change waits until every
/// handle reports closed, and closes them if it must.
struct RegisteredPool {
    id: u64,
    handle: SqlitePool,
}

/// One database's live admission: the guard that holds its OS locks and the
/// number of holders claiming it.
struct RetainedGuardEntry {
    guard: Arc<WorkspaceWriterGuard>,
    /// Live holders of this admission: the open that published it and every
    /// open that joined it. The entry — and the OS locks its guard carries —
    /// is dropped only when the LAST claim is released.
    claims: usize,
}

static POOL_REG_ID: AtomicU64 = AtomicU64::new(1);

struct CooperativePoolRegistration {
    path: PathBuf,
    id: u64,
}

impl Drop for CooperativePoolRegistration {
    fn drop(&mut self) {
        unregister_pool_handle(&self.path, self.id);
    }
}

/// This process's live writer admissions, keyed by
/// `(canonical db path, mode)`.
///
/// Keyed per mode so releasing a direct claim never drops the engine owner's
/// admission, and identity-checked by `writer_id` so a stale holder can never
/// release an entry a later acquisition replaced.
static ACTIVE_GUARDS: LazyLock<Mutex<HashMap<(PathBuf, WriterMode), RetainedGuardEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// This process's open cooperative pools, keyed by canonical db path.
///
/// Deliberately separate from [`ACTIVE_GUARDS`]: a cloned pool handle
/// ([`GuardedPool::clone_pool`]) can outlive the handle that opened it, so
/// migration quiescence follows the pool handles, not the admission claims.
static ACTIVE_POOLS: LazyLock<Mutex<HashMap<PathBuf, Vec<RegisteredPool>>>> =
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
    migration_lock: File,
    engine_lock: Option<File>,
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
        if entry.guard.writer_id != self.writer_id {
            return;
        }
        entry.claims = entry.claims.saturating_sub(1);
        if entry.claims == 0 {
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
    pool: SqlitePool,
    _pool_registration: CooperativePoolRegistration,
    /// This handle's live claim on its retained admission entry.
    ///
    /// Declared last so the pool and its registration are dropped before the
    /// claim can remove the entry that keeps the OS admission alive; dropping
    /// the handle releases it, whichever holder's lifetime ends.
    claim: RetainedAdmissionClaim,
}

impl GuardedPool {
    #[must_use]
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[must_use]
    pub fn guard(&self) -> &WorkspaceWriterGuard {
        &self.guard
    }

    /// Clone the underlying pool and register the handle for cooperative quiescence.
    #[must_use]
    pub fn clone_pool(&self) -> SqlitePool {
        let pool = self.pool.clone();
        register_pool_handle(&self.guard, &pool);
        pool
    }

    /// Give up THIS handle's live claim on its retained admission entry.
    ///
    /// The handle's own pool must be closed first: the registry entry keeps the
    /// guard (and therefore the OS admission) alive, and the entry is removed
    /// only when the LAST claim is released. Another open's admission — the
    /// creator's or a joiner's, in this mode or any other — is never evicted,
    /// and a second call is a no-op.
    pub fn release_retained_admission(&self) {
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
/// The entry exists (and is joinable) from the moment the claim is reserved,
/// and the claim keeps it alive through the awaited pool construction that
/// follows; the caller releases it when that construction or seeding fails, or
/// hands it to the constructed handle.
fn reserve_retained_admission(guard: &Arc<WorkspaceWriterGuard>) -> RetainedAdmissionClaim {
    let key = (canonical_db_path(&guard.db_path), guard.mode);
    let mut guards = ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned");
    match guards.get_mut(&key) {
        Some(entry) if entry.guard.writer_id == guard.writer_id => entry.claims += 1,
        Some(entry) => {
            // The key names a different admission (a superseding acquisition
            // replaced it): this guard becomes the live one.
            entry.guard = Arc::clone(guard);
            entry.claims += 1;
        }
        None => {
            guards.insert(
                key.clone(),
                RetainedGuardEntry {
                    guard: Arc::clone(guard),
                    claims: 1,
                },
            );
        }
    }
    drop(guards);
    RetainedAdmissionClaim::held(key, guard.writer_id.clone())
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
    let entry = guards.get_mut(&key)?;
    entry.claims += 1;
    Some((
        Arc::clone(&entry.guard),
        RetainedAdmissionClaim::held(key, entry.guard.writer_id.clone()),
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
    reserve_retained_admission(&guard).persist();
}

fn register_pool_handle(guard: &Arc<WorkspaceWriterGuard>, pool: &SqlitePool) -> u64 {
    let id = POOL_REG_ID.fetch_add(1, Ordering::Relaxed);
    let key = canonical_db_path(&guard.db_path);
    ACTIVE_POOLS
        .lock()
        .expect("pool registry poisoned")
        .entry(key)
        .or_default()
        .push(RegisteredPool {
            id,
            handle: pool.clone(),
        });
    id
}

fn unregister_pool_handle(key: &Path, id: u64) {
    let mut pools = ACTIVE_POOLS.lock().expect("pool registry poisoned");
    if let Some(registered) = pools.get_mut(key) {
        registered.retain(|reg| reg.id != id);
        if registered.is_empty() {
            pools.remove(key);
        }
    }
}

/// Whether every cooperative pool this process opened on `db_path` reports
/// closed, dropping the handles that have.
fn cooperative_pools_quiesced(db_path: &Path) -> bool {
    let target = canonical_db_path(db_path);
    let mut pools = ACTIVE_POOLS.lock().expect("pool registry poisoned");
    let Some(registered) = pools.get_mut(&target) else {
        return true;
    };
    registered.retain(|reg| !reg.handle.is_closed());
    let quiesced = registered.is_empty();
    if quiesced {
        pools.remove(&target);
    }
    quiesced
}

/// This process's live guard for `(db_path, mode)`, if any.
///
/// Kept for the lifetime regressions only: production paths must use
/// [`claim_live_admission`], which cannot observe an admission it does not
/// own a claim on.
#[cfg(test)]
fn retained_guard(db_path: &Path, mode: WriterMode) -> Option<Arc<WorkspaceWriterGuard>> {
    let key = (canonical_db_path(db_path), mode);
    ACTIVE_GUARDS
        .lock()
        .ok()?
        .get(&key)
        .map(|entry| Arc::clone(&entry.guard))
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

async fn close_cooperative_pools(db_path: &Path) {
    let target = canonical_db_path(db_path);
    let pools: Vec<SqlitePool> = ACTIVE_POOLS
        .lock()
        .expect("pool registry poisoned")
        .get(&target)
        .map(|registered| registered.iter().map(|reg| reg.handle.clone()).collect())
        .unwrap_or_default();
    for pool in pools {
        pool.close().await;
    }
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

async fn pool_with_context(
    db_path: &Path,
    context: WriterConnectionContext,
    options: GuardedPoolOptions,
) -> Result<SqlitePool, LocalDbError> {
    let db_path = db_path.to_path_buf();
    let context_for_hook = context.clone();
    let url = sqlite_url(&db_path, false);
    let mut builder = SqlitePoolOptions::new().max_connections(options.max_connections);
    if let Some(timeout) = options.acquire_timeout {
        builder = builder.acquire_timeout(timeout);
    }
    builder
        .after_connect(move |conn, _meta| {
            let context = context_for_hook.clone();
            Box::pin(async move {
                install_writer_functions(conn, context).await?;
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
        .map_err(LocalDbError::from)
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
    let pool = pool_with_context(db_path, context.clone(), single_connection()).await?;
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
    let pool = pool_with_context(db_path, context.clone(), single_connection()).await?;
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
    let pool = pool_with_context(db_path, context.clone(), single_connection()).await?;
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
    let pool = pool_with_context(db_path, context.clone(), single_connection()).await?;
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
            migration_lock,
            engine_lock,
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
    let pool = pool_with_context(&guard.db_path, context, options).await?;
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
    let claim = reserve_retained_admission(&guard);
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
    let pool = pool_with_context(&guard.db_path, context, options).await?;
    let key = canonical_db_path(&guard.db_path);
    let reg_id = register_pool_handle(&guard, &pool);
    Ok(GuardedPool {
        guard,
        pool,
        _pool_registration: CooperativePoolRegistration { path: key, id: reg_id },
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
    close_cooperative_pools(db_path).await;
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
    let pool = pool_with_context(db_path, ctx.clone(), single_connection()).await?;
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
    let (guard, joined) = acquire_claimed_guard(db_path, creator_id, mode).await?;
    // A raw pool keeps its admission for the process: a joined claim is parked,
    // and a fresh guard gets the same process-lifetime retention.
    match joined {
        Some(claim) => claim.persist(),
        None => retain_writer_guard(Arc::clone(&guard)),
    }
    open_guarded_pool_with(&guard, options).await
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
    let claim = joined.unwrap_or_else(|| reserve_retained_admission(&guard));
    let guarded = open_guarded_pool_with_claim(guard, claim, options).await?;
    crate::seed_versions(guarded.pool()).await?;
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
    let claim = joined.unwrap_or_else(|| reserve_retained_admission(&guard));
    let guarded =
        open_guarded_pool_with_claim(guard, claim, GuardedPoolOptions::default()).await?;
    crate::seed_versions(guarded.pool()).await?;
    Ok(guarded)
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

    /// A holder whose lifetime ends by dropping the handle — the local
    /// provider store, any failed construction — gives its claim back, so the
    /// last one releases the entry and the OS admission with it.
    #[tokio::test]
    async fn dropped_handle_releases_the_last_admission() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        let engine =
            init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
                .await
                .unwrap();
        let direct = init_guarded_pool(&db_path, BOOTSTRAP_CREATOR_ID)
            .await
            .unwrap();
        // Exactly the provider store's shape: no explicit release call.
        drop(direct);
        drop(engine);

        // Both shared migration locks went with their last claims.
        let fence = acquire_store_reset_fence(&db_path)
            .expect("no writer admission survives the dropped holders");
        drop(fence);
    }

    /// A pool clone may outlive the handle that opened it: the admission claim
    /// ends with the handle, while migration quiescence keeps following the
    /// open pool until it is closed.
    #[tokio::test]
    async fn surviving_pool_clone_keeps_migration_quiescence() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");

        let guarded =
            init_engine_pool(&db_path, BOOTSTRAP_CREATOR_ID, GuardedPoolOptions::default())
                .await
                .unwrap();
        let survivor = guarded.clone_pool();
        drop(guarded);
        assert!(
            retained_guard(&db_path, WriterMode::Engine).is_none(),
            "the admission ends with its last claim"
        );
        assert!(
            !cooperative_pools_quiesced(&db_path),
            "the surviving clone is still an active cooperative pool"
        );

        survivor.close().await;
        assert!(cooperative_pools_quiesced(&db_path));
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
}
