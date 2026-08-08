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
//! V1.155 P0 (N-C3 multi-host production): `connect dial <multiaddr>` is the
//! production outbound dial surface — it dials a peer host and records the
//! dialed peer's manifest at `connect()` return (fail-closed on dial or
//! record errors); `connect peers list` reads the observed-peer store
//! through the adapter boundary (`NexusAdapter::list_observed_peer_hosts` —
//! `host_id`, capabilities, `last_seen`; empty store → `no peers observed`).
//!
//! V1.155 P1 (capability-token production): `connect token issue` is the
//! operator issuance surface — `~/.nexus42/connect/issuer.key` lifecycle
//! (create-once 0600, distinct from `identity.key`) + the signed wire proof
//! `{v, claims, sig}` on stdout ([`token`]); `~/.nexus42/connect/config.json`
//! is the operator token policy — `trusted_issuers` /
//! `require_capability_token` / `capability_token_provider`, wired into
//! `build_config` ([`config`]).
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
// V1.155 P1 T2: operator config — `~/.nexus42/connect/config.json`
// (trusted_issuers / require_capability_token / capability_token_provider).
pub mod config;
pub mod identity;
// V1.153 P1 N-C1 → V1.154 P0 T2: the session-peer `InvokeHandlerV2`
// closure (architect-locked home; identity = the authenticated session peer).
pub mod invoke;
// V1.155 P1: capability-token production issuance — `connect token issue`.
pub mod token;

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
    /// Dial a peer host and record its manifest (N-C3 multi-host
    /// production).
    ///
    /// The production outbound dial surface: boots a Connect node, dials
    /// `MULTIADDR`, records the dialed peer's manifest into the active
    /// workspace's observed-peer store at `connect()` return
    /// (manifest-backed — `connect start` / `nexus-runtime` never dial), and
    /// prints the dialed peer's id / `host_id` / capabilities. Fail-closed: a
    /// dial failure or a recording reject aborts the command (no silent
    /// no-op recording).
    Dial {
        /// Multiaddr of the peer host to dial (e.g. `/ip4/127.0.0.1/tcp/4321`).
        #[arg(value_name = "MULTIADDR")]
        peer: String,
        /// Peer IDs to allowlist for this run (repeatable; unioned with
        /// `~/.nexus42/connect/allowlist.json`). The dialed peer MUST be
        /// allowlisted: the session allowlist is mutual (a connected peer
        /// can invoke this host on the same session), so dialing a
        /// non-allowlisted peer is refused at the handshake.
        #[arg(long = "allow-peer", value_name = "PEER_ID")]
        allow_peer: Vec<String>,
    },
    /// Issue signed capability tokens (V1.155 P1 production issuance).
    ///
    /// The operator issuance surface: loads or creates the issuer key
    /// (`~/.nexus42/connect/issuer.key`, create-once 0600) and prints the
    /// signed wire proof `{v, claims, sig}` as JSON on stdout.
    Token {
        #[command(subcommand)]
        command: token::TokenCommand,
    },
}

/// `connect peers` subcommands.
#[derive(Debug, Subcommand)]
pub enum PeersCommand {
    /// List observed peer hosts — `host_id`, capabilities, `last_seen`.
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
        ConnectCommand::Dial { peer, allow_peer } => dial(&peer, allow_peer).await,
        ConnectCommand::Token { command } => token::run(command),
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
///
/// Render hardening (QC fix wave): peer-controlled strings (`host_id`,
/// capabilities) are control-char-stripped before they reach the operator's
/// terminal (F-003), and `last_seen` is normalized to fixed millisecond
/// RFC 3339 precision (S-003).
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
            sanitize_cell(peer.manifest.host_id.as_str()),
            sanitize_cell(&peer.manifest.capabilities.join(", ")),
            normalize_last_seen(&peer.last_seen)
        ));
    }
    lines
}

/// Strip control characters from a peer-controlled string before terminal
/// render (QC fix wave F-003): ANSI escapes, newlines, and other control
/// bytes embedded in a remote host's claimed manifest must never reach the
/// operator's terminal. Printable characters (including ordinary
/// whitespace) pass through unchanged.
fn sanitize_cell(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).collect()
}

/// Normalize a stored RFC 3339 UTC timestamp to fixed millisecond precision
/// for render (QC fix wave S-003).
///
/// `chrono::to_rfc3339()` emits variable precision (no fraction / 3 / 6 / 9
/// digits), which makes lexicographic `ORDER BY last_seen DESC` unstable at
/// the same second (`…T10:00:00Z` vs `…T10:00:00.123Z`). Display is
/// normalized to `YYYY-MM-DDTHH:MM:SS.mmmZ` regardless of stored precision
/// (the adapter producer also writes fixed millis, so stored values are
/// already normalized in the primary path). Stored values are validated
/// RFC 3339 UTC at record time; an unparseable value (storage corruption)
/// falls back to the raw string.
fn normalize_last_seen(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value).map_or_else(
        |_| value.to_string(),
        |dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
    )
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

/// Run `connect dial <multiaddr>` — the production outbound dial surface
/// (N-C3, QC fix wave F-001).
///
/// Resolves the real user home + the active workspace DB (the daemon
/// rules, same as `connect peers list`), then dials the peer through
/// [`dial_host`]. `connect start` / `nexus-runtime` never dial — this
/// command is the shipped trigger that makes peer recording reachable in
/// production binaries.
///
/// # Errors
/// [`CliError`] on boot, dial, record, or shutdown failures — see
/// [`dial_host`]. Fail-closed: a recording reject aborts the command.
async fn dial(peer: &str, allow_peer: Vec<String>) -> Result<()> {
    let home = crate::config::user_home_dir()?;
    dial_host(&home, peer, &allow_peer, None).await
}

/// The `connect dial` core (testable with hermetic homes/DBs): dial `peer`
/// (a multiaddr string) via `SpokeConnectNode::connect()` and record the
/// dialed peer's manifest at the return — the outbound observation point
/// (iteration spec `fl-r-w3-n-c3-multi-host.md` §Design lock #1).
///
/// Boot shape = [`build_host_config`] (identity + device-id `host_id` +
/// allowlist file ∪ `allow_peer` overlay + default loopback ephemeral
/// listener + active workspace WAL pool + per-process adapter). The
/// allowlist is mutual: the dialed peer MUST be allowlisted (it can invoke
/// this host on the same session) — empty allowlist ⇒ the dial is refused
/// at the handshake (fail-closed). `workspace_db` overrides the resolved
/// active-workspace DB path (hermetic tests); `None` resolves it by the
/// daemon rules.
///
/// Output (stdout): the dialed multiaddr, the remote `peer_id`, the remote
/// manifest `host_id` (peer-controlled strings sanitized, F-003), and the
/// remote capabilities. Status lines go to stderr, matching `connect
/// start`.
///
/// # Errors
/// [`CliError::Config`] for an invalid multiaddr or node-start failure;
/// [`CliError::Other`] for dial, recording (propagated from
/// [`record_dialed_peer`] — fail-closed: a rejected record aborts the
/// command), or shutdown failures.
async fn dial_host(
    home: &Path,
    peer: &str,
    allow_peer: &[String],
    workspace_db: Option<&Path>,
) -> Result<()> {
    let addr = parse_multiaddr(peer)
        .map_err(|e| CliError::Config(format!("invalid multiaddr {peer:?}: {e}")))?;
    let (config, _host_id, _allowlist_len, adapter) = build_host_config(
        home,
        allow_peer,
        &[DEFAULT_LISTEN.to_string()],
        workspace_db,
    )
    .await?;
    let node = SpokeConnectNode::start(config)
        .await
        .map_err(|e| CliError::Config(format!("connect node start failed: {e}")))?;
    eprintln!(
        "nexus42 connect dial: dialing {addr} (local peer_id: {})",
        node.local_peer_id()
    );
    let session = node
        .connect(addr.clone())
        .await
        .map_err(|e| CliError::Other(format!("connect dial {addr} failed: {e}")))?;
    // Fail-closed (F-001): the dial is not complete until the dialed
    // manifest is recorded — a record reject is a command error, never a
    // silent no-op (the N-C3 honesty contract requires the observation to
    // land in the store).
    record_dialed_peer(&adapter, &session).await?;
    let manifest = session.remote_manifest();
    println!("dialed {addr} ok");
    println!(
        "  peer_id: {}",
        sanitize_cell(&session.remote_peer_id().to_string())
    );
    println!("  host_id: {}", sanitize_cell(manifest.host_id.as_str()));
    println!(
        "  capabilities: {}",
        sanitize_cell(&manifest.capabilities.join(", "))
    );
    // A successful return above implies the record landed: `dial_host`
    // aborts on any recording reject (fail-closed), so `recorded: yes` is
    // not printed as a separate line.
    node.shutdown()
        .await
        .map_err(|e| CliError::Other(format!("connect node shutdown failed: {e}")))?;
    Ok(())
}

/// Assemble the architect-locked N-C0 `ConnectConfig` from the on-disk
/// state + CLI inputs (steps 1–6 of `start`). The N-C1 pieces (workspace DB
/// open + adapter + invoke handler) are added by [`build_host_config`], so
/// this function stays pure (no I/O beyond the identity/allowlist/token-
/// config reads).
///
/// `home` is the **raw** user home (`$HOME`): identity/allowlist helpers
/// join `.nexus42` themselves. The device-id is resolved via
/// `get_or_create_device_id(home)` — the identical resolution
/// `host_manifest_port::resolve_device_id_from_standard_home` uses, so the
/// Connect `host_id` always equals the manifest's `host_id`.
///
/// V1.155 P1: the capability-token surface (`trusted_issuers` /
/// `require_capability_token` / `capability_token_provider`) comes from
/// `~/.nexus42/connect/config.json` — absent file ⇒ the pre-V1.155
/// defaults; malformed file or require-without-issuers ⇒ boot error
/// (fail-closed); an enabled provider loads the issuer key at boot.
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

    // 5b. Token operator config (V1.155 P1): trusted_issuers /
    //     require_capability_token / capability_token_provider from
    //     `~/.nexus42/connect/config.json` (absent ⇒ defaults; malformed or
    //     require-without-issuers ⇒ boot error, fail-closed). An enabled
    //     provider loads the issuer key at boot — a missing key is a boot
    //     error (`connect token issue` is the creation path, lock #4); the
    //     mint-on-demand closure then answers challenges with `sub` = this
    //     node's peer id and the host's manifest capabilities (a token can
    //     never grant more than the host advertises).
    let token_config = config::load(home)?;
    let capability_token_provider = match &token_config.capability_token_provider {
        Some(provider) if provider.enabled => {
            let key_path =
                config::resolve_issuer_key_path(home, provider.issuer_key_path.as_deref());
            let issuer = token::load_issuer_key_at(&key_path)?;
            let sub = identity.public().to_peer_id().to_string();
            Some(token::build_provider(&issuer, sub, local_manifest.capabilities.clone())?)
        }
        _ => None,
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
        trusted_issuers: token_config.trusted_issuers,
        require_capability_token: token_config.require_capability_token,
        capability_token_provider,
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
            observed_peer("peer-host-uuid-0002", &[], "2026-08-08T09:00:00.123456Z"),
        ];
        let lines = render_peer_lines(&peers);
        assert_eq!(lines.len(), 3, "header + one line per peer");
        assert!(lines[0].starts_with("HOST_ID"), "header names HOST_ID");
        assert!(lines[0].contains("CAPABILITIES"));
        assert!(lines[0].contains("LAST_SEEN"));
        assert!(lines[1].starts_with("peer-host-uuid-0001"));
        assert!(lines[1].contains("spoke-baseline, l2-computable"));
        assert!(
            lines[1].contains("2026-08-08T10:00:00.000Z"),
            "last_seen normalized to fixed millisecond precision (S-003)"
        );
        assert!(lines[2].starts_with("peer-host-uuid-0002"));
        assert!(
            lines[2].contains("  "),
            "empty capabilities render as an empty column (spacing preserved)"
        );
        assert!(
            lines[2].contains("2026-08-08T09:00:00.123Z"),
            "variable-precision last_seen normalized to millis"
        );
    }

    #[test]
    fn render_peer_lines_strips_control_chars_from_peer_controlled_cells() {
        // F-003: host_id / capabilities originate from the remote peer's
        // claimed manifest — ANSI escapes and newlines must never reach the
        // operator's terminal.
        let peers = vec![observed_peer(
            "peer-\u{1b}[31mred\u{1b}[0m-uuid\n0003",
            &["spoke-baseline", "l2-\u{1b}[32mcompute\u{1b}[0m"],
            "2026-08-08T10:00:00Z",
        )];
        let lines = render_peer_lines(&peers);
        assert_eq!(lines.len(), 2);
        let row = &lines[1];
        assert!(
            row.chars().all(|c| !c.is_control()),
            "control chars (ANSI ESC, newline) stripped from peer-controlled cells: {row:?}"
        );
        assert!(
            row.starts_with("peer-"),
            "printable host_id prefix preserved: {row:?}"
        );
        assert!(row.contains("uuid"), "printable host_id content preserved");
        assert!(row.contains("compute"));
    }

    #[test]
    fn normalize_last_seen_fixed_precision_and_fallback() {
        assert_eq!(
            normalize_last_seen("2026-08-08T10:00:00Z"),
            "2026-08-08T10:00:00.000Z"
        );
        assert_eq!(
            normalize_last_seen("2026-08-08T10:00:00.123456789Z"),
            "2026-08-08T10:00:00.123Z"
        );
        assert_eq!(
            normalize_last_seen("2026-08-08T10:00:00+00:00"),
            "2026-08-08T10:00:00.000Z"
        );
        // Storage corruption is rejected earlier (InternalError) — the
        // fallback keeps the render total.
        assert_eq!(normalize_last_seen("not-a-timestamp"), "not-a-timestamp");
    }

    // ── V1.155 P1 T2: token operator config wiring ───────────────────────

    fn temp_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_config(home: &Path, body: &str) {
        let dir = home.join(".nexus42").join("connect");
        std::fs::create_dir_all(&dir).expect("create connect dir");
        std::fs::write(dir.join("config.json"), body).expect("write config.json");
    }

    /// `build_config` with a loopback ephemeral listener and no CLI peers —
    /// the token-config surface under test.
    fn build_config_for(
        home: &Path,
    ) -> Result<(ConnectConfig, String, usize, allowlist::PeerScope)> {
        build_config(home, &[], &["/ip4/127.0.0.1/tcp/0".to_string()])
    }

    #[test]
    fn build_config_absent_token_config_keeps_defaults() {
        let home = temp_home();
        let (config, _, _, _) = build_config_for(home.path()).expect("boot with absent config");
        assert!(
            config.trusted_issuers.is_empty(),
            "absent config ⇒ empty trusted_issuers"
        );
        assert!(
            !config.require_capability_token,
            "absent config ⇒ require_capability_token=false"
        );
        assert!(
            config.capability_token_provider.is_none(),
            "absent config ⇒ no provider"
        );
    }

    #[test]
    fn build_config_wires_trusted_issuers_and_require_flag() {
        let home = temp_home();
        write_config(
            home.path(),
            r#"{
                "trusted_issuers": ["12D3KooWIssuerOne"],
                "require_capability_token": true
            }"#,
        );
        let (config, _, _, _) = build_config_for(home.path()).expect("boot with token config");
        assert_eq!(
            config.trusted_issuers,
            vec!["12D3KooWIssuerOne".to_string()]
        );
        assert!(config.require_capability_token);
        assert!(
            config.capability_token_provider.is_none(),
            "provider stays None when not enabled"
        );
    }

    #[test]
    fn build_config_malformed_token_config_fails_boot() {
        let home = temp_home();
        write_config(home.path(), "{ not json");
        let err = build_config_for(home.path()).expect_err("malformed config must fail boot");
        assert!(
            matches!(err, CliError::Config(_)),
            "malformed config is a boot error: {err:?}"
        );
    }

    #[test]
    fn build_config_require_without_issuers_fails_boot() {
        let home = temp_home();
        write_config(home.path(), r#"{ "require_capability_token": true }"#);
        let err = build_config_for(home.path())
            .expect_err("require-without-issuers must fail boot");
        assert!(
            matches!(err, CliError::Config(_)),
            "require-without-issuers is a boot error: {err:?}"
        );
    }

    #[test]
    fn build_config_enabled_provider_without_issuer_key_fails_boot() {
        let home = temp_home();
        write_config(
            home.path(),
            r#"{ "capability_token_provider": { "enabled": true } }"#,
        );
        let err = build_config_for(home.path())
            .expect_err("enabled provider with a missing issuer key must fail boot");
        assert!(
            matches!(err, CliError::Config(_)),
            "missing issuer key is a boot error: {err:?}"
        );
    }

    #[test]
    fn build_config_enabled_provider_yields_working_provider() {
        use spoke_connect::core::{verify_capability_token, CapabilityTokenProof};

        let home = temp_home();
        // The issuer key must exist at boot (the CLI is the creation path).
        let issuer = token::load_or_create_issuer_key(home.path()).expect("issuer key created");
        let issuer_id = token::issuer_peer_id(&issuer).expect("issuer peer id");
        write_config(
            home.path(),
            &format!(
                r#"{{
                    "trusted_issuers": ["{issuer_id}"],
                    "capability_token_provider": {{ "enabled": true }}
                }}"#
            ),
        );

        let (config, _, _, _) = build_config_for(home.path()).expect("boot with enabled provider");
        let provider = config
            .capability_token_provider
            .as_ref()
            .expect("provider is Some when enabled");

        // Mint a proof for a challenger audience and validate it end-to-end:
        // iss = issuer-derived id, sub = this node's peer id, aud = the
        // challenger, capabilities = the host manifest capabilities.
        let challenger = "12D3KooWChallengerPeer";
        let proof_value = provider(challenger).expect("provider mints a proof");
        let proof: CapabilityTokenProof =
            serde_json::from_value(proof_value).expect("proof is the wire shape");
        assert_eq!(proof.claims.iss, issuer_id);
        assert_eq!(proof.claims.aud, challenger);
        assert_eq!(
            proof.claims.sub,
            config.identity.public().to_peer_id().to_string(),
            "sub = this node's peer id"
        );
        assert_eq!(
            proof.claims.capabilities, config.local_manifest.capabilities,
            "token capabilities = the host manifest capabilities"
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock readable")
            .as_secs();
        let granted = verify_capability_token(
            &proof,
            &[issuer_id],
            challenger,
            &proof.claims.sub,
            now + 2,
        )
        .expect("provider proof verifies green");
        assert_eq!(granted, config.local_manifest.capabilities);
    }
}

#[cfg(all(test, feature = "connect-host"))]
mod interop;
