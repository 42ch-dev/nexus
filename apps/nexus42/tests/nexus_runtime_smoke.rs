//! P2 T1 — spawned-process smoke test for the headless `nexus-runtime` bin.
//!
//! Builds the REAL binary (`env!("CARGO_BIN_EXE_nexus-runtime")`) and runs
//! it against a hermetic temp `NEXUS42_HOME`, proving the headless boot
//! contract (P2 spec § Subsystem profile / AC-2):
//!
//! 1. **stdout readiness** — the process prints the readiness block
//!    (`peer_id` / `host_id` / `listen` / served invokes) on stdout;
//! 2. **serves Connect** — the `runtime_smoke_probe` example (a reference
//!    spoke-connect peer, built with the same `connect-host` feature)
//!    completes the signed-hello handshake against the spawned process and
//!    reads the N-C1 manifest (`extensions.nexus.served_ops` =
//!    upsert/promote/relate — the P1 invoke surface, honest by the P1
//!    machine-check);
//! 3. **no HTTP/SPA listener** — the daemon HTTP port refuses connections
//!    (the daemon router never boots; in release the SPA fallback is
//!    additionally compiled out by `web-embed` OFF).
//!
//! Compiled only with `--features connect-host` (same gate as the bin);
//! the test itself only spawns processes, so the default test graph stays
//! libp2p-free — the dialing lives in the probe example.

#![cfg(feature = "connect-host")]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Peer id of the probe's fixed Ed25519 seed (7) — deterministic, derived
/// once via `runtime_smoke_probe --print-peer-only` (libp2p peer-id
/// derivation is stable). Allowlisted in the seeded `allowlist.json`
/// BEFORE the host boots (fail-closed allowlist: the handshake only
/// succeeds for listed peers).
const DIALER_PEER_ID: &str = "12D3KooWRawPbxPtP1eZaJpumGnyWX2DcUyd3RQnydr3eAto4Az7";

/// Canonical nexus home dir name (the layout join used by every home
/// helper; kept literal so the test does not need the layout crate).
const NEXUS_DIR: &str = ".nexus42";

/// How long the spawned runtime may take to print readiness.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// Seed a hermetic `~/.nexus42` home for the spawned runtime: the CLI
/// config keys the boot resolves the active workspace from (same keys the
/// creator app writes), plus the Connect allowlist holding the fixed-seed
/// probe peer.
fn seed_home(tmp: &Path) {
    let nexus = tmp.join(NEXUS_DIR);
    std::fs::create_dir_all(&nexus).expect("create nexus home");

    std::fs::write(
        nexus.join("config.toml"),
        "active_creator_id = \"ctr_smoke\"\n\
         [active_workspace_slug_by_creator]\n\
         ctr_smoke = \"default\"\n",
    )
    .expect("write config.toml");

    let connect_dir = nexus.join("connect");
    std::fs::create_dir_all(&connect_dir).expect("create connect dir");
    std::fs::write(
        connect_dir.join("allowlist.json"),
        format!("{{\"peer_ids\": [\"{DIALER_PEER_ID}\"]}}"),
    )
    .expect("write allowlist.json");
}

/// The `runtime_smoke_probe` example binary — compiled by cargo (with
/// `connect-host` on) when the test target builds; resolved through the
/// standard workspace target layout.
fn probe_binary() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| manifest_dir.join("../../target"), PathBuf::from);
    let exe = if cfg!(windows) {
        "runtime_smoke_probe.exe"
    } else {
        "runtime_smoke_probe"
    };
    target.join(profile).join("examples").join(exe)
}

/// Spawn the real `nexus-runtime` binary against `home` (via the
/// `NEXUS42_HOME` override) and wait for the stdout readiness block.
/// Returns the child (still running), the readiness lines, and the printed
/// `listen:` multiaddrs.
fn spawn_runtime(home: &Path) -> (Child, Vec<String>, Vec<String>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_nexus-runtime"))
        .env("NEXUS42_HOME", home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nexus-runtime");

    // Drain both pipes through channels so the child can never block on a
    // full pipe; stderr lines are kept for failure diagnostics.
    let stdout = child.stdout.take().expect("runtime stdout");
    let stderr = child.stderr.take().expect("runtime stderr");
    let (line_tx, line_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    let (stderr_tx, stderr_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if stderr_tx.send(line).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + READY_TIMEOUT;
    let mut ready_lines = Vec::new();
    let mut listen_addrs = Vec::new();
    while Instant::now() < deadline {
        match line_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                ready_lines.push(line.clone());
                if let Some(rest) = line.trim().strip_prefix("listen:") {
                    listen_addrs.push(rest.trim().to_string());
                }
                // The readiness block ends with the Ctrl-C hint line.
                if !listen_addrs.is_empty() && line.contains("press Ctrl-C to stop") {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if listen_addrs.is_empty() {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "nexus-runtime did not print a `listen:` readiness line.\n\
             stdout so far:\n{}\nstderr:\n{}",
            ready_lines.join("\n"),
            stderr_rx.try_iter().collect::<Vec<_>>().join("\n")
        );
    }

    // Keep the stderr drain alive for the caller's lifetime (detached).
    std::mem::forget(stderr_rx);
    (child, ready_lines, listen_addrs)
}

/// RAII guard that kills and reaps the spawned runtime child on drop —
/// including panic unwind — so a failed assertion never orphans the
/// process (the pre-fix test only killed on the success path).
struct RuntimeGuard {
    child: Option<Child>,
}

impl RuntimeGuard {
    const fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Assert the spawned runtime has NO HTTP listener: every TCP listener of
/// the runtime process must be one of the Connect listen multiaddrs it
/// printed. (A well-known-port probe would false-fail under the spec's
/// coexistence model — a creator-facing `nexus42` daemon may legitimately
/// occupy the daemon port while the runtime runs; the guarantee is about
/// THIS process, which never boots the daemon router.)
///
/// Implemented with `lsof` on unix (present on macOS + Linux CI runners);
/// on Windows the check is a no-op — the property is structural (the
/// headless boot binds only `SpokeConnectNode`; there is no axum bind in
/// the path), and the T2 Windows CI leg smoke-tests `--version`.
fn assert_no_http_listener(child_pid: u32, listen_addrs: &[String]) {
    #[cfg(unix)]
    {
        let ports: Vec<u16> = listen_addrs
            .iter()
            .filter_map(|addr| addr.rsplit('/').next())
            .filter_map(|port| port.parse().ok())
            .collect();
        let out = Command::new("lsof")
            .args([
                "-nP",
                "-iTCP",
                "-sTCP:LISTEN",
                "-a",
                "-p",
                &child_pid.to_string(),
            ])
            .output()
            .expect("run lsof");
        assert!(
            out.status.success(),
            "lsof failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().skip(1) {
            let Some(port) = lsof_line_port(line) else {
                continue;
            };
            assert!(
                ports.contains(&port),
                "runtime process holds an unexpected TCP listener on port {port} \
                 (daemon HTTP / SPA listener?):\n{text}"
            );
        }
    }
    #[cfg(not(unix))]
    {
        // Structural no-op — see the doc comment.
        let _ = (child_pid, listen_addrs);
    }
}

/// Extract the listening port from a `lsof -sTCP:LISTEN` NAME column line.
///
/// Real lines end with the state suffix — `TCP 127.0.0.1:62488 (LISTEN)` —
/// so the port token is the SECOND-TO-LAST whitespace token
/// (`split_whitespace().rev().nth(1)` — `str` has no `rsplit_whitespace`),
/// NOT `.last()` (which is always `(LISTEN)` and never parses as a port —
/// the pre-fix parser skipped every line, making the no-HTTP assertion
/// vacuous).
fn lsof_line_port(line: &str) -> Option<u16> {
    line.split_whitespace()
        .rev()
        .nth(1)
        .and_then(|addr| addr.rsplit(':').next())
        .and_then(|port| port.parse::<u16>().ok())
}

#[test]
fn lsof_line_port_parses_real_listener_lines() {
    // Real shapes from `lsof -nP -iTCP -sTCP:LISTEN -a -p <pid>`: IPv4
    // loopback (observed: `TCP 127.0.0.1:62488 (LISTEN)`), wildcard, and
    // bracketed IPv6.
    assert_eq!(
        lsof_line_port(
            "nexus-runtime 5469 user 14u IPv4 0x8f7d2f5b5e0b4c8f 0t0 TCP 127.0.0.1:62488 (LISTEN)"
        ),
        Some(62488)
    );
    assert_eq!(lsof_line_port("TCP *:62086 (LISTEN)"), Some(62086));
    assert_eq!(lsof_line_port("TCP [::1]:62086 (LISTEN)"), Some(62086));

    // Regression guard: the vacuous pre-fix parser (`.last()` token, i.e.
    // always `(LISTEN)`) extracts no port from ANY real listener line, so
    // it could never fail the no-HTTP assertion. If this guard trips, the
    // assertion became vacuous again.
    for line in [
        "TCP 127.0.0.1:62488 (LISTEN)",
        "TCP *:62086 (LISTEN)",
        "TCP [::1]:62086 (LISTEN)",
    ] {
        assert_eq!(
            line.split_whitespace()
                .last()
                .and_then(|tok| tok.rsplit(':').next())
                .and_then(|p| p.parse::<u16>().ok()),
            None,
            "pre-fix parser must not extract a port from {line:?}"
        );
    }

    // Malformed / non-listen lines are skipped, never mis-parsed.
    assert_eq!(lsof_line_port("TCP 127.0.0.1:62488"), None);
    assert_eq!(lsof_line_port(""), None);
}

#[test]
fn version_flag_prints_crate_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_nexus-runtime"))
        .arg("--version")
        .output()
        .expect("run nexus-runtime --version");
    assert!(out.status.success(), "status: {:?}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("nexus-runtime "),
        "unexpected --version output: {text:?}"
    );
    assert!(
        text.contains(env!("CARGO_PKG_VERSION")),
        "--version must print the crate version: {text:?}"
    );
}

#[test]
fn headless_runtime_prints_readiness_serves_connect_and_has_no_http_listener() {
    let tmp = tempfile::tempdir().expect("temp dir");
    seed_home(tmp.path());

    // Boot the real binary against the temp home. The guard kills the
    // child on EVERY exit path — success or panic — never orphaning it.
    let (child, ready_lines, listen_addrs) = spawn_runtime(tmp.path());
    let runtime_pid = child.id();
    let _guard = RuntimeGuard::new(child);

    // 1. Readiness block: the required lines are present on stdout.
    let ready = ready_lines.join("\n");
    for expected in [
        "Connect Host (N-C1) ready",
        "peer_id:",
        "host_id:",
        "allowlisted peers: 1",
        "upsert/promote/relate served",
    ] {
        assert!(
            ready.contains(expected),
            "readiness block missing {expected:?}:\n{ready}"
        );
    }

    // 2. No HTTP/SPA listener: every TCP listener of the runtime process
    //    is one of its printed Connect listen addrs.
    assert_no_http_listener(runtime_pid, &listen_addrs);

    // 3. The reference probe peer dials the host and completes the
    //    signed-hello handshake; the N-C1 manifest advertises exactly the
    //    served write ops and the session stays usable.
    let host_peer = ready_lines
        .iter()
        .find_map(|line| line.trim().strip_prefix("peer_id:"))
        .expect("peer_id line")
        .trim();

    let probe = probe_binary();
    assert!(
        probe.exists(),
        "probe example not built — expected at {}",
        probe.display()
    );
    let out = Command::new(&probe)
        .args(["--addr", &listen_addrs[0], "--host-peer", host_peer])
        .output()
        .expect("run runtime_smoke_probe");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let status = out.status;
    assert!(
        status.success(),
        "probe failed (status {status})\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    for expected in ["DIAL_OK", "SERVED_OPS=upsert,promote,relate", "SESSION_OK"] {
        assert!(
            stdout.contains(expected),
            "probe output missing {expected:?}:\n{stdout}\nstderr:\n{stderr}"
        );
    }
    // `_guard` drops here (and on any panic unwind above): the child is
    // killed and reaped on every path.
}
