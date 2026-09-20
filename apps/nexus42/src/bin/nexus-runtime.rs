//! `nexus-runtime` — headless Connect runtime (V1.153 P2, DF-73).
//!
//! Standalone binary serving ONLY the Connect Host invoke surface — the
//! same served set as `nexus42 connect start` (world-scoped `upsert` /
//! `promote` / `relate` / `check` / `assemble`, World/module/Actor-gated
//! `compute` over the host-local `~/.nexus42/modules/` store and the
//! process-wide compiled-module cache, and the host-level
//! `tools.nexus.list_observed_peers` / `tools.nexus.list_modules` reads)
//! for the partner/integrator channel. Boots:
//! PATH enrichment, the shared `~/.nexus42` home layout, config load,
//! active-workspace `SQLite` open (`DbPool`, WAL), ONE per-process
//! `NexusAdapter`, and the Connect host with the full invoke dispatch
//! handler — then blocks on SIGINT. Liveness = **stdout readiness only**
//! (no HTTP health endpoint).
//!
//! `nexus_daemon_runtime::boot::run_daemon` is **never called**: the daemon
//! HTTP data router, embedded `apps/web` SPA, Setup/Canvas/Control Room
//! routes, ACP/agent-host subsystem, and schedule/worker supervision never
//! boot in this process (P2 spec § Subsystem profile). The embedded SPA
//! bytes are additionally excluded at COMPILE time: the distributed
//! artifact is built with
//! `cargo build --release --bin nexus-runtime --no-default-features
//! --features connect-host` (the `web-embed` feature is OFF).
//!
//! Final-graph module requirements (P2-T13 cohort collapse): `connect-host`
//! MUST keep the spoke-adapter **`compute`** feature
//! (`nexus-spoke-adapter/compute` → `nexus-wasm-host`, which compiles the
//! embedded modules and pulls `nexus-module-manifest`), the adapter's own
//! `nexus-knowledge` / `nexus-local-db` / `nexus-home-layout` edges, and
//! the Connect transport (`spoke-connect` / `spoke-schemas` / `libp2p`).
//! The peer `compute` op, the host-local `~/.nexus42/modules/` module store
//! and the process-wide compiled-module cache are retained production
//! behavior: dropping compute (or its feature) to obtain a smaller graph is
//! prohibited. That graph requires the `wasm32-unknown-unknown` target —
//! `nexus-wasm-host`'s build script compiles the embedded modules from
//! source, and `apps/nexus42/build.rs` probes the target for the
//! embedded-module compute interop tests.
//!
//! Home resolution: `--home` > `$NEXUS42_HOME` > the user home. The home
//! value is the HOME DIR itself — the PARENT of the `.nexus42` layout dir
//! (e.g. `/home/me` → `/home/me/.nexus42`), not the layout dir. The home is
//! shared with the creator-facing `nexus42` app; shared-DB write access is
//! governed by `SQLite` `WAL` (not the per-Work `runtime_lock`), per the P2
//! spec § Coexistence.

use clap::Parser;
use nexus42::commands::connect;
use nexus42::errors::Result;
use std::path::{Path, PathBuf};

/// Default listen multiaddr when `--listen` is not given — loopback only,
/// identical to `nexus42 connect start`.
const DEFAULT_LISTEN: &str = "/ip4/127.0.0.1/tcp/0";

/// Headless Connect runtime CLI (mirrors the `nexus42 connect start`
/// surface; spec-locked in the P2 spec § CLI surface).
#[derive(Debug, Parser)]
#[command(
    name = "nexus-runtime",
    version,
    about = "Nexus headless Connect runtime (Connect Host invoke surface)",
    long_about = "Headless Connect runtime: serves the Connect Host invoke \
                  surface (upsert/promote/relate/\
                  check/assemble/compute, plus the host-level \
                  tools.nexus.list_observed_peers and \
                  tools.nexus.list_modules reads) over spoke-connect \
                  against the shared ~/.nexus42 home. No daemon HTTP \
                  router, no embedded Web UI, no Setup/Canvas/Control Room."
)]
struct RuntimeCli {
    /// Listen multiaddr (repeatable; default `/ip4/127.0.0.1/tcp/0`).
    #[arg(long, value_name = "MULTIADDR", default_value = DEFAULT_LISTEN)]
    listen: Vec<String>,

    /// Peer IDs to allowlist for this run (repeatable; unioned with
    /// `~/.nexus42/connect/allowlist.json`).
    #[arg(long = "allow-peer", value_name = "PEER_ID")]
    allow_peer: Vec<String>,

    /// Nexus home override (default: `$NEXUS42_HOME`, else the user home).
    ///
    /// The home is the parent of the `.nexus42` layout dir: `/foo` →
    /// `/foo/.nexus42`.
    #[arg(long, value_name = "PATH")]
    home: Option<PathBuf>,
}

fn main() {
    let cli = RuntimeCli::parse();

    // PATH enrichment before Tokio starts (GUI-launched sidecars inherit a
    // minimal PATH; same Class-B rule as the nexus42 CLI main).
    // v1.193 P2-T2: the helper lives with the provider-discovery owner.
    nexus_agent_host::discovery::path_enrichment::apply_process_path_enrichment();

    let home = resolve_home(cli.home.as_deref());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime");
    if let Err(e) = runtime.block_on(boot(&home, &cli.allow_peer, &cli.listen)) {
        eprintln!("nexus-runtime: {e}");
        std::process::exit(1);
    }
}

/// Resolve the nexus home: `--home` > `$NEXUS42_HOME` > the user home.
#[must_use]
fn resolve_home(cli_home: Option<&Path>) -> PathBuf {
    let overridden = cli_home
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("NEXUS42_HOME").map(PathBuf::from));
    overridden.map_or_else(
        || match nexus42::config::user_home_dir() {
            Ok(home) => home,
            Err(e) => {
                eprintln!("nexus-runtime: cannot resolve user home: {e}");
                std::process::exit(1);
            }
        },
        |home| {
            // The shared config/workspace resolution in this crate reads
            // `dirs::home_dir()` directly (there is no central home
            // indirection), so an override re-points the platform home env
            // BEFORE any downstream read — the same mechanism
            // `testutil::isolated_home` uses in-process. Safe here because
            // `main` runs single-threaded before the Tokio runtime starts
            // (same pre-runtime env discipline as
            // `apply_process_path_enrichment`).
            let home = if home.is_absolute() {
                home
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(&home))
                    .unwrap_or(home)
            };
            #[cfg(unix)]
            std::env::set_var("HOME", &home);
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", &home);
            home
        },
    )
}

/// The headless boot: shared home layout + the exact `connect start` host
/// assembly ([`connect::build_host_config`] — the full invoke dispatch
/// handler, compute included) + node start + stdout readiness. `run_daemon`
/// is never called.
///
/// # Errors
/// [`CliError`] on layout/identity/allowlist/workspace-DB/node failures.
async fn boot(home: &Path, allow_peer: &[String], listen: &[String]) -> Result<()> {
    // Shared `~/.nexus42` skeleton (idempotent; no-op on an existing home).
    let nexus_home = nexus_home_layout::nexus_root_from_home(home);
    nexus_home_layout::ensure_system_layout(&nexus_home).map_err(nexus42::errors::CliError::Io)?;

    // The full host boot shared with `nexus42 connect start`:
    // persisted identity + device-id host_id + allowlist (fail-closed) +
    // honest manifest + active-workspace WAL pool + per-process
    // NexusAdapter + the invoke dispatch handler (writes, reads, compute,
    // host-level tool reads).
    let (config, host_id, allowlist_len, _adapter) =
        connect::build_host_config(home, allow_peer, listen, None).await?;

    let node = spoke_connect::SpokeConnectNode::start(config)
        .await
        .map_err(|e| {
            nexus42::errors::CliError::Config(format!("connect node start failed: {e}"))
        })?;

    // Liveness = stdout readiness (the ONLY liveness surface — no HTTP
    // health endpoint; the daemon router never boots in this process).
    println!("nexus-runtime: Connect Host (N-C2 E2) ready");
    println!("  peer_id: {}", node.local_peer_id());
    println!("  host_id: {host_id}");
    for addr in node.listen_addrs() {
        println!("  listen: {addr}");
    }
    println!(
        "  allowlisted peers: {allowlist_len} (fail-closed; add via allowlist.json or --allow-peer)"
    );
    println!(
        "  invokes: upsert/promote/relate/check/assemble/compute served (world+module scoped) \
         plus tools.nexus.list_observed_peers / tools.nexus.list_modules (host-level reads); \
         project/unknown refused (op_unsupported)"
    );
    println!("  press Ctrl-C to stop");

    tokio::signal::ctrl_c().await.map_err(|e| {
        nexus42::errors::CliError::Other(format!("failed to listen for SIGINT: {e}"))
    })?;
    println!("nexus-runtime: shutting down");
    node.shutdown().await.map_err(|e| {
        nexus42::errors::CliError::Other(format!("connect node shutdown failed: {e}"))
    })?;
    Ok(())
}
