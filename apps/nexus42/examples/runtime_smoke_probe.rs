//! V1.153 P2 T1 — spawned-process smoke probe for the headless
//! `nexus-runtime` bin.
//!
//! A `spoke-connect` reference peer that dials a **running** `nexus-runtime`
//! process from outside the process (loopback, mDNS off) and prints
//! machine-readable facts the smoke test asserts on:
//!
//! ```text
//! PROBE_PEER_ID=<peer-id>            # --print-peer-only (allowlist seed)
//! DIAL_OK session=<id> remote=<id>   # signed-hello handshake completed
//! SERVED_OPS=upsert,promote,relate   # manifest extensions.nexus.served_ops
//! SESSION_OK                         # session still usable after refusals
//! ```
//!
//! The identity comes from a fixed Ed25519 seed (default 7) so the host's
//! `allowlist.json` entry is deterministic across runs. Usage:
//!
//! ```text
//! cargo build -p nexus42 --features connect-host --example runtime_smoke_probe
//! ./target/debug/examples/runtime_smoke_probe --print-peer-only
//! ./target/debug/examples/runtime_smoke_probe \
//!     --addr /ip4/127.0.0.1/tcp/<host-port> --host-peer <host-peer-id>
//! ```

use libp2p::identity::Keypair;
use spoke_connect::{parse_multiaddr, ConnectConfig, SpokeConnectNode};
use spoke_schemas::connect::connect_hello::HostCapabilityManifest;
use std::collections::HashMap;

/// Fixed Ed25519 seed for this probe's identity — the deterministic peer id
/// the smoke test allowlists before the host boots.
const DIALER_SEED: u8 = 7;

fn fixed_keypair(seed: u8) -> Keypair {
    Keypair::ed25519_from_bytes([seed; 32]).expect("fixed seed is a valid ed25519 secret")
}

/// Minimal peer manifest (the probe is a reference peer — its own manifest
/// shape does not matter to the assertions; mirrors the upstream
/// spoke-connect test fixture).
fn probe_manifest() -> HostCapabilityManifest {
    HostCapabilityManifest {
        authority: None,
        capabilities: vec!["spoke-baseline".into()],
        extensions: HashMap::default(),
        host_id: "smoke-dialer".parse().expect("host id parses"),
        namespaces: Vec::new(),
        roles: vec!["input-source".into()],
        schema_version: std::num::NonZeroU64::new(1).expect("non-zero"),
    }
}

#[tokio::main]
async fn main() {
    let mut addr: Option<String> = None;
    let mut host_peer: Option<String> = None;
    let mut print_peer_only = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--addr" => addr = Some(args.next().expect("--addr value")),
            "--host-peer" => host_peer = Some(args.next().expect("--host-peer value")),
            "--print-peer-only" => print_peer_only = true,
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let key = fixed_keypair(DIALER_SEED);
    let my_peer = key.public().to_peer_id();
    println!("PROBE_PEER_ID={my_peer}");
    if print_peer_only {
        return;
    }

    let Some(addr) = addr else {
        eprintln!("--addr <multiaddr> is required");
        std::process::exit(2);
    };
    let Some(host_peer) = host_peer else {
        eprintln!("--host-peer <PeerId> is required");
        std::process::exit(2);
    };
    let host_peer = match host_peer.parse() {
        Ok(peer) => peer,
        Err(e) => {
            eprintln!("host peer id does not parse: {e}");
            std::process::exit(2);
        }
    };

    let config = ConnectConfig {
        identity: key,
        peer_allowlist: vec![host_peer],
        listen_addrs: vec![parse_multiaddr("/ip4/127.0.0.1/tcp/0").expect("loopback multiaddr")],
        local_manifest: probe_manifest(),
        handshake_timeout: Some(std::time::Duration::from_secs(10)),
        invoke_handler: None,
        op_capability_requirements: HashMap::new(),
        trusted_issuers: Vec::new(),
        require_capability_token: false,
        capability_token_provider: None,
    };
    let node = match SpokeConnectNode::start(config).await {
        Ok(node) => node,
        Err(e) => {
            eprintln!("PROBE_NODE_START_FAILED: {e:?}");
            std::process::exit(1);
        }
    };

    // Dial the running host (bounded by the handshake timeout above).
    let session = match node
        .connect(parse_multiaddr(&addr).expect("host addr parses"))
        .await
    {
        Ok(session) => session,
        Err(e) => {
            println!("DIAL_REJECTED: {e:?}");
            node.shutdown().await.expect("shutdown");
            std::process::exit(1);
        }
    };
    println!(
        "DIAL_OK session={} remote={}",
        session.session_id(),
        session.remote_peer_id()
    );

    // N-C1 surface: the manifest advertises exactly the served write ops.
    let manifest_json =
        serde_json::to_string(session.remote_manifest()).expect("manifest serializes");
    println!("MANIFEST_JSON={manifest_json}");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_json).expect("manifest json parses");
    let served_ops: Vec<&str> = manifest["extensions"]["nexus"]["served_ops"]
        .as_array()
        .map(|ops| ops.iter().filter_map(|op| op.as_str()).collect())
        .unwrap_or_default();
    println!("SERVED_OPS={}", served_ops.join(","));

    // Session usable after the reads: a non-served op is refused with the
    // `op_unsupported` wire envelope (N-C0 refusal contract).
    match session
        .invoke("check", serde_json::json!({ "extensions": {} }))
        .await
    {
        Err(spoke_connect::InvokeError::Wire(envelope)) if envelope.code == "op_unsupported" => {
            println!("SESSION_OK");
        }
        other => {
            println!("SESSION_CHECK_FAILED: {other:?}");
            node.shutdown().await.expect("shutdown");
            std::process::exit(1);
        }
    }

    node.shutdown().await.expect("shutdown");
}
