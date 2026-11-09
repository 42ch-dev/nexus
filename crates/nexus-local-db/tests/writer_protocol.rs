//! Cross-process writer protocol proof (architecture §4 / §12.1, proof-matrix DB-1/3/4).
//!
//! Real OS processes, deterministic file barriers, disposable databases. The
//! parent process drives; children re-enter this test binary with
//! `NEXUS_WRITER_PROTOCOL_CHILD=<mode>` set and run [`child_main`].
//!
//! Covered: trigger coverage on every persistent application table; pre-migration
//! fixture fenced after activation; committed pre-activation rows preserved;
//! 100 barrier-synchronised engine-owner races (exactly one winner per round,
//! engine epoch advanced exactly once per round); distinct direct writers both
//! succeed; §12.1 outbox (4097 direct mutations with no engine, bounded
//! oldest-row retention, foreign/raw writer fenced); DB-3 bound on engine
//! acquisition; in-process double acquisition refused; reopened committed state.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nexus_local_db::writer_protocol::{
    acquire_store_reset_fence, acquire_writer_guard, init_guarded_pool, join_live_engine_pool,
    open_admitted_pool, open_guarded_pool, open_guarded_pool_arc, release_retained_writer_guards,
    retain_writer_guard, run_guarded_migrations, GuardedPoolOptions, WriterMode,
    BOOTSTRAP_CREATOR_ID,
};
use nexus_local_db::{
    creator_holder_entry_id, ensure_creator_row, init_engine_pool, init_pool, open_pool_read_only,
    LocalDbError,
};
use nexus_storage_guard::{
    install_writer_functions, WriterConnectionContext, WriterMode as GuardWriterMode,
};
use sqlx::{Connection as _, SqliteConnection, SqlitePool};
use tempfile::TempDir;

const CHILD_ENV: &str = "NEXUS_WRITER_PROTOCOL_CHILD";
/// DB-3 / proof-matrix §2: engine and migration acquisition must stay bounded.
const ACQUISITION_BOUND: Duration = Duration::from_secs(5);
/// Tables owned by SQLite internals or by the migration runner itself.
const UNGUARDED_TABLES: [&str; 2] = ["_sqlx_migrations", "sqlite_sequence"];

fn exe() -> PathBuf {
    std::env::current_exe().expect("current exe")
}

fn barrier(path: &Path) {
    fs::write(path, b"1").expect("barrier write");
}

fn wait_barrier(path: &Path, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("barrier timeout: {}", path.display());
}

fn spawn_child(mode: &str, db: &Path, extra: &[(&str, String)], err_path: &Path) -> Child {
    let err = fs::File::create(err_path).expect("child stderr file");
    let mut cmd = Command::new(exe());
    cmd.args([
        "writer_protocol",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(CHILD_ENV, mode)
    .env("DB_PATH", db.to_string_lossy().to_string())
    .stdout(Stdio::null())
    .stderr(Stdio::from(err));
    for (key, value) in extra {
        cmd.env(key, value);
    }
    cmd.spawn().expect("spawn child")
}

/// Reap a child, surfacing its captured stderr when it exited unsuccessfully.
fn reap(child: &mut Child, err_path: &Path) {
    let status = child.wait().expect("child wait");
    if !status.success() {
        let captured = fs::read_to_string(err_path).unwrap_or_default();
        panic!("child exited {status}:\n{captured}");
    }
}

/// Run one `migration_fence` / `engine_solo` child and return its outcome.
fn child_outcome(dir: &Path, db: &Path, mode: &str, tag: &str) -> String {
    let out = dir.join(format!("{tag}_out"));
    let err = dir.join(format!("{tag}_err"));
    let mut child = spawn_child(
        mode,
        db,
        &[("OUT_PATH", out.to_string_lossy().to_string())],
        &err,
    );
    reap(&mut child, &err);
    fs::read_to_string(out).expect("child outcome")
}

/// Run one engine-owner race round between two real processes.
fn engine_race_round(dir: &Path, db: &Path, round: usize) {
    let mut outcomes = Vec::with_capacity(2);
    let mut children = Vec::with_capacity(2);
    for (self_idx, peer_idx) in [(0usize, 1usize), (1usize, 0usize)] {
        let self_barrier = dir.join(format!("r{round}_self{self_idx}"));
        let peer_barrier = dir.join(format!("r{round}_self{peer_idx}"));
        let out = dir.join(format!("r{round}_out{self_idx}"));
        let err = dir.join(format!("r{round}_err{self_idx}"));
        let child = spawn_child(
            "engine_race",
            db,
            &[
                ("BARRIER_SELF", self_barrier.to_string_lossy().to_string()),
                ("BARRIER_PEER", peer_barrier.to_string_lossy().to_string()),
                ("OUT_PATH", out.to_string_lossy().to_string()),
            ],
            &err,
        );
        children.push((child, err));
        outcomes.push(out);
    }
    // Both contenders have passed their own barrier before either acquires.
    for index in 0..2 {
        wait_barrier(
            &dir.join(format!("r{round}_self{index}")),
            Duration::from_secs(30),
        );
    }
    for (child, err) in &mut children {
        reap(child, err);
    }
    let results: Vec<String> = outcomes
        .iter()
        .map(|path| {
            fs::read_to_string(path)
                .expect("child outcome")
                .trim()
                .to_string()
        })
        .collect();
    assert_eq!(
        results.iter().filter(|outcome| *outcome == "won").count(),
        1,
        "round {round}: exactly one engine owner, got {results:?}"
    );
    assert_eq!(
        results.iter().filter(|outcome| *outcome == "busy").count(),
        1,
        "round {round}: the loser must be refused, got {results:?}"
    );
}

async fn engine_epoch(db: &Path) -> i64 {
    let pool = open_pool_read_only(db).await.expect("read-only pool");
    let row: (i64,) = sqlx::query_as("SELECT engine_epoch FROM core_workspace_gate WHERE pk = 1")
        .fetch_one(&pool)
        .await
        .expect("gate row");
    pool.close().await;
    row.0
}

/// Assert a connection cannot mutate a guarded table.
///
/// Two fence mechanisms are both correct and both observed here (§4.2):
/// - a connection without the protocol scalar functions is rejected by SQLite
///   itself (`no such function: nexus_writer_protocol`), which is what closes
///   the pre-opened / raw `sqlite3` writer loophole;
/// - a connection that carries the functions but no matching registration
///   epoch raises the generated `WRITER_FENCED` abort.
async fn assert_fenced(pool: &SqlitePool, sql: &'static str) {
    let err = sqlx::query(sql)
        .execute(pool)
        .await
        .expect_err("unregistered writer must be fenced");
    let message = err.to_string();
    assert!(
        message.contains("WRITER_FENCED") || message.contains("no such function: nexus_writer"),
        "expected a writer fence, got: {message}"
    );
}

/// Every persistent application table carries INSERT/UPDATE/DELETE guards.
async fn assert_trigger_coverage(pool: &SqlitePool) {
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("table list");
    let triggers: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='trigger' ORDER BY name")
            .fetch_all(pool)
            .await
            .expect("trigger list");
    let trigger_names: Vec<&str> = triggers.iter().map(|t| t.0.as_str()).collect();

    let mut missing = Vec::new();
    for (table,) in &tables {
        if UNGUARDED_TABLES.contains(&table.as_str()) {
            continue;
        }
        for op in ["insert", "update", "delete"] {
            let expected = format!("guard_{table}_{op}");
            if !trigger_names.contains(&expected.as_str()) {
                missing.push(expected);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "persistent application tables without guards: {missing:?}"
    );
    assert!(
        tables.len() >= 51,
        "expected the full persistent table set, saw {}",
        tables.len()
    );
}

#[tokio::test]
async fn writer_protocol() {
    if std::env::var_os(CHILD_ENV).is_some() {
        child_main().await;
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");

    pre_migration_fixture_fenced(dir.path()).await;
    bootstrap_main_database(&db).await;
    engine_owner_races(dir.path(), &db).await;
    distinct_direct_writers_succeed(&db).await;
    engine_owned_table_fences_direct(&db).await;
    outbox_mutations_bounded(&db).await;
    newer_protocol_refused(dir.path()).await;
    raw_writer_fenced_on_outbox(&db).await;
    engine_ownership_fail_fast(dir.path(), &db).await;
    legacy_reopen_still_fenced(&db).await;
    reopen_preserves_committed_mutations(&db).await;
    revision_bump_not_doubled(dir.path()).await;
    init_pool_quiescence(dir.path()).await;
    clone_pool_blocks_pending_migration(dir.path()).await;
    retention_keeps_newest_row(dir.path()).await;
    retention_bounds_hold_after_burst(dir.path()).await;
    pre_activation_state_preserved(dir.path()).await;
}

/// DB-4 (pre-migration fixture): a connection opened before the protocol
/// migration is fenced after activation.
async fn pre_migration_fixture_fenced(dir: &Path) {
    let legacy_db = dir.join("legacy.db");
    let url = format!("sqlite://{}?mode=rwc", legacy_db.display());
    let legacy = SqlitePool::connect(&url).await.expect("legacy connect");
    // Pre-migration schema bootstrap only (the workspace_meta table exists
    // from the initial migration; run the full set, then insert).
    run_guarded_migrations(&legacy_db)
        .await
        .expect("activate protocol");
    // Row committed by a pre-activation writer on the same file: emulate by
    // writing through a registered writer, then prove the *legacy* handle
    // (no protocol scalar functions) is fenced.
    let guard = acquire_writer_guard(&legacy_db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("direct guard");
    let pool = open_guarded_pool(&guard).await.expect("guarded pool");
    sqlx::query("INSERT INTO workspace_meta (key, value) VALUES ('pre_activation', 'kept')")
        .execute(&pool)
        .await
        .expect("registered write");
    pool.close().await;
    drop(guard);
    assert_fenced(
        &legacy,
        "INSERT INTO workspace_meta (key, value) VALUES ('probe', 'x')",
    )
    .await;
    legacy.close().await;
}

/// Bootstrap the main database once and verify trigger coverage on a fresh
/// schema.
async fn bootstrap_main_database(db: &Path) {
    let bootstrap = init_pool(db).await.expect("init_pool");
    bootstrap.close().await;

    let pool = open_pool_read_only(db).await.expect("read-only pool");
    assert_trigger_coverage(&pool).await;
    pool.close().await;
}

/// DB-1: 100 barrier-synchronised engine-owner races. Exactly one winner per
/// round and exactly one engine-epoch advance per round (no lost/duplicated
/// ownership), driven by two real OS processes.
async fn engine_owner_races(dir: &Path, db: &Path) {
    for round in 0..100 {
        engine_race_round(dir, db, round);
        let epoch = engine_epoch(db).await;
        assert_eq!(
            epoch,
            i64::try_from(round).unwrap() + 1,
            "round {round}: engine epoch must advance exactly once"
        );
    }
}

/// Distinct entities: two direct writers both succeed with no engine running.
async fn distinct_direct_writers_succeed(db: &Path) {
    for (key, value) in [("direct_a", "ok"), ("direct_b", "ok")] {
        let pool = open_admitted_pool(db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
            .await
            .expect("direct pool");
        sqlx::query("INSERT INTO workspace_meta (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .expect("direct write");
        pool.close().await;
    }
}

/// Direct vs engine authorization on an engine-owned table: the direct
/// writer is fenced, the engine owner writes successfully.
async fn engine_owned_table_fences_direct(db: &Path) {
    let direct = open_admitted_pool(db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("direct pool");
    assert_fenced(
        &direct,
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id)              VALUES ('xj_direct_denied', 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
    )
    .await;
    direct.close().await;

    let engine = init_engine_pool(db).await.expect("engine pool");
    sqlx::query(
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id)              VALUES ('xj_engine_ok', 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
    )
    .execute(engine.pool())
    .await
    .expect("engine-owned write");
    engine.pool().close().await;
    drop(engine);
    release_retained_writer_guards(db);
}

/// §12.1 outbox via real application mutations (no engine owner): a
/// rolled-back mutation emits nothing, committed mutations advance the
/// sequence one-for-one, and bounded retention keeps the newest 4096 rows.
async fn outbox_mutations_bounded(db: &Path) {
    let pool = open_admitted_pool(db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("direct pool");
    let (before_count, max_before): (i64, i64) =
        sqlx::query_as("SELECT COUNT(*), COALESCE(MAX(sequence), 0) FROM core_changes")
            .fetch_one(&pool)
            .await
            .expect("baseline outbox");

    let mut tx = pool.begin().await.expect("tx");
    sqlx::query("INSERT INTO workspace_meta (key, value) VALUES ('rollback_probe', 'x')")
        .execute(&mut *tx)
        .await
        .expect("tx insert");
    tx.rollback().await.expect("rollback");
    let (mid_count, max_mid): (i64, i64) =
        sqlx::query_as("SELECT COUNT(*), COALESCE(MAX(sequence), 0) FROM core_changes")
            .fetch_one(&pool)
            .await
            .expect("outbox after rollback");
    assert_eq!(
        mid_count, before_count,
        "rolled-back mutation must not emit an outbox row"
    );
    assert_eq!(
        max_mid, max_before,
        "rolled-back mutation must not advance sequence"
    );

    for index in 0..4097 {
        sqlx::query("INSERT INTO workspace_meta (key, value) VALUES (?, ?)")
            .bind(format!("outbox_{index}"))
            .bind("evt")
            .execute(&pool)
            .await
            .expect("application mutation");
    }
    let (after_count, max_after, min_seq): (i64, i64, i64) =
        sqlx::query_as("SELECT COUNT(*), MAX(sequence), MIN(sequence) FROM core_changes")
            .fetch_one(&pool)
            .await
            .expect("outbox after mutations");
    assert_eq!(
        max_after - max_before,
        4097,
        "one outbox event per committed mutation (sequence advances even when retention trims rows)"
    );
    assert_eq!(
        after_count, 4096,
        "automatic bounded retention keeps the newest 4096 rows"
    );
    assert!(
        min_seq > 1,
        "oldest committed events are trimmed automatically"
    );
    pool.close().await;
}

/// A workspace whose gate claims a protocol newer than this binary is
/// fail-closed (isolated DB; gate is guarded).
async fn newer_protocol_refused(dir: &Path) {
    let proto_db = dir.join("proto_newer.db");
    let pool = init_pool(&proto_db).await.expect("proto init");
    pool.close().await;
    let engine = init_engine_pool(&proto_db)
        .await
        .expect("engine for gate bump");
    sqlx::query("UPDATE core_workspace_gate SET protocol_version = 2 WHERE pk = 1")
        .execute(engine.pool())
        .await
        .expect("bump protocol");
    engine.pool().close().await;
    drop(engine);
    release_retained_writer_guards(&proto_db);
    let err = init_pool(&proto_db)
        .await
        .expect_err("newer protocol refused");
    assert!(
        matches!(err, LocalDbError::SchemaMismatch { .. }),
        "got {err:?}"
    );
}

/// A raw (unregistered) writer is fenced on the outbox too.
async fn raw_writer_fenced_on_outbox(db: &Path) {
    let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
        .await
        .expect("raw pool");
    assert_fenced(&raw, "DELETE FROM core_changes WHERE sequence = 2").await;
    raw.close().await;
}

/// DB-3: engine ownership is fail-fast while an owner is live. A live owner
/// is never replaced, a refusal stays inside the acquisition bound, a
/// refused acquisition does not advance the epoch, a child process cannot
/// steal the lock, and the successor after the owner exits advances the
/// epoch exactly once.
async fn engine_ownership_fail_fast(dir: &Path, db: &Path) {
    let owner = acquire_writer_guard(db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine)
        .await
        .expect("engine owner");
    let started = Instant::now();
    let acquisition = acquire_writer_guard(db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine).await;
    let Err(err) = acquisition else {
        panic!("a live engine owner must not be replaced");
    };
    let elapsed = started.elapsed();
    assert!(matches!(err, LocalDbError::OwnerBusy { .. }), "got {err:?}");
    assert!(
        elapsed <= ACQUISITION_BOUND,
        "engine acquisition must stay bounded, took {elapsed:?}"
    );

    // In-process double acquisition is refused: this process already owns
    // the engine lock, and engine ownership is not a stealable lease. (The
    // pool factories differ deliberately: a repeat request there resolves
    // to the one live owner instead of taking a second lock, which is what
    // lets the daemon re-open its creator DB; see the direct-writer loop
    // above.) A refusal must not advance the epoch.
    let epoch_with_owner = engine_epoch(db).await;
    let acquisition = acquire_writer_guard(db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine).await;
    let Err(err) = acquisition else {
        panic!("a second in-process engine acquisition must be refused");
    };
    assert!(matches!(err, LocalDbError::OwnerBusy { .. }), "got {err:?}");
    assert_eq!(
        engine_epoch(db).await,
        epoch_with_owner,
        "a refused acquisition must not advance the engine epoch"
    );

    // Cross-process: a child cannot steal the live owner's engine either.
    let err_path = dir.join("err_owner");
    let owner_out = dir.join("owner_out");
    let mut child = spawn_child(
        "engine_solo",
        db,
        &[("OUT_PATH", owner_out.to_string_lossy().to_string())],
        &err_path,
    );
    reap(&mut child, &err_path);
    assert_eq!(
        fs::read_to_string(dir.join("owner_out")).expect("owner out"),
        "busy"
    );

    // Ownership transfer across restart: once the owner is gone the next
    // engine takes over and advances the epoch exactly once.
    drop(owner);
    let successor = acquire_writer_guard(db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine)
        .await
        .expect("engine ownership transfers after the predecessor exits");
    assert_eq!(
        engine_epoch(db).await,
        epoch_with_owner + 1,
        "the successor owner advances the engine epoch exactly once"
    );
    drop(successor);
}

/// A reopened raw legacy connection is still fenced on guarded tables.
async fn legacy_reopen_still_fenced(db: &Path) {
    let legacy = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
        .await
        .expect("legacy reopen");
    assert_fenced(
        &legacy,
        "UPDATE workspace_meta SET value = 'bad' WHERE key = 'direct_a'",
    )
    .await;
    legacy.close().await;
}

/// Re-open preserves committed mutations exactly once.
async fn reopen_preserves_committed_mutations(db: &Path) {
    let pool = init_pool(db).await.expect("reopen");
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value FROM workspace_meta WHERE key IN ('direct_a', 'direct_b') ORDER BY key",
    )
    .fetch_all(&pool)
    .await
    .expect("committed rows");
    assert_eq!(
        rows,
        vec![
            ("direct_a".to_string(), "ok".to_string()),
            ("direct_b".to_string(), "ok".to_string()),
        ]
    );
    let (events,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM core_changes")
        .fetch_one(&pool)
        .await
        .expect("outbox rows");
    assert_eq!(events, 4096, "committed events appear exactly once");
    pool.close().await;
}

/// Revision bump: a legacy non-OCC write bumps `revision` exactly once; an
/// explicit CAS write is not doubled by the trigger.
async fn revision_bump_not_doubled(dir: &Path) {
    let rev_db = dir.join("revision.db");
    let pool = init_pool(&rev_db).await.expect("revision init");
    ensure_creator_row(&pool, "ctr_test", "Test")
        .await
        .expect("creator row");
    sqlx::query(
        "INSERT INTO narrative_worlds (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json, created_at)              VALUES ('wld_rev', 'ws', 'ctr_test', 'Rev', 'rev', 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(&pool)
    .await
    .expect("seed world");
    sqlx::query(
        "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, body_json, revision, created_at)              VALUES ('kb_rev_probe', 'wld_rev', 'info_point', 'probe', 'confirmed', '{}', 0, datetime('now'))",
    )
    .execute(&pool)
    .await
    .expect("seed kb row");
    sqlx::query("UPDATE kb_key_blocks SET body_json = ?1 WHERE key_block_id = 'kb_rev_probe'")
        .bind(r#"{"x":1}"#)
        .execute(&pool)
        .await
        .expect("legacy non-OCC update");
    let (legacy_rev,): (i64,) =
        sqlx::query_as("SELECT revision FROM kb_key_blocks WHERE key_block_id = 'kb_rev_probe'")
            .fetch_one(&pool)
            .await
            .expect("legacy revision");
    assert_eq!(
        legacy_rev, 1,
        "non-OCC update must bump revision exactly once"
    );

    sqlx::query(
        "UPDATE kb_key_blocks SET body_json = ?1, revision = 2 WHERE key_block_id = 'kb_rev_probe' AND revision = 1",
    )
    .bind(r#"{"x":2}"#)
    .execute(&pool)
    .await
    .expect("explicit CAS bump");
    let (cas_rev,): (i64,) =
        sqlx::query_as("SELECT revision FROM kb_key_blocks WHERE key_block_id = 'kb_rev_probe'")
            .fetch_one(&pool)
            .await
            .expect("cas revision");
    assert_eq!(
        cas_rev, 2,
        "explicit CAS revision must not be doubled by the trigger"
    );
    pool.close().await;
    release_retained_writer_guards(&rev_db);
}

/// Quiescence: a no-op run must not wait for a live cooperative pool, while
/// a genuinely pending migration must wait until it closes.
async fn init_pool_quiescence(dir: &Path) {
    let init_db = dir.join("init_pool_quiescence.db");
    let pool = init_pool(&init_db).await.expect("init_pool");

    // Nothing pending: the schema is current, so the guarded path has no
    // DDL to serialize and must return without waiting for the live pool.
    run_guarded_migrations(&init_db)
        .await
        .expect("a current schema must not wait for cooperative quiescence");

    // A genuinely pending migration still waits for the pool to close.
    make_migration_pending(&pool).await;
    let migrate_db = init_db.clone();
    let migrate = tokio::spawn(async move { run_guarded_migrations(&migrate_db).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !migrate.is_finished(),
        "migration must wait while an init_pool-returned pool remains open"
    );
    pool.close().await;
    release_retained_writer_guards(&init_db);
    migrate
        .await
        .expect("migrate task")
        .expect("migration after init_pool pool closed");
}

/// A surviving `clone_pool` handle blocks a pending migration until closed.
async fn clone_pool_blocks_pending_migration(dir: &Path) {
    let quiet_db = dir.join("quiescence.db");
    let guarded = init_engine_pool(&quiet_db)
        .await
        .expect("engine guarded pool");
    let survivor = guarded.clone_pool();
    drop(guarded);
    // Pending work is what makes the wait meaningful; a no-op run fast-paths.
    make_migration_pending(&survivor).await;
    let migrate_db = quiet_db.clone();
    let migrate = tokio::spawn(async move { run_guarded_migrations(&migrate_db).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !migrate.is_finished(),
        "migration must wait while a cooperative pool clone remains open"
    );
    survivor.close().await;
    release_retained_writer_guards(&quiet_db);
    migrate
        .await
        .expect("migrate task")
        .expect("migration after quiescence");
}

/// Retention: the newest outbox row survives a single >8MiB event (F-9).
async fn retention_keeps_newest_row(dir: &Path) {
    let newest_db = dir.join("retention_newest_row.db");
    let pool = open_admitted_pool(&newest_db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("direct pool for newest-row retention");
    let giant = "x".repeat(9 * 1024 * 1024);
    sqlx::query("INSERT INTO workspace_meta (key, value) VALUES ('huge_outbox', ?)")
        .bind(&giant)
        .execute(&pool)
        .await
        .expect("single large mutation");
    let (count, max_seq): (i64, i64) =
        sqlx::query_as("SELECT COUNT(*), COALESCE(MAX(sequence), 0) FROM core_changes")
            .fetch_one(&pool)
            .await
            .expect("outbox after huge insert");
    assert_eq!(count, 1, "newest row must survive byte-budget retention");
    assert!(max_seq >= 1, "sequence must advance for the inserted row");
    pool.close().await;
    release_retained_writer_guards(&newest_db);
}

/// Retention burst: row and byte bounds hold after rapid inserts.
async fn retention_bounds_hold_after_burst(dir: &Path) {
    let burst_db = dir.join("retention_burst.db");
    let pool = init_pool(&burst_db).await.expect("burst init");
    for index in 0..5000 {
        let payload = "x".repeat(2048);
        sqlx::query("INSERT INTO workspace_meta (key, value) VALUES (?, ?)")
            .bind(format!("burst_{index}"))
            .bind(payload)
            .execute(&pool)
            .await
            .expect("burst insert");
    }
    let (count, bytes): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(length(world_id) + length(resource_kind) + length(resource_id)              + COALESCE(length(resource_revision), 0) + length(change_kind) + length(writer_id)), 0)              FROM core_changes",
    )
        .fetch_one(&pool)
        .await
        .expect("outbox bounds");
    assert!(count <= 4096, "row retention bound: got {count}");
    assert!(bytes <= 8_388_608, "byte retention bound: got {bytes}");
    pool.close().await;
    release_retained_writer_guards(&burst_db);
}

/// The pre-activation row committed on the fixture database survived the
/// whole suite.
async fn pre_activation_state_preserved(dir: &Path) {
    let legacy_db = dir.join("legacy.db");
    let pool = open_pool_read_only(&legacy_db)
        .await
        .expect("fixture read-only");
    let row: (String,) =
        sqlx::query_as("SELECT value FROM workspace_meta WHERE key = 'pre_activation'")
            .fetch_one(&pool)
            .await
            .expect("pre-activation row survives");
    assert_eq!(row.0, "kept");
    pool.close().await;
}

/// Version of the newest migration whose script is safe to re-apply: the
/// JS-provider journal is `CREATE ... IF NOT EXISTS` throughout.
///
/// The v1.191 holder cutover is deliberately **not** re-appliable — its
/// preflight refuses a pre-existing registry — so a helper that dropped "the
/// newest applied migration" would try to re-run a migration whose whole point
/// is to run exactly once. The pending state these quiescence and epoch tests
/// need is therefore produced by dropping this pinned idempotent migration,
/// which leaves the applied set genuinely incomplete.
const IDEMPOTENT_MIGRATION_VERSION: i64 = 20_260_914_000_001;

/// Force a genuinely pending migration by dropping the newest *idempotent*
/// applied `_sqlx_migrations` row — the state a binary whose migration set is
/// ahead of the file would find.
async fn make_migration_pending(pool: &SqlitePool) {
    let dropped = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
        .bind(IDEMPOTENT_MIGRATION_VERSION)
        .execute(pool)
        .await
        .expect("drop the pinned idempotent applied migration row")
        .rows_affected();
    assert_eq!(dropped, 1, "the pinned migration must be applied");
}

/// v1.191 P1 T3: the two holder-cutover tables are writer-guarded like every
/// other persistent application table, and a writer admitted *before* the
/// cutover epoch is refused after the migration epoch advances.
///
/// Complements [`v1191_sqlite_writer`] (which proves the registry/refusal
/// contract on `workspace_meta`): every mutation of `knowledge_holders` and
/// `knowledge_import_quarantine` requires admission, and the epoch staleness
/// that refuses an old binary/writer applies to them too.
#[tokio::test]
async fn v1191_holder_writer() {
    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("v1191_holder_writer.db");

    let bootstrap = init_pool(&db).await.expect("init_pool");
    bootstrap.close().await;
    release_retained_writer_guards(&db);

    assert_holder_tables_fenced_for_raw_writer(&db).await;
    assert_holder_tables_require_admission(&db).await;
    assert_holder_table_trigger_coverage(&db).await;
    // The admission refusal is exercised last: it leaves a deliberately
    // unknown applied migration in the workspace.
    assert_unknown_applied_migration_refused(&db).await;

    release_retained_writer_guards(&db);
}

/// Remote dependency metadata rejects every mutation from an unregistered writer.
#[tokio::test]
async fn remote_dependency_tables_fence_raw_writers() {
    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("remote_dependencies.db");
    let bootstrap = init_pool(&db).await.expect("init_pool");
    bootstrap.close().await;
    release_retained_writer_guards(&db);

    let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
        .await
        .expect("raw connect");
    for sql in [
        "INSERT INTO remote_binding_memory_admissions \
         (admission_id, owner_creator_id, character_id, binding_id, authority_host_id, \
          authority_generation, space_id, client_identity, created_at) \
         VALUES ('adm_raw', 'ctr_raw', 'chr_raw', 'bnd_raw', 'host', '1', 'space', 'client', 'now')",
        "UPDATE remote_binding_memory_admissions SET created_at = 'changed'",
        "DELETE FROM remote_binding_memory_admissions",
        "INSERT INTO remote_binding_memory_objects \
         (authority_host_id, authority_generation, space_id, client_identity, character_id, \
          binding_id, object_kind, object_id, state, created_at, updated_at) \
         VALUES ('host', '1', 'space', 'client', 'chr_raw', 'bnd_raw', \
                 'candidate', 'cand_raw', 'live', 'now', 'now')",
        "UPDATE remote_binding_memory_objects SET state = 'released'",
        "DELETE FROM remote_binding_memory_objects",
    ] {
        assert_fenced(&raw, sql).await;
    }
    raw.close().await;
}

/// Every mutation of the new tables is fenced on a handle that never received
/// the protocol scalars — the raw/pre-opened writer loophole.
async fn assert_holder_tables_fenced_for_raw_writer(db: &Path) {
    let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
        .await
        .expect("raw connect");
    for sql in [
        "INSERT INTO knowledge_holders (holder_entry_id, creator_id) VALUES ('hld_raw', 'ctr_raw')",
        "UPDATE knowledge_holders SET created_at = 'x'",
        "DELETE FROM knowledge_holders",
        "INSERT INTO knowledge_import_quarantine \
         (quarantine_id, import_batch_id, controlling_creator_id, owner_kind, world_id, \
          quarantine_reason, original_entry_json, source_provenance_json) \
         VALUES ('q1', 'b1', 'ctr_raw', 'world', 'wld_x', 'unresolved_holder', '{}', '{}')",
        "UPDATE knowledge_import_quarantine SET quarantine_reason = 'unknown_disclosure'",
        "DELETE FROM knowledge_import_quarantine",
    ] {
        assert_fenced(&raw, sql).await;
    }
    raw.close().await;
}

/// A registered direct writer is admitted to both tables, and the admitted rows
/// read back.
async fn assert_holder_tables_require_admission(db: &Path) {
    let direct = open_admitted_pool(db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("direct admitted pool");
    // The subject row alone: the holder is registered by the explicit insert
    // below, which is what proves the guard admits a direct writer on the
    // registry table. `ensure_creator_row` commits the holder together with the
    // subject (§2.1) and would collide with that insert.
    // SAFETY: fixture insert against the known `creators` DDL.
    sqlx::query(
        "INSERT INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_holder_writer', 'Holder Writer', 'active', '2026-09-18T00:00:00Z', '{}')",
    )
    .execute(&direct)
    .await
    .expect("creator subject row");
    // The quarantine container is a real stored World (FK), not a placeholder.
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_holder_writer', 'ws', 'ctr_holder_writer', 'Holder World', \
                 'holder-world', 'active', 'private', 'manual', '{}', \
                 '2026-09-18T00:00:00Z')",
    )
    .execute(&direct)
    .await
    .expect("world container for the quarantined atom");
    let holder = creator_holder_entry_id("ctr_holder_writer");
    sqlx::query("INSERT INTO knowledge_holders (holder_entry_id, creator_id) VALUES (?, ?)")
        .bind(&holder)
        .bind("ctr_holder_writer")
        .execute(&direct)
        .await
        .expect("an admitted direct writer registers the holder");
    sqlx::query(
        "INSERT INTO knowledge_import_quarantine \
         (quarantine_id, import_batch_id, controlling_creator_id, owner_kind, world_id, \
          quarantine_reason, original_entry_json, source_provenance_json) \
         VALUES ('qrn_admitted', 'batch_admitted', 'ctr_holder_writer', 'world', \
                 'wld_holder_writer', 'unresolved_holder', \
                 '{\"entry_id\":\"kb_foreign\"}', '{\"pack\":\"p1\"}')",
    )
    .execute(&direct)
    .await
    .expect("an admitted direct writer quarantines a foreign atom");

    let stored: (String, Option<String>) =
        sqlx::query_as("SELECT holder_entry_id, creator_id FROM knowledge_holders")
            .fetch_one(&direct)
            .await
            .expect("holding row");
    assert_eq!(stored, (holder, Some("ctr_holder_writer".to_string())));
    let quarantined: (String, String) =
        sqlx::query_as("SELECT quarantine_id, quarantine_reason FROM knowledge_import_quarantine")
            .fetch_one(&direct)
            .await
            .expect("quarantine row");
    assert_eq!(
        quarantined,
        ("qrn_admitted".to_string(), "unresolved_holder".to_string())
    );
    direct.close().await;
}

/// Both tables carry INSERT/UPDATE/DELETE guards, so the generic coverage
/// assertion (every persistent application table) holds with them added.
async fn assert_holder_table_trigger_coverage(db: &Path) {
    let pool = open_pool_read_only(db).await.expect("read-only pool");
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' \
         AND name IN ('knowledge_holders', 'knowledge_import_quarantine') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("holder table list");
    assert_eq!(
        tables,
        vec![
            "knowledge_holders".to_string(),
            "knowledge_import_quarantine".to_string()
        ],
        "the cutover tables exist"
    );
    assert_trigger_coverage(&pool).await;
    pool.close().await;
}

/// A writer older than the cutover is refused by **strict migration
/// admission**: when the workspace carries an applied migration the binary's
/// source set does not know — exactly what the holder cutover looks like to a
/// pre-v1.191 binary — the runner refuses instead of serving a partially
/// understood schema.
///
/// The epoch-staleness arm of the guards is not reachable in a conforming
/// system: every live direct/engine writer holds the shared migration lock, so
/// `run_guarded_migrations` refuses (`OwnerBusy`) before an epoch can advance
/// under a live writer — which is precisely the quiescence invariant.
async fn assert_unknown_applied_migration_refused(db: &Path) {
    let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
        .await
        .expect("raw connect");
    sqlx::query(
        "INSERT INTO _sqlx_migrations \
         (version, description, success, checksum, execution_time) \
         VALUES (?, 'future holder cutover', TRUE, x'00', 0)",
    )
    .bind(nexus_local_db::HOLDER_MIGRATION_VERSION + 1)
    .execute(&raw)
    .await
    .expect("simulate a workspace migrated by a newer binary");
    raw.close().await;

    let err = run_guarded_migrations(db)
        .await
        .expect_err("an unknown applied migration must be refused");
    assert!(
        err.to_string().contains("previously applied"),
        "expected strict admission to name the unknown applied version: {err}"
    );
}

/// v1.189 P1 fix round 1 (proof-matrix DB-1): the guarded-migration fast path.
///
/// A workspace whose schema is already current has nothing for the exclusive
/// migration lock to serialize, so a second in-process opener must not be
/// forced through cooperative quiescence. That refusal was exactly what broke
/// the daemon's boot-pool + engine-owner coexistence (World KB handlers 500'd
/// with `cooperative pools still active`). A genuinely pending migration must
/// still take the guarded path and refuse while a cooperative pool is live.
#[tokio::test]
async fn guarded_migrations_fast_path() {
    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("fast_path.db");

    // A registered, live cooperative pool — the daemon's boot pool analogue.
    let live = init_pool(&db).await.expect("init_pool");

    // ── (1) current schema: no-op, and no quiescence wait ───────────────────
    let started = Instant::now();
    run_guarded_migrations(&db)
        .await
        .expect("current schema must fast-path while a cooperative pool is live");
    let fast_path_elapsed = started.elapsed();
    assert!(
        fast_path_elapsed < Duration::from_secs(1),
        "fast path must not wait for quiescence; took {fast_path_elapsed:?}"
    );

    // ── (2) pending migration: still quiescence-gated and bounded ───────────
    make_migration_pending(&live).await;

    let started = Instant::now();
    let err = run_guarded_migrations(&db)
        .await
        .expect_err("a pending migration must still require cooperative quiescence");
    let guarded_elapsed = started.elapsed();
    assert!(
        matches!(err, LocalDbError::OwnerBusy { .. }),
        "expected OwnerBusy with a live cooperative pool, got {err:?}"
    );
    assert!(
        guarded_elapsed <= ACQUISITION_BOUND + Duration::from_secs(1),
        "pending-migration refusal must stay bounded; took {guarded_elapsed:?}"
    );

    live.close().await;
    release_retained_writer_guards(&db);
}

/// v1.191 P0 T4 (issue #317) — focused requalification of the storage FFI
/// against the single resolved `libsqlite3-sys 0.37.0` (`links=sqlite3`)
/// shared with `sqlx-sqlite`. Deliberately not the 100-round engine-owner
/// race: four contracts, one case.
///
/// 1. **Registration** — the five protocol scalars resolve on the handle the
///    FFI entry point locks, and report the context the admission registered
///    durably.
/// 2. **Refusal** — a handle that never received the functions (the
///    pre-opened / raw writer loophole) is fenced by SQLite itself.
/// 3. **Reopen** — committed state survives and the reopened handle installs
///    its own connection-local context.
/// 4. **Destructor** — re-registering on a live connection replaces all five
///    registrations (SQLite runs each replaced box's `destroy_userdata`), and
///    every teardown after an install leaves the allocator intact with the
///    next connection reporting its own context.
#[tokio::test]
async fn v1191_sqlite_writer() {
    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("v1191_writer.db");

    let bootstrap = init_pool(&db).await.expect("init_pool");
    bootstrap.close().await;
    release_retained_writer_guards(&db);

    let writer_id = assert_writer_registration(&db).await;
    assert_unregistered_writer_refused(&db).await;
    assert_reopen_carries_its_own_context(&db, &writer_id).await;
    assert_re_registration_replaces_live_userdata(&db).await;
    #[cfg(debug_assertions)]
    assert_registration_ownership_isolated(dir.path(), &db);

    release_retained_writer_guards(&db);
}

/// (1) Registration: the five scalars resolve on the handle the FFI entry
/// point locks and report the context the admission registered durably.
/// Returns the live writer id for the reopen comparison.
async fn assert_writer_registration(db: &Path) -> String {
    let direct = open_admitted_pool(db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("direct admitted pool");
    let (writer_id, protocol, mode, migration_epoch, engine_epoch): (
        String,
        i64,
        String,
        i64,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT nexus_writer_id(), nexus_writer_protocol(), nexus_writer_mode(), \
         nexus_migration_epoch(), nexus_engine_epoch()",
    )
    .fetch_one(&direct)
    .await
    .expect("the five protocol scalars resolve on a registered connection");
    assert_eq!(protocol, 1, "connection-local protocol version");
    assert!(
        migration_epoch >= 1,
        "activated migration epoch: {migration_epoch}"
    );
    assert_eq!(mode, "direct");
    assert_eq!(
        engine_epoch, None,
        "a direct writer carries no engine epoch"
    );
    let registered: (String, String, i64, Option<i64>) = sqlx::query_as(
        "SELECT mode, creator_id, migration_epoch, engine_epoch \
         FROM core_writer_registration WHERE writer_id = ?",
    )
    .bind(&writer_id)
    .fetch_one(&direct)
    .await
    .expect("the live FFI userdata belongs to the durably registered writer");
    assert_eq!(
        registered,
        (
            mode.clone(),
            BOOTSTRAP_CREATOR_ID.to_owned(),
            migration_epoch,
            None
        ),
        "the connection-local context matches the registration row"
    );
    // The function registry of THIS connection — the SQLx handle the FFI
    // entry point locked — carries exactly the five application functions.
    let mut functions: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_function_list WHERE name LIKE 'nexus_%' AND builtin = 0",
    )
    .fetch_all(&direct)
    .await
    .expect("pragma_function_list");
    functions.sort();
    assert_eq!(
        functions,
        [
            "nexus_engine_epoch",
            "nexus_migration_epoch",
            "nexus_writer_id",
            "nexus_writer_mode",
            "nexus_writer_protocol",
        ],
        "exactly the five registered scalars on the connection being served"
    );
    sqlx::query("INSERT INTO workspace_meta (key, value) VALUES ('v1191_registered', 'ok')")
        .execute(&direct)
        .await
        .expect("a registered writer mutates a guarded table");
    direct.close().await;
    writer_id
}

/// (2) Refusal: a handle that never received the functions (the pre-opened /
/// raw writer loophole) is fenced by SQLite itself.
async fn assert_unregistered_writer_refused(db: &Path) {
    let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
        .await
        .expect("raw connect");
    assert_fenced(
        &raw,
        "INSERT INTO workspace_meta (key, value) VALUES ('v1191_raw', 'x')",
    )
    .await;
    let missing = sqlx::query_scalar::<_, Option<i64>>("SELECT nexus_writer_protocol()")
        .fetch_one(&raw)
        .await
        .expect_err("an unregistered handle has no protocol scalars");
    assert!(
        missing
            .to_string()
            .contains("no such function: nexus_writer_protocol"),
        "the fence names the missing scalar: {missing}"
    );
    raw.close().await;
}

/// (3) Reopen: committed state survives and the reopened handle installs its
/// own connection-local context.
async fn assert_reopen_carries_its_own_context(db: &Path, first_writer: &str) {
    release_retained_writer_guards(db);
    let reopened = open_admitted_pool(db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("reopened admitted pool");
    let committed: (String,) =
        sqlx::query_as("SELECT value FROM workspace_meta WHERE key = 'v1191_registered'")
            .fetch_one(&reopened)
            .await
            .expect("committed row survives the reopen");
    assert_eq!(committed.0, "ok");
    let reopened_id: (String,) = sqlx::query_as("SELECT nexus_writer_id()")
        .fetch_one(&reopened)
        .await
        .expect("the reopened handle carries its own context");
    assert_ne!(
        reopened_id.0, first_writer,
        "a reopened handle reports its own writer, never the closed one's userdata"
    );
    sqlx::query("INSERT INTO workspace_meta (key, value) VALUES ('v1191_reopened', 'ok')")
        .execute(&reopened)
        .await
        .expect("the reopened handle writes");
    reopened.close().await;
}

/// (4a) Replacement on a live connection: SQLite replaces all five
/// registrations, so the connection reports the replacement context and the
/// registry still holds exactly five names — never ten. The exit is observed
/// through values and the registry; the exact box arithmetic is asserted by
/// the isolated child below.
async fn assert_re_registration_replaces_live_userdata(db: &Path) {
    let url = format!("sqlite://{}?mode=rwc", db.display());
    let mut conn = SqliteConnection::connect(&url)
        .await
        .expect("probe connection");
    install_writer_functions(
        &mut conn,
        probe_context("v1191_first", GuardWriterMode::Direct, 1, None),
        None,
    )
    .await
    .expect("first install");
    assert_eq!(
        probe_scalars(&mut conn).await,
        ("v1191_first".to_owned(), "direct".to_owned(), 1, None),
        "the first context is live on the raw handle"
    );

    install_writer_functions(
        &mut conn,
        probe_context("v1191_second", GuardWriterMode::Engine, 2, Some(7)),
        None,
    )
    .await
    .expect("re-registration on the same live connection");
    assert_eq!(
        probe_scalars(&mut conn).await,
        ("v1191_second".to_owned(), "engine".to_owned(), 2, Some(7)),
        "the replacement context is live on the connection"
    );
    let installed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_function_list WHERE name LIKE 'nexus_%' AND builtin = 0",
    )
    .fetch_one(&mut conn)
    .await
    .expect("registry count after re-registration");
    assert_eq!(installed, 5, "re-registration replaces, never accumulates");
    conn.close().await.expect("close the probe connection");
}

/// (4b) Exact ownership accounting, run in an isolated child process: every
/// `sqlite3_create_function_v2` attempt must add exactly one userdata box, and
/// every box must be destroyed exactly once — by connection teardown or by the
/// replacement of a live registration. The counter is process-global, so a
/// sibling test installing writer functions would perturb it; the child runs
/// only this check (`--exact`, one thread).
///
/// This is the leak gate: a regression that stops invoking `destroy_userdata`
/// leaves the count above the expected value and fails here.
///
/// Coverage limit: the *failed*-registration branch of `destroy_userdata` is
/// not reachable from this fixture — SQLite returns `SQLITE_BUSY` only while a
/// statement evaluating the function is actively stepping, which a
/// single-connection, single-threaded test cannot arrange deterministically.
#[cfg(debug_assertions)]
fn assert_registration_ownership_isolated(dir: &Path, db: &Path) {
    let err_path = dir.join("v1191_ownership_err");
    let mut child = spawn_child("v1191_ownership", db, &[], &err_path);
    reap(&mut child, &err_path);
}

/// Child entry point for [`assert_registration_ownership_isolated`].
#[cfg(debug_assertions)]
async fn assert_registration_ownership(db: &Path) {
    let live = WriterConnectionContext::live_registration_count;
    let url = format!("sqlite://{}?mode=rwc", db.display());
    assert_eq!(live(), 0, "a fresh process holds no writer registrations");

    let mut conn = SqliteConnection::connect(&url)
        .await
        .expect("ownership connection");
    install_writer_functions(
        &mut conn,
        probe_context("v1191_a", GuardWriterMode::Direct, 1, None),
        None,
    )
    .await
    .expect("install");
    assert_eq!(live(), 5, "one box per registration");

    install_writer_functions(
        &mut conn,
        probe_context("v1191_b", GuardWriterMode::Engine, 2, Some(1)),
        None,
    )
    .await
    .expect("replace");
    assert_eq!(live(), 5, "a replacement destroys the five replaced boxes");
    assert_eq!(
        probe_scalars(&mut conn).await,
        ("v1191_b".to_owned(), "engine".to_owned(), 2, Some(1))
    );

    conn.close().await.expect("close the ownership connection");
    assert_eq!(live(), 0, "connection teardown destroys every registration");

    for cycle in 0..8_i32 {
        let mut conn = SqliteConnection::connect(&url)
            .await
            .expect("cycle connection");
        install_writer_functions(
            &mut conn,
            probe_context(
                &format!("v1191_cycle_{cycle}"),
                GuardWriterMode::Direct,
                i64::from(cycle) + 1,
                None,
            ),
            None,
        )
        .await
        .expect("cycle install");
        assert_eq!(live(), 5, "cycle {cycle}: one box per registration");
        conn.close().await.expect("cycle close");
        assert_eq!(live(), 0, "cycle {cycle}: teardown leaves none live");
    }
}

/// The connection-local context the FFI probe installs by hand.
fn probe_context(
    writer_id: &str,
    mode: GuardWriterMode,
    migration_epoch: i64,
    engine_epoch: Option<i64>,
) -> WriterConnectionContext {
    WriterConnectionContext {
        writer_id: writer_id.to_owned(),
        protocol_version: 1,
        mode,
        migration_epoch,
        engine_epoch,
    }
}

/// Read the identity/mode/epoch scalars back from one probe connection.
async fn probe_scalars(conn: &mut SqliteConnection) -> (String, String, i64, Option<i64>) {
    sqlx::query_as(
        "SELECT nexus_writer_id(), nexus_writer_mode(), nexus_migration_epoch(), \
         nexus_engine_epoch()",
    )
    .fetch_one(&mut *conn)
    .await
    .expect("probe scalars")
}

/// A genuine holder's final lifetime end — the local provider store dropping
/// its own direct admission, a failed initialization — must give the OS
/// admission back: a real second process can take the exclusive migration lock
/// afterwards, which it cannot while any shared writer lock survives.
#[tokio::test]
async fn dropped_guard_holders_release_the_migration_lock() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");

    let engine = init_engine_pool(&db).await.expect("engine owner");
    let direct = init_guarded_pool(&db, BOOTSTRAP_CREATOR_ID)
        .await
        .expect("direct holder");
    // The provider store's shape: the handle's final lifetime end is a drop,
    // with no explicit release call.
    drop(direct);
    // The core's shape: an explicit release at a defined drain point, then the
    // handle is given up.
    engine.pool().close().await;
    engine.release_retained_admission();
    drop(engine);

    let out = dir.path().join("fence_out");
    let err = dir.path().join("fence_err");
    let mut child = spawn_child(
        "migration_fence",
        &db,
        &[("OUT_PATH", out.to_string_lossy().to_string())],
        &err,
    );
    reap(&mut child, &err);
    assert_eq!(
        fs::read_to_string(&out).expect("fence outcome"),
        "won",
        "a separate process must acquire the exclusive migration lock"
    );
}

/// The documented raw-pool retention: an `open_admitted_pool` admission is
/// owned by this process until an explicit release, even after every pool and
/// connection the caller held is really closed and dropped.
#[tokio::test]
async fn raw_process_retention_holds_until_explicit_release() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");

    let pool = open_admitted_pool(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("raw admitted pool");
    let (one,): (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(&pool)
        .await
        .expect("raw pool serves");
    assert_eq!(one, 1);
    pool.close().await;
    drop(pool);
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }

    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "raw_blocked"),
        "busy",
        "a raw process-retained admission survives its caller's pools"
    );

    release_retained_writer_guards(&db);
    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "raw_allowed"),
        "won",
        "the documented explicit release frees the raw admission"
    );
}

/// A usable pool clone keeps the OS migration admission: a second process is
/// refused while the clone can still run a query, and allowed once the pool is
/// closed and the admission settled.
#[tokio::test]
async fn usable_pool_clone_blocks_another_process_until_settled() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");

    let guarded = init_engine_pool(&db).await.expect("engine owner");
    let survivor = guarded.clone_pool();
    drop(guarded);

    // A real query proves the surviving clone is a usable writer.
    let (one,): (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(&survivor)
        .await
        .expect("survivor query");
    assert_eq!(one, 1);

    let blocked = dir.path().join("clone_fence_blocked");
    let blocked_err = dir.path().join("clone_fence_blocked_err");
    let mut child = spawn_child(
        "migration_fence",
        &db,
        &[("OUT_PATH", blocked.to_string_lossy().to_string())],
        &blocked_err,
    );
    reap(&mut child, &blocked_err);
    assert_eq!(
        fs::read_to_string(&blocked).expect("blocked outcome"),
        "busy",
        "a second process must not migrate while the clone is usable"
    );

    survivor.close().await;
    release_retained_writer_guards(&db);
    drop(survivor);

    let allowed = dir.path().join("clone_fence_allowed");
    let allowed_err = dir.path().join("clone_fence_allowed_err");
    let mut child = spawn_child(
        "migration_fence",
        &db,
        &[("OUT_PATH", allowed.to_string_lossy().to_string())],
        &allowed_err,
    );
    reap(&mut child, &allowed_err);
    assert_eq!(
        fs::read_to_string(&allowed).expect("allowed outcome"),
        "won",
        "after the pool closed and its admission settled another process may migrate"
    );
}

/// The original public raw close: a clone is closed while one of its
/// connections is still checked out and usable. Both the migration lock and the
/// engine owner's exclusivity must stay held by that connection's own admission
/// until SQLite really disposes it — then both release on their own, with no
/// purge and no helper.
#[tokio::test]
async fn raw_clone_close_keeps_both_admissions_until_connection_disposed() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");

    let guarded = init_engine_pool(&db).await.expect("engine owner");
    let clone = guarded.clone_pool();
    drop(guarded);

    let mut checked_out = clone.acquire().await.expect("checked-out connection");
    let (one,): (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(&mut *checked_out)
        .await
        .expect("the clone's connection is usable");
    assert_eq!(one, 1);

    // The public raw close starts here and waits for the checked-out connection.
    let closing = clone.close();
    // Let any close-start settlement run before probing: the admission must
    // survive on the checked-out connection's own ownership, not on the
    // registry's timing.
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }

    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "blocked_migration"),
        "busy",
        "another process must not migrate while a checked-out connection is alive"
    );
    assert_eq!(
        child_outcome(dir.path(), &db, "engine_solo", "blocked_engine"),
        "busy",
        "nor may another process become the engine owner"
    );

    // Real disposal of that connection.
    drop(checked_out);
    closing.await;
    drop(clone);

    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "allowed_migration"),
        "won",
        "after real disposal the migration lock is free"
    );
    assert_eq!(
        child_outcome(dir.path(), &db, "engine_solo", "allowed_engine"),
        "won",
        "after real disposal the engine ownership is free"
    );
}

/// F13: an OPEN pool keeps its published admission with ZERO current native
/// connections. The last idle connection is really detached from the pool and
/// disposed, so its userdata pin is gone; both OS locks must still be held by
/// the pool itself, a reconnect must reuse the exact published identity through
/// the existing join path, and the real close must release everything on its
/// own, even though the closed handle stays referenced here.
#[tokio::test]
async fn open_pool_keeps_its_admission_with_zero_connections() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");

    let guarded = init_engine_pool(&db).await.expect("engine owner");
    let published = {
        let mut conn = guarded
            .pool()
            .acquire()
            .await
            .expect("published connection");
        probe_scalars(&mut conn).await.0
    };
    let clone = guarded.clone_pool();
    // The original holder is gone: only the pool clone and its own admission
    // remain.
    drop(guarded);

    // Real eviction: detach the last connection from the pool, then dispose it
    // so SQLite drops that connection's userdata pin.
    // Drain every pooled connection this way: each one is detached from the
    // pool and then really disposed, so SQLite drops its userdata pin.
    for _ in 0..8 {
        if clone.size() == 0 {
            break;
        }
        let conn = clone.acquire().await.expect("pooled connection to drain");
        drop(conn.detach());
    }
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        clone.size(),
        0,
        "precondition: the pool is open with zero current connections"
    );

    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "zero_conn_migration"),
        "busy",
        "an open pool with no connections still holds the migration admission"
    );
    assert_eq!(
        child_outcome(dir.path(), &db, "engine_solo", "zero_conn_engine"),
        "busy",
        "and the engine ownership"
    );

    // A reconnect is served under the SAME published admission.
    let mut reconnected = clone.acquire().await.expect("reconnect on the open pool");
    let (writer_id, mode, _, _) = probe_scalars(&mut reconnected).await;
    assert_eq!(
        writer_id, published,
        "the reconnect is the published writer, not a new identity"
    );
    assert_eq!(mode, "engine");
    drop(reconnected);

    // The existing in-process join path still resolves the published admission.
    let joined = join_live_engine_pool(&db, GuardedPoolOptions::default())
        .await
        .expect("join lookup")
        .expect("the open pool's admission is joinable with zero connections");
    let mut joined_conn = joined.pool().acquire().await.expect("joined connection");
    assert_eq!(
        probe_scalars(&mut joined_conn).await.0,
        published,
        "a joining open reuses the published identity"
    );
    drop(joined_conn);
    // Drain the independent joined pool's native pins before closing the original pool.
    joined.pool().close().await;
    drop(joined);

    // Close-start retirement: the closed handle stays referenced below.
    clone.close().await;
    assert_eq!(
        child_outcome(
            dir.path(),
            &db,
            "migration_fence",
            "zero_conn_closed_migration"
        ),
        "won",
        "the real close released the migration admission with no purge"
    );
    assert_eq!(
        child_outcome(dir.path(), &db, "engine_solo", "zero_conn_closed_engine"),
        "won",
        "and freed the engine ownership"
    );
    assert_eq!(clone.size(), 0, "the closed handle stays referenced");
}

/// F14: a raw pool that JOINS an ordinary open must promote the published
/// admission to process retention — parking the claim alone would let the
/// admission die with the ordinary holder.
#[tokio::test]
async fn joined_raw_open_retains_the_published_admission() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");

    let ordinary = init_guarded_pool(&db, BOOTSTRAP_CREATOR_ID)
        .await
        .expect("ordinary direct holder");
    let raw = open_admitted_pool(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("raw admitted pool");
    let (one,): (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(&raw)
        .await
        .expect("raw pool serves");
    assert_eq!(one, 1);
    raw.close().await;
    drop(raw);

    // The ordinary holder ends: its pool closes and its own claim goes back.
    ordinary.pool().close().await;
    ordinary.release_retained_admission();
    drop(ordinary);
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }

    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "joined_raw_blocked"),
        "busy",
        "the joined raw retention outlives the ordinary holder it joined"
    );

    release_retained_writer_guards(&db);
    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "joined_raw_allowed"),
        "won",
        "the documented explicit release frees the joined raw admission"
    );
}

/// F14: retaining a guard NEVER installs an identity that is not the published
/// one. The caller's guard here is a stale identity whose entry was dropped
/// while the guard stayed alive; the admission a later open published must stay
/// authoritative, and the next join must carry that published writer id.
#[tokio::test]
async fn raw_retention_keeps_the_published_identity() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }

    let dir = TempDir::new().expect("tempdir");
    let db = dir.path().join("state.db");
    run_guarded_migrations(&db)
        .await
        .expect("migrated workspace");

    // Identity A publishes itself, then its entry is dropped while the guard
    // itself is still alive.
    let stale = acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("stale identity");
    let stale_pool = open_guarded_pool_arc(Arc::clone(&stale))
        .await
        .expect("stale pool publishes its admission");
    let stale_id = {
        let mut conn = stale_pool.pool().acquire().await.expect("stale connection");
        probe_scalars(&mut conn).await.0
    };
    release_retained_writer_guards(&db);

    // Identity B is published afterwards.
    let published = init_guarded_pool(&db, BOOTSTRAP_CREATOR_ID)
        .await
        .expect("published holder");
    let published_id = {
        let mut conn = published
            .pool()
            .acquire()
            .await
            .expect("published connection");
        probe_scalars(&mut conn).await.0
    };
    assert_ne!(
        stale_id, published_id,
        "precondition: two different identities exist for this path"
    );

    // The caller hands over the STALE guard: retention must keep B.
    retain_writer_guard(&stale);

    let joined = acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
        .await
        .expect("join the published admission");
    let joined_pool = open_guarded_pool(&joined).await.expect("joined pool");
    let mut conn = joined_pool.acquire().await.expect("joined connection");
    assert_eq!(
        probe_scalars(&mut conn).await.0,
        published_id,
        "the retained/joined admission is the published identity, not the caller's"
    );
    drop(conn);
    stale_pool.pool().close().await;
    drop(stale_pool);
    joined_pool.close().await;
    published.pool().close().await;
    published.release_retained_admission();
    drop(published);
    drop(joined);
    release_retained_writer_guards(&db);
    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "stale_identity_held"),
        "busy",
        "the caller's stale guard still owns its admission after its pool closes"
    );
    drop(stale);

    assert_eq!(
        child_outcome(dir.path(), &db, "migration_fence", "identity_released"),
        "won",
        "every identity's locks are released once both are gone"
    );
}

async fn child_main() {
    let mode = std::env::var(CHILD_ENV).expect("child mode");
    let db = PathBuf::from(std::env::var("DB_PATH").expect("db"));
    match mode.as_str() {
        "engine_race" => {
            let self_barrier = PathBuf::from(std::env::var("BARRIER_SELF").expect("self"));
            let peer_barrier = PathBuf::from(std::env::var("BARRIER_PEER").expect("peer"));
            let out = PathBuf::from(std::env::var("OUT_PATH").expect("out"));
            barrier(&self_barrier);
            wait_barrier(&peer_barrier, Duration::from_secs(30));
            let outcome =
                match acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine).await {
                    Ok(guard) => {
                        // Hold ownership across the peer's attempt so the loser
                        // observes a live owner rather than a released lock.
                        std::thread::sleep(Duration::from_millis(50));
                        drop(guard);
                        "won"
                    }
                    Err(LocalDbError::OwnerBusy { .. }) => "busy",
                    Err(err) => panic!("unexpected child error: {err}"),
                };
            fs::write(&out, outcome).expect("write outcome");
        }
        "engine_solo" => {
            let out = PathBuf::from(std::env::var("OUT_PATH").expect("out"));
            let outcome =
                match acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine).await {
                    Ok(_) => "won",
                    Err(LocalDbError::OwnerBusy { .. }) => "busy",
                    Err(err) => panic!("unexpected child error: {err}"),
                };
            fs::write(out, outcome).expect("write owner outcome");
        }
        "migration_fence" => {
            let out = PathBuf::from(std::env::var("OUT_PATH").expect("out"));
            let outcome = match acquire_store_reset_fence(&db) {
                Ok(fence) => {
                    drop(fence);
                    "won"
                }
                Err(LocalDbError::OwnerBusy { .. }) => "busy",
                Err(err) => panic!("unexpected child error: {err}"),
            };
            fs::write(out, outcome).expect("write fence outcome");
        }
        #[cfg(debug_assertions)]
        "v1191_ownership" => assert_registration_ownership(&db).await,
        other => panic!("unknown child mode {other}"),
    }
}
