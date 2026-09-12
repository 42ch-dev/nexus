//! Cross-process workspace writer protocol: OS locks, epochs, registration, guarded pools.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use nexus_storage_guard::{install_writer_functions, WriterConnectionContext, WriterMode as GuardMode};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::LocalDbError;

const LOCK_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
const SUPPORTED_PROTOCOL_VERSION: i64 = 1;
pub const BOOTSTRAP_CREATOR_ID: &str = "bootstrap";

/// Live admission guards held by THIS process, keyed by
/// `(canonical db path, mode)`.
///
/// Held so that already-published pools keep their admission (their
/// connections carry fixed UDF epochs and cannot re-register) while the OS
/// locks stay held for the process lifetime. Keyed per mode so retaining a
/// direct guard never releases the engine owner's lock.
struct RegisteredPool {
    id: u64,
    handle: SqlitePool,
}

struct RetainedGuardEntry {
    guard: Arc<WorkspaceWriterGuard>,
    pools: Mutex<Vec<RegisteredPool>>,
}

impl RetainedGuardEntry {
    fn pools_quiesced(&self) -> bool {
        let mut pools = self.pools.lock().expect("pool registry poisoned");
        pools.retain(|reg| !reg.handle.is_closed());
        pools.is_empty()
    }
}

static POOL_REG_ID: AtomicU64 = AtomicU64::new(1);

struct CooperativePoolRegistration {
    key: (PathBuf, WriterMode),
    id: u64,
}

impl Drop for CooperativePoolRegistration {
    fn drop(&mut self) {
        unregister_pool_handle(&self.key, self.id);
    }
}

static ACTIVE_GUARDS: LazyLock<Mutex<HashMap<(PathBuf, WriterMode), RetainedGuardEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterMode {
    Direct,
    Engine,
    Migration,
}

impl WriterMode {
    fn to_guard(self) -> GuardMode {
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
            acquire_timeout: None,
        }
    }
}

pub struct GuardedPool {
    guard: Arc<WorkspaceWriterGuard>,
    pool: SqlitePool,
    _pool_registration: CooperativePoolRegistration,
}

impl GuardedPool {
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
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
}

fn canonical_db_path(db_path: &Path) -> PathBuf {
    if db_path.exists() {
        db_path.canonicalize().unwrap_or_else(|_| db_path.to_path_buf())
    } else {
        let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = db_path.file_name().map(PathBuf::from).unwrap_or_else(|| db_path.as_os_str().into());
        let canonical_parent = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
        canonical_parent.join(file_name)
    }
}

pub fn retain_writer_guard(guard: Arc<WorkspaceWriterGuard>) {
    let key = (canonical_db_path(&guard.db_path), guard.mode);
    ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned")
        .entry(key)
        .and_modify(|entry| entry.guard = Arc::clone(&guard))
        .or_insert_with(|| RetainedGuardEntry { guard, pools: Mutex::new(Vec::new()) });
}

fn register_pool_handle(guard: &Arc<WorkspaceWriterGuard>, pool: &SqlitePool) -> u64 {
    let id = POOL_REG_ID.fetch_add(1, Ordering::Relaxed);
    let key = (canonical_db_path(&guard.db_path), guard.mode);
    let mut guards = ACTIVE_GUARDS.lock().expect("writer guard registry poisoned");
    let entry = guards.entry(key).or_insert_with(|| RetainedGuardEntry {
        guard: Arc::clone(guard),
        pools: Mutex::new(Vec::new()),
    });
    entry
        .pools
        .lock()
        .expect("pool registry poisoned")
        .push(RegisteredPool {
            id,
            handle: pool.clone(),
        });
    id
}

fn unregister_pool_handle(key: &(PathBuf, WriterMode), id: u64) {
    let guards = ACTIVE_GUARDS.lock().expect("writer guard registry poisoned");
    if let Some(entry) = guards.get(key) {
        entry
            .pools
            .lock()
            .expect("pool registry poisoned")
            .retain(|reg| reg.id != id);
    }
}

/// This process's live guard for `(db_path, mode)`, if any.
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
    let mut guards = ACTIVE_GUARDS.lock().expect("writer guard registry poisoned");
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
        let quiesced = ACTIVE_GUARDS.lock().expect("writer guard registry poisoned").iter()
            .filter(|((path, _), _)| path == &target)
            .all(|(_, entry)| entry.pools_quiesced());
        if quiesced { return Ok(()); }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(LocalDbError::OwnerBusy { resource: format!("{}: cooperative pools still active", target.display()) })
}

fn migration_lock_path(db_path: &Path) -> PathBuf {
    let canonical = canonical_db_path(db_path);
    PathBuf::from(format!("{}.migration.lock", canonical.display()))
}

fn engine_lock_path(db_path: &Path) -> PathBuf {
    let canonical = canonical_db_path(db_path);
    PathBuf::from(format!("{}.engine.lock", canonical.display()))
}

fn workspace_identity(db_path: &Path) -> Result<String, LocalDbError> {
    let canonical = match db_path.canonicalize() {
        Ok(path) => path,
        Err(_) => db_path.to_path_buf(),
    };
    Ok(canonical.display().to_string())
}

fn open_lock_file(path: &Path) -> Result<File, LocalDbError> {
    OpenOptions::new()
        .create(true)
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
                sqlx::query("PRAGMA journal_mode = WAL").execute(&mut *conn).await?;
                sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await?;
                sqlx::query("PRAGMA busy_timeout = 2000").execute(&mut *conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .map_err(LocalDbError::from)
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateState { protocol_version: i64, migration_epoch: i64, engine_epoch: i64 }

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
        return Ok(GateState { protocol_version: 0, migration_epoch: 0, engine_epoch: 0 });
    }
    let url = sqlite_url(db_path, true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&url).await.map_err(LocalDbError::from)?;
    let present: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM sqlite_master WHERE type='table' AND name='core_workspace_gate'").fetch_optional(&pool).await?;
    if present.is_none() { pool.close().await; return Ok(GateState { protocol_version: 0, migration_epoch: 0, engine_epoch: 0 }); }
    let row: Option<(i64, i64, i64)> = sqlx::query_as("SELECT protocol_version, migration_epoch, engine_epoch FROM core_workspace_gate WHERE pk = 1").fetch_optional(&pool).await?;
    pool.close().await;
    Ok(row.map(|(protocol_version, migration_epoch, engine_epoch)| GateState { protocol_version, migration_epoch, engine_epoch }).unwrap_or(GateState { protocol_version: 0, migration_epoch: 0, engine_epoch: 0 }))
}

async fn needs_activation_recovery(db_path: &Path) -> Result<bool, LocalDbError> {
    if !protocol_tables_present(db_path).await { return Ok(false); }
    Ok(read_gate_state(db_path).await?.migration_epoch == 0)
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
    if let Ok(pool) = SqlitePoolOptions::new().max_connections(1).connect(&url).await {
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
    let current_epoch = current.map(|row| row.0).unwrap_or(0);
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


async fn advance_migration_epoch(db_path: &Path, context: &WriterConnectionContext) -> Result<i64, LocalDbError> {
    let pool = pool_with_context(db_path, context.clone(), single_connection()).await?;
    let mut tx = crate::begin_immediate(&pool).await?;
    let current: (i64,) = sqlx::query_as("SELECT migration_epoch FROM core_workspace_gate WHERE pk = 1").fetch_one(&mut *tx).await?;
    let next = current.0 + 1;
    let updated = sqlx::query("UPDATE core_workspace_gate SET migration_epoch = ? WHERE pk = 1 AND migration_epoch = ?").bind(next).bind(current.0).execute(&mut *tx).await?;
    if updated.rows_affected() == 0 { return Err(LocalDbError::OwnerBusy { resource: "migration_epoch".to_string() }); }
    tx.commit().await?; pool.close().await; Ok(next)
}

pub async fn acquire_writer_guard(
    db_path: &Path,
    creator_id: &str,
    mode: WriterMode,
) -> Result<Arc<WorkspaceWriterGuard>, LocalDbError> {
    // Reuse this process's live guard when it is still current: a re-opened
    // pool (the daemon's lazy creator-DB open, a restarted in-process test)
    // must neither deadlock on its own OS lock nor be refused as a second
    // owner. Cross-process contenders have no retained guard and still fail
    // closed. A stale epoch (a migration advanced the gate) releases first.
    if mode != WriterMode::Migration {
        if let Some(existing) = retained_guard(db_path, mode) {
            let (gate_migration, gate_engine) = read_gate_epochs(db_path).await?;
            let still_current = existing.migration_epoch == gate_migration.max(1)
                && existing.engine_epoch
                    == (mode == WriterMode::Engine).then_some(gate_engine);
            if still_current {
                return Ok(existing);
            }
            release_guard(db_path, mode);
        }
    }

    let workspace_identity = workspace_identity(db_path)?;
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

    Ok(Arc::new(WorkspaceWriterGuard {
        db_path: db_path.to_path_buf(),
        writer_id,
        mode,
        migration_epoch,
        engine_epoch,
        migration_lock,
        engine_lock,
    }))
}

/// One pooled connection: used by the protocol's own short transactions.
const fn single_connection() -> GuardedPoolOptions {
    GuardedPoolOptions {
        max_connections: 1,
        acquire_timeout: None,
    }
}

pub async fn open_guarded_pool(guard: &Arc<WorkspaceWriterGuard>) -> Result<SqlitePool, LocalDbError> {
    open_guarded_pool_with(guard, GuardedPoolOptions::default()).await
}

/// Open a guarded pool honouring caller sizing (the daemon's `PoolConfig`).
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

pub async fn open_guarded_pool_arc(guard: Arc<WorkspaceWriterGuard>) -> Result<GuardedPool, LocalDbError> {
    open_guarded_pool_arc_with(guard, GuardedPoolOptions::default()).await
}

/// Open a [`GuardedPool`] keeping the guard alive, honouring caller sizing.
pub async fn open_guarded_pool_arc_with(
    guard: Arc<WorkspaceWriterGuard>,
    options: GuardedPoolOptions,
) -> Result<GuardedPool, LocalDbError> {
    let context = writer_context(
        &guard.writer_id,
        guard.mode,
        guard.migration_epoch,
        guard.engine_epoch,
    );
    let pool = pool_with_context(&guard.db_path, context, options).await?;
    let key = (canonical_db_path(&guard.db_path), guard.mode);
    let reg_id = register_pool_handle(&guard, &pool);
    Ok(GuardedPool {
        guard,
        pool,
        _pool_registration: CooperativePoolRegistration { key, id: reg_id },
    })
}

pub async fn run_guarded_migrations(db_path: &Path) -> Result<(), LocalDbError> {
    let gate_before = read_gate_state(db_path).await?;
    validate_protocol_version(gate_before.protocol_version)?;
    await_cooperative_quiescence(db_path).await?;
    release_retained_guards(db_path);
    let _migration_lock = acquire_exclusive_lock(&migration_lock_path(db_path))?;
    let writer_id = Uuid::new_v4().to_string();
    let workspace_identity = workspace_identity(db_path)?;
    let ctx = writer_context(&writer_id, WriterMode::Migration, gate_before.migration_epoch, None);
    if protocol_tables_present(db_path).await {
        register_writer(db_path, &ctx, BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
    }
    let pool = pool_with_context(db_path, ctx.clone(), single_connection()).await?;
    let before_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&pool).await.unwrap_or(0);
    crate::run_migrations_with_retry(&pool, crate::run_migrations).await?;
    let after_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&pool).await.unwrap_or(before_count);
    pool.close().await;
    let gate_after = read_gate_state(db_path).await?;
    validate_protocol_version(gate_after.protocol_version)?;
    if gate_before.migration_epoch == 0 && gate_after.migration_epoch == 0 {
        register_writer(db_path, &ctx, BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
        let activated = activate_protocol_epoch(db_path, &ctx).await?;
        register_writer(db_path, &writer_context(&writer_id, WriterMode::Migration, activated, None), BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
    } else if after_count > before_count {
        let advanced = advance_migration_epoch(db_path, &writer_context(&writer_id, WriterMode::Migration, gate_after.migration_epoch, None)).await?;
        register_writer(db_path, &writer_context(&writer_id, WriterMode::Migration, advanced, None), BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
    } else if protocol_tables_present(db_path).await {
        register_writer(db_path, &writer_context(&writer_id, WriterMode::Migration, gate_after.migration_epoch, None), BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
    }
    Ok(())
}

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
    let guard = acquire_writer_guard(db_path, creator_id, mode).await?;
    retain_writer_guard(Arc::clone(&guard));
    open_guarded_pool_with(&guard, options).await
}

/// Open another cooperative pool on a live in-process engine guard when one
/// already exists (e.g. daemon `DbPool` + `CoreService` co-hosted in one process).
pub async fn join_live_engine_pool(
    db_path: &Path,
    options: GuardedPoolOptions,
) -> Result<Option<GuardedPool>, LocalDbError> {
    if let Some(guard) = retained_guard(db_path, WriterMode::Engine) {
        let guarded = open_guarded_pool_arc_with(guard, options).await?;
        return Ok(Some(guarded));
    }
    Ok(None)
}

/// Migrate, take the single engine ownership, and open a guarded pool.
///
/// This is the engine-owner entry point (the daemon's `DbPool`): the returned
/// guard holds `state.db.engine.lock` exclusively and persists the advanced
/// engine epoch, so no second process can become an effect owner.
pub async fn init_engine_pool(
    db_path: &Path,
    creator_id: &str,
    options: GuardedPoolOptions,
) -> Result<GuardedPool, LocalDbError> {
    run_guarded_migrations(db_path).await?;
    let guard = acquire_writer_guard(db_path, creator_id, WriterMode::Engine).await?;
    retain_writer_guard(Arc::clone(&guard));
    let guarded = open_guarded_pool_arc_with(guard, options).await?;
    crate::seed_versions(guarded.pool()).await?;
    Ok(guarded)
}

pub async fn init_guarded_pool(db_path: &Path, creator_id: &str) -> Result<GuardedPool, LocalDbError> {
    run_guarded_migrations(db_path).await?;
    let guard = acquire_writer_guard(db_path, creator_id, WriterMode::Direct).await?;
    retain_writer_guard(Arc::clone(&guard));
    let guarded = open_guarded_pool_arc(guard).await?;
    crate::seed_versions(guarded.pool()).await?;
    Ok(guarded)
}

impl WorkspaceWriterGuard {
    #[must_use]
    pub fn writer_id(&self) -> &str {
        &self.writer_id
    }

    #[must_use]
    pub fn mode(&self) -> WriterMode {
        self.mode
    }
}
