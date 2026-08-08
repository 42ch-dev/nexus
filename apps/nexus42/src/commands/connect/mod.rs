//! Connect Host commands (DF-72 N-C0 → N-C2 read half) — opt-in feature
//! `connect-host`.
//!
//! `nexus42 connect start` runs a `spoke-connect` node in a **separate OS
//! process** (architect lock Q7): signed-hello handshake, allowlist,
//! honest `HostCapabilityManifest`, and — since V1.153 P1 (N-C1) — an
//! inbound **invoke dispatcher** ([`invoke`]) backed by a per-process
//! `NexusAdapter` over the active workspace DB. N-C2 (V1.154 P1) extends
//! the served surface with the read half (`check` / `assemble`); every op
//! the host does not serve (`compute` / `project` / unknown) is refused
//! with `op_unsupported` (the N-C0 refusal contract extends);
//! non-allowlisted peers never reach the handler (handshake).
//!
//! V1.155 P0 (N-C3 multi-host production): `connect peers list` reads the
//! observed-peer store through the adapter boundary
//! (`NexusAdapter::list_observed_peer_hosts` — host_id, capabilities,
//! last_seen; empty store → `no peers observed`).
//!
//! Topology rules (product draft `fl-r-connect-host-foundation.md` §2.1/§2.6):
//! - mDNS is **never** enabled (`spoke-connect/mdns` not in the feature set).
//! - `nexus42 daemon start` MUST NOT open a Connect listener — only
//!   `connect start` does (feature-on binary still keeps the daemon unchanged).
//! - Identity + allowlist persist under `~/.nexus42/connect/` (home-layout
//!   path helpers); missing allowlist ⇒ fail-closed (rejects all peers).
//! - N-C1 coexistence with a co-running daemon/CLI is governed by the
//!   `SQLite` WAL mode (1 writer + N readers, `DbPool` busy timeout) — the
//!   per-Work `nexus-local-db` `runtime_lock` is daemon-internal and the
//!   Connect invoke path never acquires it (P1 spec § Process model,
//!   corrected). Same-entry write correctness is the orchestrators' OCC CAS.

pub mod allowlist;
pub mod identity;
// V1.153 P1 N-C1 → V1.154 P0 T2: the session-peer `InvokeHandlerV2`
// closure (architect-locked home; identity = the authenticated session peer).
pub mod invoke;

use crate::errors::{CliError, Result};
use clap::Subcommand;
use libp2p::Multiaddr;
use nexus_home_layout::device_id::get_or_create_device_id;
use nexus_spoke_adapter::manifest::build_connect_hello_manifest;
use nexus_spoke_adapter::{NexusAdapter, ObservedPeerHost, SpokeResult};
use spoke_connect::{parse_multiaddr, ConnectConfig, SpokeConnectNode};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Default listen multiaddr when `--listen` is not given (loopback only —
/// binding a routable interface is an explicit operator choice, N-C0 §5.3).
const DEFAULT_LISTEN: &str = "/ip4/127.0.0.1/tcp/0";

/// Connect Host subcommands.
#[derive(Debug, Subcommand)]
pub enum ConnectCommand {
    /// Start the Connect Host node (N-C2 read half: handshake + manifest +
    /// world-scoped upsert/promote/relate/check/assemble invoke dispatch;
    /// compute/project/unknown ops refused)
    Start {
        /// Peer IDs to allowlist for this run (repeatable; unioned with
        /// `~/.nexus42/connect/allowlist.json`).
        #[arg(long = "allow-peer", value_name = "PEER_ID")]
        allow_peer: Vec<String>,
        /// Listen multiaddr (repeatable; default `/ip4/127.0.0.1/tcp/0`).
        #[arg(long, value_name = "MULTIADDR", default_value = DEFAULT_LISTEN)]
        listen: Vec<String>,
    },
    /// Inspect observed peer hosts (N-C3 multi-host production).
    Peers {
        #[command(subcommand)]
        command: PeersCommand,
    },
}

/// `connect peers` subcommands.
#[derive(Debug, Subcommand)]
pub enum PeersCommand {
    /// List observed peer hosts — host_id, capabilities, last_seen.
    ///
    /// Reads the adapter's observed-peer store (the `peer_hosts` table of
    /// the active workspace DB): every peer this host has dialed and
    /// recorded at `connect()` return (manifest-backed observations only —
    /// inbound-only peers are not recorded, spec lock #1 fallback). Empty
    /// store → `no peers observed`. Ordering: `last_seen` DESC, `host_id`
    /// ASC (the storage ordering contract).
    List,
}

/// Run a Connect Host command.
///
/// # Errors
/// Returns a [`CliError`] when identity/allowlist I/O, manifest building, or
/// node startup fails.
pub async fn run(command: ConnectCommand) -> Result<()> {
    match command {
        ConnectCommand::Start { allow_peer, listen } => start(allow_peer, listen).await,
        ConnectCommand::Peers { command } => run_peers(command).await,
    }
}

/// Run a `connect peers` command.
///
/// # Errors
/// Returns a [`CliError`] when the workspace DB open or the adapter read
/// fails (fail-closed: an adapter reject is surfaced, never swallowed as an
/// empty list).
pub async fn run_peers(command: PeersCommand) -> Result<()> {
    match command {
        PeersCommand::List => peers_list().await,
    }
}

/// Run `connect peers list`.
///
/// Opens the active-workspace DB pool (the same WAL pool the adapter runs
/// against) and reads the observed-peer store through the adapter boundary
/// ([`NexusAdapter::list_observed_peer_hosts`] — NOT raw `peer_hosts`
/// columns; the shared rows→typed-manifest parse path means a corrupt
/// stored row surfaces as `InternalError`, the port contract).
///
/// Output: one row per observed peer host — `host_id`, `capabilities`
/// (typed `manifest.capabilities`, the single honest spoke-shaped view),
/// `last_seen` (the nexus-local observation timestamp, the only
/// non-manifest field). Ordering preserved from the storage contract:
/// `last_seen` DESC, `host_id` ASC. Empty store → `no peers observed`.
///
/// # Errors
/// [`CliError::Config`] when the active workspace cannot be resolved, or
/// [`CliError::Other`] when the DB open or the adapter read is rejected.
async fn peers_list() -> Result<()> {
    let pool = open_workspace_pool(None).await?;
    let adapter = NexusAdapter::new(pool);
    let peers = match adapter.list_observed_peer_hosts().await {
        SpokeResult::Ok(peers) => peers,
        SpokeResult::Reject(reject) => {
            return Err(CliError::Other(format!(
                "observed peer hosts read rejected: {}",
                reject.message
            )));
        }
    };

    for line in render_peer_lines(&peers) {
        println!("{line}");
    }
    Ok(())
}

/// Render the `connect peers list` output lines (pure, testable).
///
/// Empty store → the single line `no peers observed`. Otherwise a header
/// row (`HOST_ID`, `CAPABILITIES`, `LAST_SEEN`) followed by one row per
/// observed peer host: `host_id` (typed manifest), `capabilities` (typed
/// `manifest.capabilities` joined with `", "` — the single honest
/// spoke-shaped view), `last_seen` (the nexus-local observation timestamp,
/// the only non-manifest field). Callers print each line verbatim.
fn render_peer_lines(peers: &[ObservedPeerHost]) -> Vec<String> {
    if peers.is_empty() {
        return vec!["no peers observed".to_string()];
    }
    let mut lines = Vec::with_capacity(peers.len() + 1);
    lines.push(format!(
        "{:<40} {:<32} {}",
        "HOST_ID", "CAPABILITIES", "LAST_SEEN"
    ));
    for peer in peers {
        lines.push(format!(
            "{:<40} {:<32} {}",
            peer.manifest.host_id.as_str(),
            peer.manifest.capabilities.join(", "),
            peer.last_seen
        ));
    }
    lines
}

/// Run the full N-C1 `connect start` boot: the N-C0 assembly
/// ([`build_config`]) + the active-workspace DB open + the per-process
/// `NexusAdapter` + the [`invoke::build_handler`] wiring — exactly the
/// shared boot shape the P1 spec § Process model locks (shared with
/// `nexus-runtime` in P2). Every inbound op that is not served is answered
/// `op_unsupported` by the handler (N-C0 refusal contract extends).
async fn start(allow_peer: Vec<String>, listen: Vec<String>) -> Result<()> {
    // Raw home: the home-layout identity/allowlist helpers join `.nexus42`
    // themselves. The device-id resolution mirrors `host_manifest_port`
    // exactly so the Connect host_id is the SAME value HostManifestPort
    // advertises (single builder SSOT).
    let home = crate::config::user_home_dir()?;
    // N-C3 (V1.155 P0): the boot also returns the per-process adapter (the
    // peer-recording capability). `connect start` never dials — peers are
    // recorded by the dialing path at `connect()` return
    // ([`record_dialed_peer`]) — so the adapter is kept alive here (same
    // Arc the invoke handler captures) for the process lifetime.
    let (config, host_id, allowlist_len, _adapter) =
        build_host_config(&home, &allow_peer, &listen, None).await?;

    // 7. Start the node; block on the tokio runtime until SIGINT.
    let node = SpokeConnectNode::start(config)
        .await
        .map_err(|e| CliError::Config(format!("connect node start failed: {e}")))?;

    eprintln!("nexus42 connect start: Connect Host (N-C2 E2) listening");
    eprintln!("  peer_id: {}", node.local_peer_id());
    eprintln!("  host_id: {host_id}");
    for addr in node.listen_addrs() {
        eprintln!("  listen: {addr}");
    }
    eprintln!(
        "  allowlisted peers: {allowlist_len} (fail-closed; add via allowlist.json or --allow-peer)"
    );
    eprintln!(
        "  invokes: upsert/promote/relate/check/assemble/compute served (world+module scoped); \
         project/unknown refused (op_unsupported)"
    );
    eprintln!("  press Ctrl-C to stop");

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| CliError::Other(format!("failed to listen for SIGINT: {e}")))?;
    eprintln!("nexus42 connect start: shutting down");
    node.shutdown()
        .await
        .map_err(|e| CliError::Other(format!("connect node shutdown failed: {e}")))?;
    Ok(())
}

/// Assemble the architect-locked N-C0 `ConnectConfig` from the on-disk
/// state + CLI inputs (steps 1–6 of `start`). The N-C1 pieces (workspace DB
/// open + adapter + invoke handler) are added by [`build_host_config`], so
/// this function stays pure (no I/O beyond the identity/allowlist reads).
///
/// `home` is the **raw** user home (`$HOME`): identity/allowlist helpers
/// join `.nexus42` themselves. The device-id is resolved via
/// `get_or_create_device_id(home)` — the identical resolution
/// `host_manifest_port::resolve_device_id_from_standard_home` uses, so the
/// Connect `host_id` always equals the manifest's `host_id`.
///
/// Returns the config, the resolved `host_id` (for start-up logging), the
/// effective allowlist length, and the resolved `PeerScope` (consumed by the
/// N-C1 dispatch gate in [`invoke`]).
///
/// # Errors
/// [`CliError`] on identity/allowlist/manifest/listen failures — see the
/// per-step helpers.
fn build_config(
    home: &Path,
    allow_peer: &[String],
    listen: &[String],
) -> Result<(ConnectConfig, String, usize, allowlist::PeerScope)> {
    // 1. Identity: `~/.nexus42/connect/identity.key` (Ed25519, create-once 0600).
    let identity = identity::load_or_create_identity(home)?;

    // 2. host_id: installation device-id UUID — resolved exactly like
    //    `host_manifest_port::resolve_device_id_from_standard_home`.
    //    `get_or_create_device_id` takes the RAW home and joins `.nexus42`
    //    itself (canonical `~/.nexus42/device-id`; device_id_path contract).
    let host_id = get_or_create_device_id(home)
        .map_err(|e| CliError::Config(format!("device id unavailable: {e}")))?;

    // 3. Allowlist: file ∪ `--allow-peer*`; missing file ⇒ empty ⇒ fail-closed.
    //    `load` resolves the N-C1 `PeerScope` (per-peer world/op scope for
    //    the dispatch gate); the flat id set feeds the handshake allowlist.
    let peer_scope = allowlist::load(home, allow_peer)?;
    let peer_allowlist = peer_scope.peer_ids();
    let allowlist_len = peer_allowlist.len();

    // 4. Listen multiaddrs from `--listen` (default loopback ephemeral port).
    let listen_addrs = listen
        .iter()
        .map(|addr| {
            parse_multiaddr(addr)
                .map_err(|e| CliError::Config(format!("invalid --listen multiaddr {addr:?}: {e}")))
        })
        .collect::<Result<Vec<Multiaddr>>>()?;

    // 5. Manifest: shared builder → connect_hello wire type (JSON round-trip).
    let local_manifest = match build_connect_hello_manifest(&host_id) {
        SpokeResult::Ok(manifest) => manifest,
        SpokeResult::Reject(reject) => {
            return Err(CliError::Config(format!(
                "manifest build failed: {}",
                reject.message
            )));
        }
    };

    // 6. Architect-locked ConnectConfig (the invoke handler is installed by
    //    build_host_config once the workspace adapter exists).
    let config = ConnectConfig {
        identity,
        peer_allowlist,
        listen_addrs,
        local_manifest,
        handshake_timeout: None,
        invoke_handler: None,
        invoke_handler_v2: None,
        op_capability_requirements: HashMap::new(),
        trusted_issuers: Vec::new(),
        require_capability_token: false,
        capability_token_provider: None,
    };
    Ok((config, host_id, allowlist_len, peer_scope))
}

/// Full N-C1 host boot: [`build_config`] + workspace DB open + per-process
/// `NexusAdapter` + [`invoke::build_handler`] — the `connect start` shape.
///
/// DF-73: the `nexus-runtime` bin shares this boot; `run_daemon` is never
/// called.
///
/// `workspace_db` overrides the resolved active-workspace DB path (hermetic
/// tests); `None` resolves it by the daemon rules: active workspace from the
/// `~/.nexus42` `CliConfig` (`active_creator_id` +
/// `active_workspace_slug_by_creator` → `resolve_state_db_path`).
///
/// V1.155 P0 N-C3: the fourth return value is the per-process adapter Arc —
/// the recording capability (and the peer-store handle) the outbound
/// `connect()`-return wiring needs ([`record_dialed_peer`] /
/// `NexusAdapter::record_peer_manifest`). `connect start` / `nexus-runtime`
/// do not dial today, so the adapter flows to callers that do.
///
/// # Errors
/// [`CliError`] on N-C0 assembly, workspace resolution, or DB open failures.
pub async fn build_host_config(
    home: &Path,
    allow_peer: &[String],
    listen: &[String],
    workspace_db: Option<&Path>,
) -> Result<(ConnectConfig, String, usize, Arc<NexusAdapter<'static>>)> {
    let (mut config, host_id, allowlist_len, peer_scope) = build_config(home, allow_peer, listen)?;

    // N-C1: workspace DB open (WAL pool via the shared Schema initializer —
    // coexistence with a co-running daemon is WAL-governed, not
    // runtime_lock-governed; P1 spec § Process model) + the per-process
    // adapter singleton + the session-peer invoke dispatch handler
    // (spoke-connect 0.9.2 `invoke_handler_v2` — caller identity is the
    // authenticated session peer; the legacy `invoke_handler` is not
    // selected, clean cutover per spec §5.2).
    let pool = open_workspace_pool(workspace_db).await?;
    // P2: the adapter's ComputablePort resolves compute modules host-locally
    // from `~/.nexus42/modules/` (spec §2.1 — never peer-supplied bytes).
    let modules_dir = nexus_home_layout::user_modules_dir(home);
    let adapter = Arc::new(NexusAdapter::new(pool).with_user_modules_dir(modules_dir));
    config.invoke_handler_v2 = Some(invoke::build_handler(peer_scope, Arc::clone(&adapter)));

    Ok((config, host_id, allowlist_len, adapter))
}

/// N-C3 (V1.155 P0): record the dialed peer at `SpokeConnectNode::connect()`
/// return — the outbound observation point (iteration spec
/// `fl-r-w3-n-c3-multi-host.md` §Design lock #1).
///
/// `session.remote_manifest()` is the dialed peer's manifest as the
/// `connect_hello` wire type (spoke-connect 0.9.2, verified); it is
/// converted to the data-type `HostCapabilityManifest` (single builder
/// SSOT round-trip) and the adapter validates it at the spoke-schema
/// boundary (`host_id` non-empty/within cap) and upserts it into the
/// `peer_hosts` table of this host's workspace DB, fail-closed — a
/// malformed manifest is never stored. Inbound-only peers (a peer dials us)
/// are not recorded: the invoke boundary carries only `&PeerId`, and the
/// inbound-manifest API change is a spoke-connect change, out of nexus
/// scope (spec lock #1 fallback).
///
/// Callers decide how to treat a recording failure: the session is already
/// established, so a record reject is a bookkeeping error, not a connect
/// failure.
///
/// # Errors
/// [`CliError::Other`] when the wire conversion or the adapter recording is
/// rejected (manifest validation or storage failure).
pub async fn record_dialed_peer(
    adapter: &NexusAdapter<'_>,
    session: &spoke_connect::PeerSession,
) -> Result<()> {
    let manifest =
        match nexus_spoke_adapter::manifest::from_connect_hello(session.remote_manifest()) {
            SpokeResult::Ok(manifest) => manifest,
            SpokeResult::Reject(reject) => {
                return Err(CliError::Other(format!(
                    "peer manifest conversion rejected: {}",
                    reject.message
                )));
            }
        };
    match adapter.record_peer_manifest(&manifest).await {
        SpokeResult::Ok(()) => Ok(()),
        SpokeResult::Reject(reject) => Err(CliError::Other(format!(
            "peer manifest recording rejected: {}",
            reject.message
        ))),
    }
}

/// Open the active-workspace `SQLite` pool (WAL mode, migrations applied) —
/// the `DbPool` the per-process `NexusAdapter` runs against.
///
/// `workspace_db` is a test/embedding seam; `None` resolves the path by the
/// daemon rules (active workspace from `~/.nexus42` config).
///
/// # Errors
/// [`CliError::Config`] when the active workspace cannot be resolved
/// (fail-closed: the host refuses to boot without a workspace), or
/// [`CliError`] from the DB open/migration path.
async fn open_workspace_pool(workspace_db: Option<&Path>) -> Result<sqlx::SqlitePool> {
    let db_path = if let Some(path) = workspace_db {
        path.to_path_buf()
    } else {
        let config = crate::config::CliConfig::load()
            .map_err(|e| CliError::Config(format!("active workspace resolution failed: {e}")))?;
        crate::config::resolve_state_db_path(&config)
            .map_err(|e| CliError::Config(format!("active workspace resolution failed: {e}")))?
    };
    let pool = crate::db::Schema::init(&db_path).await.map_err(|e| {
        CliError::Other(format!(
            "workspace DB open failed at {}: {e}",
            db_path.display()
        ))
    })?;
    Ok(pool)
}

#[cfg(all(test, feature = "connect-host"))]
mod tests {
    use super::*;

    fn observed_peer(host_id: &str, capabilities: &[&str], last_seen: &str) -> ObservedPeerHost {
        let manifest = match nexus_spoke_adapter::manifest::build_local_host_manifest(host_id) {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => panic!("manifest build is Ok: {r:?}"),
        };
        // Capabilities are locked constants in the shared builder; override
        // them to exercise the render path with the peer's actual list.
        let manifest = {
            let mut m = manifest;
            m.capabilities = capabilities.iter().map(|c| (*c).to_string()).collect();
            m
        };
        ObservedPeerHost {
            manifest,
            last_seen: last_seen.to_string(),
        }
    }

    #[test]
    fn render_peer_lines_empty_store_prints_no_peers_observed() {
        let lines = render_peer_lines(&[]);
        assert_eq!(lines, vec!["no peers observed"]);
    }

    #[test]
    fn render_peer_lines_prints_header_and_peer_rows() {
        let peers = vec![
            observed_peer(
                "peer-host-uuid-0001",
                &["spoke-baseline", "l2-computable"],
                "2026-08-08T10:00:00Z",
            ),
            observed_peer("peer-host-uuid-0002", &[], "2026-08-08T09:00:00Z"),
        ];
        let lines = render_peer_lines(&peers);
        assert_eq!(lines.len(), 3, "header + one line per peer");
        assert!(lines[0].starts_with("HOST_ID"), "header names HOST_ID");
        assert!(lines[0].contains("CAPABILITIES"));
        assert!(lines[0].contains("LAST_SEEN"));
        assert!(lines[1].starts_with("peer-host-uuid-0001"));
        assert!(lines[1].contains("spoke-baseline, l2-computable"));
        assert!(lines[1].contains("2026-08-08T10:00:00Z"));
        assert!(lines[2].starts_with("peer-host-uuid-0002"));
        assert!(
            lines[2].contains("  "),
            "empty capabilities render as an empty column (spacing preserved)"
        );
        assert!(lines[2].contains("2026-08-08T09:00:00Z"));
    }
}

#[cfg(all(test, feature = "connect-host"))]
mod interop;
