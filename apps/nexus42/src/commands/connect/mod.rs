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
use nexus_spoke_adapter::{NexusAdapter, SpokeResult};
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
}

/// Run a Connect Host command.
///
/// # Errors
/// Returns a [`CliError`] when identity/allowlist I/O, manifest building, or
/// node startup fails.
pub async fn run(command: ConnectCommand) -> Result<()> {
    match command {
        ConnectCommand::Start { allow_peer, listen } => start(allow_peer, listen).await,
    }
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
    let (config, host_id, allowlist_len) =
        build_host_config(&home, &allow_peer, &listen, None).await?;

    // 7. Start the node; block on the tokio runtime until SIGINT.
    let node = SpokeConnectNode::start(config)
        .await
        .map_err(|e| CliError::Config(format!("connect node start failed: {e}")))?;

    eprintln!("nexus42 connect start: Connect Host (N-C2 read half) listening");
    eprintln!("  peer_id: {}", node.local_peer_id());
    eprintln!("  host_id: {host_id}");
    for addr in node.listen_addrs() {
        eprintln!("  listen: {addr}");
    }
    eprintln!(
        "  allowlisted peers: {allowlist_len} (fail-closed; add via allowlist.json or --allow-peer)"
    );
    eprintln!(
        "  invokes: upsert/promote/relate/check/assemble served (world-scoped); \
         compute/project/unknown refused (op_unsupported)"
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
/// # Errors
/// [`CliError`] on N-C0 assembly, workspace resolution, or DB open failures.
pub async fn build_host_config(
    home: &Path,
    allow_peer: &[String],
    listen: &[String],
    workspace_db: Option<&Path>,
) -> Result<(ConnectConfig, String, usize)> {
    let (mut config, host_id, allowlist_len, peer_scope) = build_config(home, allow_peer, listen)?;

    // N-C1: workspace DB open (WAL pool via the shared Schema initializer —
    // coexistence with a co-running daemon is WAL-governed, not
    // runtime_lock-governed; P1 spec § Process model) + the per-process
    // adapter singleton + the session-peer invoke dispatch handler
    // (spoke-connect 0.9.2 `invoke_handler_v2` — caller identity is the
    // authenticated session peer; the legacy `invoke_handler` is not
    // selected, clean cutover per spec §5.2).
    let pool = open_workspace_pool(workspace_db).await?;
    let adapter = Arc::new(NexusAdapter::new(pool));
    config.invoke_handler_v2 = Some(invoke::build_handler(peer_scope, adapter));

    Ok((config, host_id, allowlist_len))
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
mod interop;
