//! P1 proof driver: concurrent HTTP/CLI World KB patch scenario.
//!
//! Spawns the current HTTP host (`nexus42 daemon-run`), waits for
//! `/v1/daemon/runtime/health`, then runs a barrier-synchronised stale-version
//! write group (two HTTP contenders + the selected CLI process) and the named
//! coexistence/recovery cases.
//!
//! v1.189 P1-T3 additions:
//! - the barrier loser must be a TYPED conflict (HTTP 409, or CLI exit 76 with
//!   `world_kb_conflict` on stderr) — any other non-zero CLI error fails;
//! - `--cli-bin` is required (a two-HTTP-contender run no longer passes);
//! - named cases: distinct-row success, future-version conflict, foreign-world
//!   denial, crash recovery, and DB-2 read-after-patch visibility;
//! - a deterministic transport trace: the CLI child's TCP sockets are sampled
//!   for its whole lifetime and must never include the daemon port (with a
//!   positive control proving the sampler can see such a socket).

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::store::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::init_pool;
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use sqlx::SqlitePool;
use tempfile::TempDir;

const CREATOR: &str = "proof_creator";
/// Second creator owning a DIFFERENT world — the foreign-world denial subject.
const FOREIGN_CREATOR: &str = "other_creator";
const SLUG: &str = "default";
const CLI_DEGRADED_EXIT: i32 = 78;
/// Exit code the CLI uses for a per-row OCC conflict (E_VERSION family).
const CLI_CONFLICT_EXIT: i32 = 76;

struct ProofHome {
    _tmp: TempDir,
    user_home: PathBuf,
    daemon_url: String,
    world_id: String,
    entity_id: String,
    /// Second entity in the same world (distinct-row case).
    entity_b_id: String,
    /// World owned by [`FOREIGN_CREATOR`] (foreign-world case).
    foreign_world_id: String,
    foreign_entity_id: String,
}

/// One CLI invocation, with the TCP sockets observed while it ran.
struct CliRun {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    /// Every `lsof -iTCP` line sampled for this pid during its lifetime.
    socket_samples: Vec<String>,
}

impl CliRun {
    /// True when the CLI lost the CAS as a *typed* conflict: exit 76 AND the
    /// structured `world_kb_conflict` surface on stderr. Any other non-zero
    /// exit (config error, panic, connection failure) is NOT a conflict.
    fn is_typed_conflict(&self) -> bool {
        self.code == Some(CLI_CONFLICT_EXIT) && self.stderr.contains("world_kb_conflict")
    }

    fn touched_daemon_port(&self, port: u16) -> bool {
        let needle = format!(":{port}");
        self.socket_samples.iter().any(|l| l.contains(&needle))
    }
}

fn tcp_connect_with_retry<A: ToSocketAddrs>(
    addr: A,
    timeout: Duration,
) -> Option<TcpStream> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(addrs) = addr.to_socket_addrs() {
            for socket_addr in addrs {
                if let Ok(stream) = TcpStream::connect_timeout(&socket_addr, Duration::from_millis(200)) {
                    return Some(stream);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn reserve_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve port")
        .local_addr()
        .expect("local addr")
        .port()
}

/// Sample one pid's TCP sockets via `lsof`.
///
/// `-a` ANDs the selectors, `-n`/`-P` keep addresses/ports numeric so the
/// daemon port can be matched textually.
fn lsof_tcp_lines(pid: u32) -> Vec<String> {
    let out = Command::new("lsof")
        .args(["-p", &pid.to_string(), "-a", "-iTCP", "-n", "-P"])
        .output();
    match out {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .skip(1) // header
            .map(str::to_string)
            .collect(),
        Err(e) => vec![format!("lsof-unavailable: {e}")],
    }
}

/// Run the CLI, sampling its TCP sockets for the whole process lifetime.
///
/// The sampler thread polls `lsof` continuously from just after spawn until the
/// child is reaped, so a connection to the daemon would be observed (see
/// [`sampler_positive_control`] for proof the sampler sees one).
fn run_cli(cli: &Path, home: &Path, args: &[&str]) -> CliRun {
    let child = Command::new(cli)
        .args(args)
        .env("HOME", home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn CLI");

    let pid = child.id();
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut samples: Vec<String> = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                for line in lsof_tcp_lines(pid) {
                    if !samples.contains(&line) {
                        samples.push(line);
                    }
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            samples
        })
    };

    let output: Output = child.wait_with_output().expect("CLI output");
    stop.store(true, Ordering::Relaxed);
    let socket_samples = sampler.join().expect("sampler thread");

    CliRun {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        socket_samples,
    }
}

/// Spawn the CLI and SIGKILL it almost immediately (the crash case).
fn spawn_and_kill_cli(cli: &Path, home: &Path, args: &[&str], delay: Duration) -> Option<i32> {
    let mut child = Command::new(cli)
        .args(args)
        .env("HOME", home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn CLI for crash case");
    std::thread::sleep(delay);
    let _ = child.kill();
    child.wait().ok().and_then(|s| s.code())
}

/// Prove the socket sampler is not vacuous: open a real TCP connection to the
/// daemon port from THIS process and confirm the sampler sees it.
fn sampler_positive_control(port: u16) -> (bool, Vec<String>) {
    let Ok(_hold) = TcpStream::connect(("127.0.0.1", port)) else {
        return (false, vec!["positive-control-connect-failed".to_string()]);
    };
    let pid = std::process::id();
    let needle = format!(":{port}");
    let mut seen = Vec::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        seen = lsof_tcp_lines(pid);
        if seen.iter().any(|l| l.contains(&needle)) {
            return (true, seen);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    (false, seen)
}

fn read_http_body(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(buf)
}

fn http_get(host: &str, port: u16, path: &str, connect_timeout: Duration) -> Option<String> {
    let mut stream = tcp_connect_with_retry((host, port), connect_timeout)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .expect("write GET");
    let buf = read_http_body(&mut stream).expect("read GET response");
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn http_post_json(
    host: &str,
    port: u16,
    path: &str,
    body: &str,
    connect_timeout: Duration,
) -> Option<(u16, String)> {
    let mut stream = tcp_connect_with_retry((host, port), connect_timeout)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(request.as_bytes())
        .expect("write POST");
    let buf = read_http_body(&mut stream).expect("read POST response");
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    Some((status, text))
}

fn wait_healthy(port: u16, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(health) =
            http_get("127.0.0.1", port, "/v1/daemon/runtime/health", Duration::from_secs(2))
        {
            if health.contains("HTTP/1.1 200") {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon health check timed out on port {port}");
}

/// True when the CLI binary exposes the direct graph/patch surface.
fn cli_supports_direct_path(cli_bin: &Path) -> bool {
    Command::new(cli_bin)
        .args(["creator", "world", "kb", "entity", "patch", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn patch_body(entity_id: &str, title: &str) -> String {
    serde_json::json!({
        "entity_id": entity_id,
        "expected_version": 0,
        "patch": { "title": title }
    })
    .to_string()
}

/// CLI patch argv for `entity patch`.
fn patch_args<'a>(
    world_id: &'a str,
    entity_id: &'a str,
    expected_version: &'a str,
    title: &'a str,
) -> Vec<&'a str> {
    vec![
        "creator",
        "world",
        "kb",
        "entity",
        "patch",
        "--world-id",
        world_id,
        "--entity-id",
        entity_id,
        "--expected-version",
        expected_version,
        "--title",
        title,
    ]
}

fn graph_args(world_id: &str) -> Vec<String> {
    vec![
        "creator".to_string(),
        "world".to_string(),
        "kb".to_string(),
        "graph".to_string(),
        "--world-id".to_string(),
        world_id.to_string(),
        "--json".to_string(),
    ]
}

/// Read the graph via the CLI and return `(version, canonical_name)` for `entity_id`.
fn cli_graph_entity_version(cli: &Path, home: &Path, world_id: &str, entity_id: &str) -> Option<u64> {
    let args = graph_args(world_id);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = run_cli(cli, home, &refs);
    if run.code != Some(0) {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(&run.stdout).ok()?;
    parsed["entities"]
        .as_array()?
        .iter()
        .find(|e| e["key_block_id"] == entity_id)
        .and_then(|e| e["version"].as_u64())
}

struct BarrierRound {
    http_status_a: u16,
    http_status_b: u16,
    cli_code: Option<i32>,
    cli_conflict_typed: bool,
    cli_degraded: bool,
    cli_contended: bool,
    /// True when the CLI leg opened no socket toward the daemon port.
    cli_transport_clean: bool,
    cli_socket_samples: Vec<String>,
}

/// Barrier round: two HTTP contenders + the CLI process, all racing one row.
fn run_barrier_round(
    port: u16,
    home: &Path,
    world_id: &str,
    entity_id: &str,
    cli_bin: &Path,
) -> (BarrierRound, bool) {
    let cli_contended = cli_supports_direct_path(cli_bin);
    let cli_degraded = !cli_contended;
    let parties = 3;
    let barrier = Arc::new(Barrier::new(parties));
    let path = format!("/v1/daemon/worlds/{world_id}/kb/patch-entity");
    let body_a = patch_body(entity_id, "Barrier A");
    let body_b = patch_body(entity_id, "Barrier B");
    let connect_timeout = Duration::from_secs(10);

    let http_a = {
        let barrier = Arc::clone(&barrier);
        let path = path.clone();
        let body = body_a.clone();
        std::thread::spawn(move || {
            barrier.wait();
            http_post_json("127.0.0.1", port, &path, &body, connect_timeout)
                .map(|(status, _)| status)
                .unwrap_or(0)
        })
    };
    let http_b = {
        let barrier = Arc::clone(&barrier);
        let path = path.clone();
        let body = body_b.clone();
        std::thread::spawn(move || {
            barrier.wait();
            http_post_json("127.0.0.1", port, &path, &body, connect_timeout)
                .map(|(status, _)| status)
                .unwrap_or(0)
        })
    };

    // The CLI leg only races when it has the direct path; otherwise it is a
    // degraded HTTP-only run and the caller fails on `cli_degraded`.
    let cli_run = if cli_contended {
        let barrier = Arc::clone(&barrier);
        let cli = cli_bin.to_path_buf();
        let home = home.to_path_buf();
        let args: Vec<String> = patch_args(world_id, entity_id, "0", "Barrier CLI")
            .into_iter()
            .map(str::to_string)
            .collect();
        Some(std::thread::spawn(move || {
            barrier.wait();
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run_cli(&cli, &home, &refs)
        }))
    } else {
        barrier.wait();
        None
    };

    let status_a = http_a.join().expect("http_a thread");
    let status_b = http_b.join().expect("http_b thread");
    let (cli_code, cli_conflict_typed, cli_transport_clean, cli_socket_samples) =
        cli_run.map_or((None, false, true, Vec::new()), |h| {
            let run = h.join().expect("cli thread");
            let typed = run.is_typed_conflict();
            let clean = !run.touched_daemon_port(port);
            (run.code, typed, clean, run.socket_samples)
        });

    (
        BarrierRound {
            http_status_a: status_a,
            http_status_b: status_b,
            cli_code,
            cli_conflict_typed,
            cli_degraded,
            cli_contended,
            cli_transport_clean,
            cli_socket_samples,
        },
        cli_degraded,
    )
}

/// Exact-one-winner accounting over typed outcomes only.
fn contender_outcomes(round: &BarrierRound) -> (u32, u32, u32, u32) {
    let mut winners = 0u32;
    let mut conflicts = 0u32;
    let mut contenders = 0u32;
    let mut untyped = 0u32;

    for status in [round.http_status_a, round.http_status_b] {
        contenders += 1;
        if status == 200 {
            winners += 1;
        } else if status == 409 {
            conflicts += 1;
        } else {
            untyped += 1;
        }
    }

    if round.cli_contended {
        contenders += 1;
        match round.cli_code {
            Some(0) => winners += 1,
            // EVERY losing CLI leg must be a typed conflict; an arbitrary
            // non-zero exit (config error, panic, ECONNREFUSED) is not.
            _ if round.cli_conflict_typed => conflicts += 1,
            _ => untyped += 1,
        }
    }

    (winners, conflicts, contenders, untyped)
}

async fn seed_creator(pool: &SqlitePool, owner: &str, name: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', datetime('now'), '{}')",
    )
    .bind(owner)
    .bind(name)
    .execute(pool)
    .await
    .expect("seed creator");
}

async fn insert_entity(
    pool: &SqlitePool,
    world_id: &str,
    name: &str,
) -> String {
    let mut entry = KnowledgeEntryRecord::new(world_id, BlockType::Character, name);
    entry.status = "confirmed".to_string();
    entry.revision = Some(0);
    let entity_id = entry.entry_id.clone();
    let store = SqliteKbStore::new(pool.clone());
    store
        .insert_knowledge_entry(entry)
        .await
        .expect("insert knowledge entry");
    entity_id
}

/// Seed: the proof world (two entities) plus a foreign world owned by another
/// creator (one entity).
async fn bootstrap_fixture(pool: &SqlitePool) -> (String, String, String, String, String) {
    seed_creator(pool, CREATOR, "Proof").await;
    let world = nexus_local_db::narrative_write::create_world(
        pool,
        CREATOR,
        "Proof",
        "proof",
        "private",
        "manual",
    )
    .await
    .expect("create world via narrative_write");
    let entity_id = insert_entity(pool, &world.world_id, "Hero").await;
    let entity_b_id = insert_entity(pool, &world.world_id, "Sidekick").await;

    seed_creator(pool, FOREIGN_CREATOR, "Other").await;
    let foreign = nexus_local_db::narrative_write::create_world(
        pool,
        FOREIGN_CREATOR,
        "Other",
        "other",
        "private",
        "manual",
    )
    .await
    .expect("create foreign world");
    let foreign_entity_id = insert_entity(pool, &foreign.world_id, "Intruder").await;

    (
        world.world_id,
        entity_id,
        entity_b_id,
        foreign.world_id,
        foreign_entity_id,
    )
}

fn seed_fixture_in_child(user_home: &Path) -> (String, String, String, String, String) {
    let self_exe = std::env::current_exe().expect("current exe");
    let output = Command::new(self_exe)
        .arg("--seed-only")
        .arg("--home")
        .arg(user_home)
        .output()
        .expect("spawn seed child");
    if !output.status.success() {
        eprintln!("seed child stderr: {}", String::from_utf8_lossy(&output.stderr));
        panic!("seed child failed");
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<String> = line.split_whitespace().map(str::to_string).collect();
    assert_eq!(parts.len(), 5, "seed child must print 5 ids, got: {line}");
    (
        parts[0].clone(),
        parts[1].clone(),
        parts[2].clone(),
        parts[3].clone(),
        parts[4].clone(),
    )
}

async fn seed_only_main(home: PathBuf) {
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("nexus home");
    let op = nexus_home_layout::operational_workspace_dir(&home, CREATOR, SLUG);
    std::fs::create_dir_all(&op).expect("workspace dir");
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let pool = init_pool(&db_path).await.expect("init pool for fixture seed");
    let (world_id, entity_id, entity_b_id, foreign_world_id, foreign_entity_id) =
        bootstrap_fixture(&pool).await;
    pool.close().await;
    release_retained_writer_guards(&db_path);
    println!("{world_id} {entity_id} {entity_b_id} {foreign_world_id} {foreign_entity_id}");
}

async fn prepare_home(port: u16) -> ProofHome {
    let tmp = TempDir::new().expect("temp home");
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("nexus home");
    let op = nexus_home_layout::operational_workspace_dir(&user_home, CREATOR, SLUG);
    std::fs::create_dir_all(&op).expect("workspace dir");
    let daemon_url = format!("http://127.0.0.1:{port}");
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             daemon_url = \"{daemon_url}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"{SLUG}\"\n"
        ),
    )
    .expect("config.toml");

    let (world_id, entity_id, entity_b_id, foreign_world_id, foreign_entity_id) =
        seed_fixture_in_child(&user_home);

    ProofHome {
        _tmp: tmp,
        user_home,
        daemon_url,
        world_id,
        entity_id,
        entity_b_id,
        foreign_world_id,
        foreign_entity_id,
    }
}

fn spawn_daemon(http_bin: &Path, home: &Path, port: u16) -> Child {
    Command::new(http_bin)
        .arg("daemon-run")
        .arg("--port")
        .arg(port.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .env("HOME", home)
        .env("RUST_LOG", "off")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon-run")
}

/// Named coexistence/recovery cases (proof-matrix §3 P1-T3).
fn run_named_cases(home: &ProofHome, cli: &Path) -> serde_json::Value {
    let mut out = serde_json::Map::new();

    // ── distinct-row: two different entities both succeed ───────────────────
    {
        // Derive each row's current version from the canonical read so the case
        // is independent of whatever earlier rounds did to the row.
        let a_version =
            cli_graph_entity_version(cli, &home.user_home, &home.world_id, &home.entity_id)
                .unwrap_or(0);
        let b_version =
            cli_graph_entity_version(cli, &home.user_home, &home.world_id, &home.entity_b_id)
                .unwrap_or(0);
        let a = run_cli(
            cli,
            &home.user_home,
            &patch_args(
                &home.world_id,
                &home.entity_id,
                &a_version.to_string(),
                "Distinct A",
            ),
        );
        let b = run_cli(
            cli,
            &home.user_home,
            &patch_args(
                &home.world_id,
                &home.entity_b_id,
                &b_version.to_string(),
                "Distinct B",
            ),
        );
        let both_ok = a.code == Some(0) && b.code == Some(0);
        out.insert(
            "distinct_row".to_string(),
            serde_json::json!({
                "entity_a_expected_version": a_version,
                "entity_a_code": a.code,
                "entity_b_expected_version": b_version,
                "entity_b_code": b.code,
                "both_succeeded": both_ok,
                "stderr_a": a.stderr.trim(),
                "stderr_b": b.stderr.trim(),
                "ok": both_ok,
            }),
        );
    }

    // ── DB-2 visibility: a read begun after the patch ack sees its version ──
    {
        let before =
            cli_graph_entity_version(cli, &home.user_home, &home.world_id, &home.entity_id)
                .unwrap_or(0);
        let patch = run_cli(
            cli,
            &home.user_home,
            &patch_args(
                &home.world_id,
                &home.entity_id,
                &before.to_string(),
                "Visible",
            ),
        );
        let acked = patch.code == Some(0);
        let after =
            cli_graph_entity_version(cli, &home.user_home, &home.world_id, &home.entity_id);
        let visible = matches!(after, Some(v) if v > before);
        out.insert(
            "db2_visibility".to_string(),
            serde_json::json!({
                "expected_version_used": before,
                "patch_code": patch.code,
                "version_before": before,
                "version_after": after,
                "patch_acked": acked,
                "read_sees_newer_version": visible,
                "ok": acked && visible,
            }),
        );
    }

    // ── future-version: expected above the row's version → typed conflict ───
    {
        let current =
            cli_graph_entity_version(cli, &home.user_home, &home.world_id, &home.entity_id)
                .unwrap_or(0);
        let future = (current + 5).to_string();
        let run = run_cli(
            cli,
            &home.user_home,
            &patch_args(&home.world_id, &home.entity_id, &future, "Future"),
        );
        let ok = run.is_typed_conflict();
        out.insert(
            "future_version".to_string(),
            serde_json::json!({
                "expected_version": future,
                "exit_code": run.code,
                "typed_conflict": run.is_typed_conflict(),
                "stderr": run.stderr.trim(),
                "ok": ok,
            }),
        );
    }

    // ── foreign-world: the active creator may not patch another's world ─────
    {
        let run = run_cli(
            cli,
            &home.user_home,
            &patch_args(&home.foreign_world_id, &home.foreign_entity_id, "0", "Intrude"),
        );
        let denied = run.code != Some(0) && !run.stderr.contains("world_kb_conflict");
        out.insert(
            "foreign_world".to_string(),
            serde_json::json!({
                "exit_code": run.code,
                "stderr": run.stderr.trim(),
                "denied_without_conflict": denied,
                "ok": denied,
            }),
        );
    }

    // ── crash: SIGKILL mid-write must leave a usable, consistent DB ─────────
    {
        let before =
            cli_graph_entity_version(cli, &home.user_home, &home.world_id, &home.entity_id)
                .unwrap_or(0);
        let killed_code = spawn_and_kill_cli(
            cli,
            &home.user_home,
            &patch_args(&home.world_id, &home.entity_id, &before.to_string(), "Crash"),
            Duration::from_millis(8),
        );
        // Recovery: a read must succeed and a retry with the observed version
        // must be accepted — no poisoned lock, no half-applied row.
        let after =
            cli_graph_entity_version(cli, &home.user_home, &home.world_id, &home.entity_id);
        let retry = run_cli(
            cli,
            &home.user_home,
            &patch_args(
                &home.world_id,
                &home.entity_id,
                &after.unwrap_or(before).to_string(),
                "Recovered",
            ),
        );
        let recovered = after.is_some() && retry.code == Some(0);
        out.insert(
            "crash_recovery".to_string(),
            serde_json::json!({
                "killed_exit_code": killed_code,
                "version_after_crash": after,
                "retry_exit_code": retry.code,
                "readable_and_retry_accepted": recovered,
                "ok": recovered,
            }),
        );
    }

    serde_json::Value::Object(out)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--seed-only") {
        let home_arg = args.get(pos + 1).map(String::as_str);
        let home = if home_arg == Some("--home") {
            PathBuf::from(args.get(pos + 2).expect("--home path"))
        } else {
            PathBuf::from(home_arg.expect("--home path after --seed-only"))
        };
        seed_only_main(home).await;
        return;
    }

    let mut scenario = String::new();
    let mut cli_bin: Option<PathBuf> = None;
    let mut http_bin: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scenario" if i + 1 < args.len() => {
                scenario = args[i + 1].clone();
                i += 2;
            }
            "--http-bin" if i + 1 < args.len() => {
                http_bin = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--cli-bin" if i + 1 < args.len() => {
                cli_bin = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--out" if i + 1 < args.len() => {
                out_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    if scenario != "concurrent-http-cli" {
        eprintln!(
            "unsupported scenario '{scenario}'; expected --scenario concurrent-http-cli"
        );
        std::process::exit(2);
    }

    let http_bin = http_bin.unwrap_or_else(|| PathBuf::from("target/debug/nexus42"));
    if !http_bin.exists() {
        eprintln!(
            "HTTP host binary not found at {}; build with `cargo build -p nexus42 --bin nexus42`",
            http_bin.display()
        );
        std::process::exit(2);
    }
    // P1-T3 requires the CLI leg: a two-HTTP-contender run cannot prove
    // cross-process coexistence between the host and the direct CLI.
    let Some(cli_bin) = cli_bin else {
        eprintln!("--cli-bin is required for scenario concurrent-http-cli");
        std::process::exit(2);
    };
    if !cli_bin.exists() {
        eprintln!(
            "CLI binary not found at {}; build with `cargo build -p nexus42 --bin nexus42 --no-default-features --features basic-cli`",
            cli_bin.display()
        );
        std::process::exit(2);
    }

    let out = out_dir.unwrap_or_else(|| PathBuf::from("evidence"));
    std::fs::create_dir_all(&out).expect("create evidence dir");

    let port = reserve_port();
    let home = prepare_home(port).await;
    let mut daemon = spawn_daemon(&http_bin, &home.user_home, port);
    wait_healthy(port, Duration::from_secs(30));

    // Positive control AFTER the host is listening — it must be able to
    // observe a real connection to the daemon port, otherwise a "no sockets"
    // result for the CLI would be vacuous.
    let (sampler_ok, sampler_lines) = sampler_positive_control(port);

    let (round, cli_degraded) = run_barrier_round(
        port,
        &home.user_home,
        &home.world_id,
        &home.entity_id,
        &cli_bin,
    );

    let (winners, conflicts, contenders, untyped) = contender_outcomes(&round);
    let barrier_ok = winners == 1
        && conflicts == contenders - 1
        && untyped == 0
        && round.cli_transport_clean
        && sampler_ok;

    let cases = run_named_cases(&home, &cli_bin);
    let cases_ok = cases
        .as_object()
        .is_some_and(|m| m.values().all(|v| v["ok"] == serde_json::Value::Bool(true)));

    let evidence = serde_json::json!({
        "scenario": scenario,
        "http_bin": http_bin.display().to_string(),
        "cli_bin": cli_bin.display().to_string(),
        "daemon_url": home.daemon_url,
        "world_id": home.world_id,
        "entity_id": home.entity_id,
        "entity_b_id": home.entity_b_id,
        "foreign_world_id": home.foreign_world_id,
        "health_path": "/v1/daemon/runtime/health",
        "cli_transport_trace": {
            "mechanism": "lsof -p <cli-pid> -a -iTCP -n -P sampled continuously for the CLI process lifetime",
            "sampler_positive_control_ok": sampler_ok,
            "sampler_positive_control_lines": sampler_lines,
            "cli_socket_samples": round.cli_socket_samples,
            "cli_touched_daemon_port": !round.cli_transport_clean,
        },
        "barrier_round": {
            "http_status_a": round.http_status_a,
            "http_status_b": round.http_status_b,
            "cli_exit_code": round.cli_code,
            "cli_degraded": round.cli_degraded,
            "cli_contended": round.cli_contended,
            "cli_conflict_typed": round.cli_conflict_typed,
            "cli_transport_clean": round.cli_transport_clean,
            "contender_count": contenders,
            "winner_count": winners,
            "conflict_count": conflicts,
            "untyped_failure_count": untyped,
            "barrier_ok": barrier_ok,
        },
        "cases": cases,
        "cases_ok": cases_ok,
    });
    std::fs::write(
        out.join("world_kb_proof.json"),
        serde_json::to_string_pretty(&evidence).expect("serialize evidence"),
    )
    .expect("write evidence");

    let _ = daemon.kill();
    let _ = daemon.wait();

    if cli_degraded {
        eprintln!(
            "CLI binary at {} lacks the direct basic-cli path (P1-T3); degrading with exit {CLI_DEGRADED_EXIT}",
            cli_bin.display()
        );
        std::process::exit(CLI_DEGRADED_EXIT);
    }

    if !barrier_ok {
        eprintln!(
            "barrier round failed: expected one winner and {} typed conflicts across {} contenders; got winners={} conflicts={} untyped={} http=[{}, {}] cli={:?} cli_conflict_typed={} cli_transport_clean={} sampler_ok={}",
            contenders - 1,
            contenders,
            winners,
            conflicts,
            untyped,
            round.http_status_a,
            round.http_status_b,
            round.cli_code,
            round.cli_conflict_typed,
            round.cli_transport_clean,
            sampler_ok,
        );
        std::process::exit(1);
    }

    if !cases_ok {
        eprintln!("named coexistence/recovery cases failed: {}", cases);
        std::process::exit(1);
    }

    println!(
        "world_kb_proof complete for scenario={scenario} out={}",
        out.display()
    );
}
