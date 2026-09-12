//! P1 proof driver: concurrent HTTP/CLI World KB patch scenario.
//!
//! Spawns the current HTTP host (`nexus42 daemon-run`), waits for
//! `/v1/daemon/runtime/health`, then runs a barrier-synchronised stale-version
//! write pair. The CLI leg uses `--cli-bin` when provided; when the binary
//! lacks the `basic-cli` direct path the driver records degradation and exits
//! 78 after the HTTP leg completes (full dual-leg proof lands in P1-T3).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use nexus_local_db::writer_protocol::{GuardedPoolOptions, init_engine_pool};
use sqlx::SqlitePool;
use tempfile::TempDir;

const CREATOR: &str = "proof_creator";
const SLUG: &str = "default";
const WORLD_ID: &str = "wld_proof";
const ENTITY_ID: &str = "kb_aabbcc01";
const CLI_DEGRADED_EXIT: i32 = 78;

struct ProofHome {
    _tmp: TempDir,
    user_home: PathBuf,
    daemon_url: String,
}

fn reserve_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve port")
        .local_addr()
        .expect("local addr")
        .port()
}

async fn seed_world(pool: &SqlitePool, world_id: &str, owner: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, 'Proof', 'active', datetime('now'), '{}')",
    )
    .bind(owner)
    .execute(pool)
    .await
    .expect("seed creator");
    sqlx::query(
        "INSERT INTO narrative_worlds (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) VALUES (?, 'wrk', ?, 'Proof', 'proof', 'active', 'private', 'manual', '{}')",
    )
    .bind(world_id)
    .bind(owner)
    .execute(pool)
    .await
    .expect("seed world");
}

async fn seed_entity(pool: &SqlitePool, entity_id: &str, world_id: &str) {
    sqlx::query(
        "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, revision, modules_json, created_at, updated_at) VALUES (?, ?, 'character', 'Hero', 'confirmed', 0, NULL, datetime('now'), datetime('now'))",
    )
    .bind(entity_id)
    .bind(world_id)
    .execute(pool)
    .await
    .expect("seed entity");
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

    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, SLUG);
    let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .expect("engine pool");
    let pool = guarded.clone_pool();
    seed_world(&pool, WORLD_ID, CREATOR).await;
    seed_entity(&pool, ENTITY_ID, WORLD_ID).await;
    pool.close().await;

    ProofHome {
        _tmp: tmp,
        user_home,
        daemon_url,
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

fn http_get(host: &str, port: u16, path: &str) -> String {
    let mut stream =
        std::net::TcpStream::connect((host, port)).expect("tcp connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .expect("write GET");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read GET response");
    String::from_utf8_lossy(&buf).into_owned()
}

fn http_post_json(host: &str, port: u16, path: &str, body: &str) -> (u16, String) {
    let mut stream =
        std::net::TcpStream::connect((host, port)).expect("tcp connect for POST");
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
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read POST response");
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, text)
}

fn wait_healthy(port: u16, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let health = http_get("127.0.0.1", port, "/v1/daemon/runtime/health");
        if health.contains("HTTP/1.1 200") {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon health check timed out on port {port}");
}

fn cli_supports_direct_path(cli_bin: &Path) -> bool {
    Command::new(cli_bin)
        .args(["creator", "world", "kb", "entity", "patch", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn patch_body(title: &str) -> String {
    serde_json::json!({
        "entity_id": ENTITY_ID,
        "expected_version": 0,
        "patch": { "title": title }
    })
    .to_string()
}

struct BarrierRound {
    http_status_a: u16,
    http_status_b: u16,
    cli_status: Option<i32>,
    cli_degraded: bool,
}

fn run_barrier_round(
    port: u16,
    home: &Path,
    cli_bin: Option<&Path>,
) -> (BarrierRound, bool) {
    let barrier = Arc::new(Barrier::new(3));
    let path = format!("/v1/daemon/worlds/{WORLD_ID}/kb/patch-entity");
    let body_a = patch_body("Barrier A");
    let body_b = patch_body("Barrier B");

    let http_a = {
        let barrier = Arc::clone(&barrier);
        let path = path.clone();
        let body = body_a.clone();
        std::thread::spawn(move || {
            barrier.wait();
            http_post_json("127.0.0.1", port, &path, &body)
        })
    };
    let http_b = {
        let barrier = Arc::clone(&barrier);
        let path = path.clone();
        let body = body_b.clone();
        std::thread::spawn(move || {
            barrier.wait();
            http_post_json("127.0.0.1", port, &path, &body)
        })
    };

    let (cli_handle, cli_degraded) = if let Some(cli) = cli_bin {
        if !cli_supports_direct_path(cli) {
            barrier.wait();
            (None, true)
        } else {
            let barrier = Arc::clone(&barrier);
            let cli = cli.to_path_buf();
            let home = home.to_path_buf();
            let handle = std::thread::spawn(move || {
                barrier.wait();
                Command::new(&cli)
                    .args([
                        "creator",
                        "world",
                        "kb",
                        "entity",
                        "patch",
                        "--world-id",
                        WORLD_ID,
                        "--entity-id",
                        ENTITY_ID,
                        "--expected-version",
                        "0",
                        "--title",
                        "Barrier CLI",
                    ])
                    .env("HOME", &home)
                    .output()
            });
            (Some(handle), false)
        }
    } else {
        barrier.wait();
        (None, false)
    };

    let (status_a, _) = http_a.join().expect("http_a thread");
    let (status_b, _) = http_b.join().expect("http_b thread");
    let cli_status = cli_handle.map(|h| {
        h.join()
            .expect("cli thread")
            .unwrap()
            .status
            .code()
            .unwrap_or(-1)
    });

    (
        BarrierRound {
            http_status_a: status_a,
            http_status_b: status_b,
            cli_status,
            cli_degraded,
        },
        cli_degraded,
    )
}

#[tokio::main]
async fn main() {
    let mut scenario = String::new();
    let mut cli_bin: Option<PathBuf> = None;
    let mut http_bin: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let args: Vec<String> = std::env::args().collect();
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

    let out = out_dir.unwrap_or_else(|| PathBuf::from("evidence"));
    std::fs::create_dir_all(&out).expect("create evidence dir");

    let port = reserve_port();
    let home = prepare_home(port).await;
    let mut daemon = spawn_daemon(&http_bin, &home.user_home, port);
    wait_healthy(port, Duration::from_secs(30));

    let (round, cli_degraded) = run_barrier_round(port, &home.user_home, cli_bin.as_deref());

    let statuses = [round.http_status_a, round.http_status_b];
    let winners = statuses.iter().filter(|s| **s == 200).count();
    let losers_409 = statuses.iter().filter(|s| **s == 409).count();
    let http_ok = winners == 1 && losers_409 == 1;

    let evidence = serde_json::json!({
        "scenario": scenario,
        "http_bin": http_bin.display().to_string(),
        "cli_bin": cli_bin.as_ref().map(|p| p.display().to_string()),
        "daemon_url": home.daemon_url,
        "health_path": "/v1/daemon/runtime/health",
        "barrier_round": {
            "http_status_a": round.http_status_a,
            "http_status_b": round.http_status_b,
            "cli_status": round.cli_status,
            "cli_degraded": round.cli_degraded,
            "expected_http_winner_count": 1,
            "expected_http_conflict_count": 1,
            "http_ok": http_ok,
        },
    });
    std::fs::write(
        out.join("world_kb_proof.json"),
        serde_json::to_string_pretty(&evidence).expect("serialize evidence"),
    )
    .expect("write evidence");

    let _ = daemon.kill();
    let _ = daemon.wait();

    if !http_ok {
        eprintln!(
            "barrier round failed: expected one HTTP 200 and one HTTP 409, got {:?}",
            statuses
        );
        std::process::exit(1);
    }

    if cli_degraded {
        eprintln!(
            "CLI binary at {} lacks the direct basic-cli path (P1-T3); HTTP barrier leg passed — degrading with exit {CLI_DEGRADED_EXIT}",
            cli_bin
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
        std::process::exit(CLI_DEGRADED_EXIT);
    }

    println!(
        "world_kb_proof complete for scenario={scenario} out={}",
        out.display()
    );
}
