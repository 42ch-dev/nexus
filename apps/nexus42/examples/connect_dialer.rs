//! Real-process dogfood dialer for the Connect Host N-C0 (DF-72, V1.148 P4).
//!
//! A `spoke-connect` reference peer that dials a **running** `nexus42 connect
//! start` host process from outside the host (loopback, mDNS off) — the P4 T1
//! dogfood path. The P3 interop tests prove the same contract in-process; this
//! example is the external-peer shape used to confirm the integrated
//! experience (real CLI process + persisted identity + device-id `host_id` +
//! shared manifest builder).
//!
//! Usage (build the feature-on binary first):
//!
//! ```text
//! cargo build -p nexus42 --features connect-host --example connect_dialer
//! ./target/debug/examples/connect_dialer --print-peer-only          # step 1: the peer id to allowlist
//! ./target/debug/examples/connect_dialer \
//!     --addr /ip4/127.0.0.1/tcp/<host-port> --host-peer <host-peer-id>   # step 2: dial + probe
//! ```
//!
//! The dialer identity comes from a fixed Ed25519 seed (``--seed <n>``,
//! default 7) so the host's ``--allow-peer`` entry is stable across runs; a
//! different seed is the "non-allowlisted outsider" run.

use libp2p::identity::Keypair;
use nexus_spoke_adapter::manifest::ConnectHelloManifest;
use spoke_connect::{parse_multiaddr, ConnectConfig, SpokeConnectNode};
use std::collections::HashMap;
const CORE_OPS: [&str; 7] = [
    "upsert", "promote", "relate", "check", "assemble", "project", "compute",
];

/// Deterministic Ed25519 keypair (same helper shape as the interop tests).
fn fixed_keypair(seed: u8) -> Keypair {
    Keypair::ed25519_from_bytes([seed; 32]).expect("fixed seed is a valid ed25519 secret")
}

#[tokio::main]
async fn main() {
    let mut seed: u8 = 7;
    let mut addr: Option<String> = None;
    let mut host_peer: Option<String> = None;
    let mut print_peer_only = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => seed = args.next().expect("--seed value").parse().expect("u8 seed"),
            "--addr" => addr = Some(args.next().expect("--addr value")),
            "--host-peer" => host_peer = Some(args.next().expect("--host-peer value")),
            "--print-peer-only" => print_peer_only = true,
            other => panic!("unknown arg: {other}"),
        }
    }

    let key = fixed_keypair(seed);
    let my_peer = key.public().to_peer_id();
    println!("dialer peer_id: {my_peer}");

    if print_peer_only {
        return;
    }

    let addr = addr.expect("--addr <multiaddr> is required");
    let host_peer = host_peer.expect("--host-peer <PeerId> is required");
    let host_peer = host_peer.parse().expect("host peer id parses");

    let config = ConnectConfig {
        identity: key,
        peer_allowlist: vec![host_peer],
        listen_addrs: vec![parse_multiaddr("/ip4/127.0.0.1/tcp/0").expect("loopback")],
        local_manifest: spoke_connect_manifest(),
        handshake_timeout: Some(std::time::Duration::from_secs(10)),
        invoke_handler: None,
        invoke_handler_v2: None,
        op_capability_requirements: std::collections::HashMap::default(),
        trusted_issuers: Vec::new(),
        require_capability_token: false,
        capability_token_provider: None,
    };
    let node = SpokeConnectNode::start(config)
        .await
        .expect("dialer node starts");

    // Dial the running host.
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
        "HANDSHAKE_OK session={} remote={}",
        session.session_id(),
        session.remote_peer_id()
    );

    // Manifest read (§2.5 — delivered inside the signed hello).
    println!(
        "MANIFEST {}",
        serde_json::to_string(session.remote_manifest()).expect("manifest serializes")
    );
    println!(
        "NEGOTIATED_CAPABILITIES {:?}",
        session.negotiated_capabilities()
    );

    // Every core op + a garbage op must be refused `op_unsupported` with the
    // session staying open (N-C0 §3).
    let mut failures = Vec::new();
    for op in CORE_OPS.iter().chain(std::iter::once(&"garbage-op")) {
        match session
            .invoke(*op, serde_json::json!({ "extensions": {} }))
            .await
        {
            Err(spoke_connect::InvokeError::Wire(envelope)) => {
                println!("OP {op}: refused code={}", envelope.code);
                if envelope.code != "op_unsupported" {
                    failures.push(format!(
                        "{op}: expected op_unsupported, got {}",
                        envelope.code
                    ));
                }
            }
            other => failures.push(format!("{op}: expected op_unsupported, got {other:?}")),
        }
    }
    // Session must still be usable after refusals.
    match session
        .invoke("check", serde_json::json!({ "extensions": {} }))
        .await
    {
        Err(spoke_connect::InvokeError::Wire(envelope)) if envelope.code == "op_unsupported" => {
            println!("SESSION_OPEN_AFTER_REFUSALS ok");
        }
        other => failures.push(format!(
            "session-not-open-after-refusal: expected op_unsupported, got {other:?}"
        )),
    }

    node.shutdown().await.expect("shutdown");
    if failures.is_empty() {
        println!("DOGFOOD_RESULT pass");
    } else {
        println!("DOGFOOD_RESULT fail: {failures:?}");
        std::process::exit(2);
    }
}

/// Minimal peer manifest (the dialer is a reference peer, not the host — its
/// manifest shape does not matter to the assertions). Uses the type
/// re-exported by the spoke-adapter boundary (no direct spoke-schemas dep).
fn spoke_connect_manifest() -> ConnectHelloManifest {
    ConnectHelloManifest {
        authority: None,
        capabilities: vec!["spoke-baseline".into()],
        extensions: HashMap::default(),
        host_id: "dogfood-dialer".parse().expect("host id parses"),
        namespaces: Vec::new(),
        roles: vec!["input-source".into()],
        schema_version: std::num::NonZeroU64::new(1).expect("non-zero"),
        // V1.169 (0.11.1): honest empty tools declaration (no tool ABI served).
        tools: Vec::new(),
    }
}
