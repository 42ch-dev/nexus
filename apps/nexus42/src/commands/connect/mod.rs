//! Connect Host commands (DF-72 N-C0) — opt-in feature `connect-host`.
//!
//! `nexus42 connect start` runs a `spoke-connect` node in a **separate OS
//! process** (architect lock Q7): signed-hello handshake, allowlist,
//! honest `HostCapabilityManifest`, and **every inbound op refused** via
//! `invoke_handler = None` (architect lock — no `NexusAdapter` on the
//! connect invoke path; N-C1 is a Non-Goal).
//!
//! Topology rules (product draft `fl-r-connect-host-foundation.md` §2.1/§2.6):
//! - mDNS is **never** enabled (`spoke-connect/mdns` not in the feature set).
//! - `nexus42 daemon start` MUST NOT open a Connect listener — only
//!   `connect start` does (feature-on binary still keeps the daemon unchanged).
//! - Identity + allowlist persist under `~/.nexus42/connect/` (home-layout
//!   path helpers); missing allowlist ⇒ fail-closed (rejects all peers).

pub mod allowlist;
pub mod identity;

use crate::errors::{CliError, Result};
use clap::Subcommand;
use libp2p::Multiaddr;
use nexus_home_layout::device_id::get_or_create_device_id;
use nexus_spoke_adapter::manifest::build_connect_hello_manifest;
use nexus_spoke_adapter::SpokeResult;
use spoke_connect::{parse_multiaddr, ConnectConfig, SpokeConnectNode};
use std::collections::HashMap;
use std::path::Path;

/// Default listen multiaddr when `--listen` is not given (loopback only —
/// binding a routable interface is an explicit operator choice, N-C0 §5.3).
const DEFAULT_LISTEN: &str = "/ip4/127.0.0.1/tcp/0";

/// Connect Host subcommands.
#[derive(Debug, Subcommand)]
pub enum ConnectCommand {
    /// Start the Connect Host node (N-C0: handshake + manifest; all ops refused)
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

/// Wire the N-C0 `ConnectConfig` exactly per the architect lock:
/// `invoke_handler = None` (every inbound invoke answered `op_unsupported`),
/// empty `op_capability_requirements` / `trusted_issuers`,
/// `require_capability_token = false`, provider `None`, handshake timeout
/// default, mDNS not compiled. The manifest comes from the **single builder
/// SSOT** shared with `HostManifestPort` (JSON round-trip to the
/// `connect_hello` wire type).
async fn start(allow_peer: Vec<String>, listen: Vec<String>) -> Result<()> {
    // Raw home: the home-layout identity/allowlist helpers join `.nexus42`
    // themselves. The device-id resolution mirrors `host_manifest_port`
    // exactly so the Connect host_id is the SAME value HostManifestPort
    // advertises (single builder SSOT).
    let home = crate::config::user_home_dir()?;
    let (config, host_id, allowlist_len) = build_config(&home, &allow_peer, &listen)?;

    // 7. Start the node; block on the tokio runtime until SIGINT.
    let node = SpokeConnectNode::start(config)
        .await
        .map_err(|e| CliError::Config(format!("connect node start failed: {e}")))?;

    eprintln!("nexus42 connect start: Connect Host (N-C0) listening");
    eprintln!("  peer_id: {}", node.local_peer_id());
    eprintln!("  host_id: {host_id}");
    for addr in node.listen_addrs() {
        eprintln!("  listen: {addr}");
    }
    eprintln!(
        "  allowlisted peers: {allowlist_len} (fail-closed; add via allowlist.json or --allow-peer)"
    );
    eprintln!("  invokes: all refused (op_unsupported; invoke_handler = None)");
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
/// state + CLI inputs (steps 1–6 of `start`).
///
/// `home` is the **raw** user home (`$HOME`): identity/allowlist helpers
/// join `.nexus42` themselves. The device-id is resolved via
/// `get_or_create_device_id(home)` — the identical resolution
/// `host_manifest_port::resolve_device_id_from_standard_home` uses, so the
/// Connect `host_id` always equals the manifest's `host_id`.
///
/// Returns the config, the resolved `host_id` (for start-up logging), and
/// the effective allowlist length.
///
/// # Errors
/// [`CliError`] on identity/allowlist/manifest/listen failures — see the
/// per-step helpers.
fn build_config(
    home: &Path,
    allow_peer: &[String],
    listen: &[String],
) -> Result<(ConnectConfig, String, usize)> {
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
    //    the T2 dispatch gate); the flat id set feeds the handshake allowlist.
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

    // 6. Architect-locked ConnectConfig (N-C0: no inbound op dispatch).
    let config = ConnectConfig {
        identity,
        peer_allowlist,
        listen_addrs,
        local_manifest,
        handshake_timeout: None,
        invoke_handler: None,
        op_capability_requirements: HashMap::new(),
        trusted_issuers: Vec::new(),
        require_capability_token: false,
        capability_token_provider: None,
    };
    Ok((config, host_id, allowlist_len))
}

#[cfg(all(test, feature = "connect-host"))]
mod interop;
