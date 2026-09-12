//! Cross-process workspace writer protocol: OS locks, epochs, registration, guarded pools.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use nexus_storage_guard::{install_writer_functions, WriterConnectionContext, WriterMode as GuardMode};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::LocalDbError;

const LOCK_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
pub const BOOTSTRAP_CREATOR_ID: &str = "bootstrap";

/// Live admission guards held by THIS process, keyed by
/// `(canonical db path, mode)`.
///
/// Held so that already-published pools keep their admission (their
/// connections carry fixed UDF epochs and cannot re-register) while the OS
/// locks stay held for the process lifetime. Keyed per mode so retaining a
/// direct guard never releases the engine owner's lock.
static ACTIVE_GUARDS: LazyLock<Mutex<HashMap<(PathBuf, WriterMode), Arc<WorkspaceWriterGuard>>>> =
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
    creator_id: String,
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

    #[must_use]
    pub fn clone_pool(&self) -> SqlitePool {
        self.pool.clone()
    }
}

fn canonical_db_path(db_path: &Path) -> PathBuf {
    db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf())
}

pub fn retain_writer_guard(guard: Arc<WorkspaceWriterGuard>) {
    let key = (canonical_db_path(&guard.db_path), guard.mode);
    ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned")
        .insert(key, guard);
}

/// This process's live guard for `(db_path, mode)`, if any.
fn retained_guard(db_path: &Path, mode: WriterMode) -> Option<Arc<WorkspaceWriterGuard>> {
    let key = (canonical_db_path(db_path), mode);
    ACTIVE_GUARDS
        .lock()
        .ok()?
        .get(&key)
        .map(Arc::clone)
}

/// Drop one retained guard, releasing its OS admission locks.
fn release_guard(db_path: &Path, mode: WriterMode) {
    let key = (canonical_db_path(db_path), mode);
    ACTIVE_GUARDS
        .lock()
        .expect("writer guard registry poisoned")
        .remove(&key);
}

/// Drop this process's retained admission guards for `db_path`.
///
/// A migration needs the exclusive migration lock, which the same process's
/// retained shared guards would otherwise deadlock against. Dropping a guard
/// releases only the OS admission lock (`File`); writer registration rows stay
/// durable and already-published pools keep their installed scalar functions,
/// so they remain admitted while the epoch is unchanged and are fenced
/// fail-closed if the migration advances it.
fn release_retained_guards(db_path: &Path) {
    let target = canonical_db_path(db_path);
    let mut guards = ACTIVE_GUARDS.lock().expect("writer guard registry poisoned");
    guards.retain(|(path, _), _| path != &target);
}

fn migration_lock_path(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.migration.lock", db_path.display()))
}

fn engine_lock_path(db_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.engine.lock", db_path.display()))
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

async fn read_gate_epochs(db_path: &Path) -> Result<(i64, i64), LocalDbError> {
    // A not-yet-created database file cannot be opened `mode=ro` (SQLITE_CANTOPEN),
    // and it has no protocol state: treat absence as the pre-activation (0, 0) gate.
    if !db_path.exists() {
        return Ok((0, 0));
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
        return Ok((0, 0));
    }
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT migration_epoch, engine_epoch FROM core_workspace_gate WHERE pk = 1",
    )
    .fetch_optional(&pool)
    .await?;
    pool.close().await;
    Ok(row.unwrap_or((0, 0)))
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

    let (migration_epoch, _) = read_gate_epochs(db_path).await?;
    let migration_epoch = migration_epoch.max(1);

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
        creator_id: creator_id.to_string(),
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
    pool_with_context(&guard.db_path, context, options).await
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
    Ok(GuardedPool { guard, pool })
}

pub async fn run_guarded_migrations(db_path: &Path) -> Result<(), LocalDbError> {
    release_retained_guards(db_path);
    let _migration_lock = acquire_exclusive_lock(&migration_lock_path(db_path))?;
    let (current_epoch, _) = read_gate_epochs(db_path).await?;
    let writer_id = Uuid::new_v4().to_string();
    let workspace_identity = workspace_identity(db_path)?;
    let ctx = writer_context(&writer_id, WriterMode::Migration, current_epoch, None);
    let pool = pool_with_context(db_path, ctx.clone(), single_connection()).await?;
    // The exclusive OS migration lock is the primary serialization (two
    // co-booting processes cannot both migrate). The retry stays wired for the
    // residual transient classes it already classifies (SQLITE_BUSY from an
    // external writer, `_sqlx_migrations` bookkeeping collision).
    crate::run_migrations_with_retry(&pool, crate::run_migrations).await?;
    pool.close().await;
    if current_epoch == 0 {
        // Gate triggers require a migration writer registration before epoch bumps.
        register_writer(db_path, &ctx, BOOTSTRAP_CREATOR_ID, &workspace_identity).await?;
        let activated = activate_protocol_epoch(db_path, &ctx).await?;
        register_writer(
            db_path,
            &writer_context(&writer_id, WriterMode::Migration, activated, None),
            BOOTSTRAP_CREATOR_ID,
            &workspace_identity,
        )
        .await?;
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
    if !protocol_tables_present(db_path).await {
        run_guarded_migrations(db_path).await?;
    }
    let guard = acquire_writer_guard(db_path, creator_id, mode).await?;
    retain_writer_guard(Arc::clone(&guard));
    open_guarded_pool_with(&guard, options).await
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
