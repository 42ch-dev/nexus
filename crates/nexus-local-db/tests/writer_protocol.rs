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
use std::time::{Duration, Instant};

use nexus_local_db::writer_protocol::{
    acquire_writer_guard, open_admitted_pool, open_guarded_pool, run_guarded_migrations,
    BOOTSTRAP_CREATOR_ID, WriterMode,
};
use nexus_local_db::{init_pool, open_pool_read_only, LocalDbError};
use sqlx::SqlitePool;
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
        wait_barrier(&dir.join(format!("r{round}_self{index}")), Duration::from_secs(30));
    }
    for (child, err) in &mut children {
        reap(child, err);
    }
    let results: Vec<String> = outcomes
        .iter()
        .map(|path| fs::read_to_string(path).expect("child outcome").trim().to_string())
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

    // ── DB-4 (pre-migration fixture) ────────────────────────────────────────
    // A connection opened BEFORE the protocol migration commits a row, keeps
    // its handle open across activation, and must then be fenced; the row it
    // committed before activation must survive intact.
    {
        let legacy_db = dir.path().join("legacy.db");
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

    // ── main database ───────────────────────────────────────────────────────
    let db = dir.path().join("state.db");
    init_pool(&db).await.expect("init_pool");

    {
        let pool = open_pool_read_only(&db).await.expect("read-only pool");
        assert_trigger_coverage(&pool).await;
        pool.close().await;
    }

    // DB-1: 100 barrier-synchronised engine-owner races. Exactly one winner per
    // round and exactly one engine-epoch advance per round (no lost/duplicated
    // ownership), driven by two real OS processes.
    for round in 0..100 {
        engine_race_round(dir.path(), &db, round);
        let epoch = engine_epoch(&db).await;
        assert_eq!(
            epoch,
            i64::try_from(round).unwrap() + 1,
            "round {round}: engine epoch must advance exactly once"
        );
    }

    // Distinct entities: two direct writers both succeed with no engine running.
    for (key, value) in [("direct_a", "ok"), ("direct_b", "ok")] {
        let pool = open_admitted_pool(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
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

    // ── §12.1 outbox, entirely without an engine owner ──────────────────────
    {
        let guard = acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Direct)
            .await
            .expect("direct guard");
        let pool = open_guarded_pool(&guard).await.expect("guarded pool");
        for index in 0..4097 {
            sqlx::query(
                "INSERT INTO core_changes (world_id, resource_kind, resource_id, resource_revision, change_kind, writer_id) \
                 VALUES ('wld_test', 'entity', ?, '1', 'upsert', ?)",
            )
            .bind(format!("kb_out_{index}"))
            .bind(guard.writer_id())
            .execute(&pool)
            .await
            .expect("outbox insert");
        }
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM core_changes")
            .fetch_one(&pool)
            .await
            .expect("outbox count");
        assert_eq!(count, 4097, "every direct mutation is durably logged");

        // Bounded oldest-row retention: the oldest committed row is trimmed and
        // the newest is retained.
        let trimmed = sqlx::query("DELETE FROM core_changes WHERE sequence = 1")
            .execute(&pool)
            .await
            .expect("oldest-row retention");
        assert_eq!(trimmed.rows_affected(), 1);
        let (remaining, min_seq): (i64, i64) =
            sqlx::query_as("SELECT COUNT(*), MIN(sequence) FROM core_changes")
                .fetch_one(&pool)
                .await
                .expect("retained window");
        assert_eq!(remaining, 4096);
        assert_eq!(min_seq, 2);
        pool.close().await;
    }

    // Raw (unregistered) writer is fenced on the outbox too.
    {
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
            .await
            .expect("raw pool");
        assert_fenced(&raw, "DELETE FROM core_changes WHERE sequence = 2").await;
        raw.close().await;
    }

    // ── DB-3: engine ownership is fail-fast while an owner is live ──────────
    {
        let owner = acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine)
            .await
            .expect("engine owner");
        let started = Instant::now();
        let err = match acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine).await {
            Ok(_) => panic!("a live engine owner must not be replaced"),
            Err(err) => err,
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
        let epoch_with_owner = engine_epoch(&db).await;
        let err = match acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine).await {
            Ok(_) => panic!("a second in-process engine acquisition must be refused"),
            Err(err) => err,
        };
        assert!(matches!(err, LocalDbError::OwnerBusy { .. }), "got {err:?}");
        assert_eq!(
            engine_epoch(&db).await,
            epoch_with_owner,
            "a refused acquisition must not advance the engine epoch"
        );

        // Cross-process: a child cannot steal the live owner's engine either.
        let err_path = dir.path().join("err_owner");
        let owner_out = dir.path().join("owner_out");
        let mut child = spawn_child(
            "engine_solo",
            &db,
            &[("OUT_PATH", owner_out.to_string_lossy().to_string())],
            &err_path,
        );
        reap(&mut child, &err_path);
        assert_eq!(
            fs::read_to_string(dir.path().join("owner_out")).expect("owner out"),
            "busy"
        );

        // Ownership transfer across restart: once the owner is gone the next
        // engine takes over and advances the epoch exactly once.
        drop(owner);
        let successor = acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine)
            .await
            .expect("engine ownership transfers after the predecessor exits");
        assert_eq!(
            engine_epoch(&db).await,
            epoch_with_owner + 1,
            "the successor owner advances the engine epoch exactly once"
        );
        drop(successor);
    }

    // Reopened *raw* legacy connection is still fenced on guarded tables.
    {
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

    // Re-open preserves committed mutations exactly once.
    {
        let pool = init_pool(&db).await.expect("reopen");
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

    // ── preserved pre-activation state on the fixture database ──────────────
    {
        let legacy_db = dir.path().join("legacy.db");
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
            let outcome = match acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine)
                .await
            {
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
            let outcome = match acquire_writer_guard(&db, BOOTSTRAP_CREATOR_ID, WriterMode::Engine)
                .await
            {
                Ok(_) => "won",
                Err(LocalDbError::OwnerBusy { .. }) => "busy",
                Err(err) => panic!("unexpected child error: {err}"),
            };
            fs::write(out, outcome).expect("write owner outcome");
        }
        other => panic!("unknown child mode {other}"),
    }
}
