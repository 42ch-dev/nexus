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
//! SERVED_OPS=upsert,promote,relate,check,assemble,compute   # manifest extensions.nexus.served_ops
//! UNSERVED_REFUSAL_CODE=<code>       # a truly unserved op's wire refusal
//! UNSERVED_REFUSAL_MESSAGE=<reason>
//! OP_SCOPE_REFUSAL_CODE=<code>       # a SERVED op this peer may not use
//! OP_SCOPE_REFUSAL_MESSAGE=<reason>
//! SESSION_OK                         # session still usable after refusals
//! UPSERT_OK                          # --world: in-scope write after the refusal
//! SESSION_ALIVE_OK                   # the session still works after a refusal
//! COMPUTE_OK session_tag=<16hex> entry=<id> current_hp=<n>  # real retained compute
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
//!
//! The optional `--world <world-id>` half writes two combatants through the
//! served `upsert` op after the refusal and prints `SESSION_ALIVE_OK` — proof
//! the session is still usable. Adding `--compute-session <id>` then runs
//! `compute` against the session the host's workspace has staged. Both are
//! opt-in because the packaged probe also dials hosts that staged no compute
//! fixture, and a host without the wasm32 target can still prove session
//! survival after a refusal.

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

/// Minimal peer manifest (the probe is a reference peer — mirrors the
/// upstream spoke-connect test fixture). `l2-computable` is declared because
/// the opt-in compute half performs the `compute` op: spoke-connect negotiates
/// that capability and refuses the op without it.
fn probe_manifest() -> HostCapabilityManifest {
    HostCapabilityManifest {
        authority: None,
        capabilities: vec!["spoke-baseline".into(), "l2-computable".into()],
        extensions: HashMap::default(),
        host_id: "smoke-dialer".parse().expect("host id parses"),
        namespaces: Vec::new(),
        roles: vec!["input-source".into()],
        schema_version: std::num::NonZeroU64::new(1).expect("non-zero"),
        // V1.169 (0.11.1): honest empty tools declaration (no tool ABI served).
        tools: Vec::new(),
    }
}

#[tokio::main]
// One linear arg-parse plus probe sequence; splitting it would scatter the
// ordering the probe exists to observe.
#[allow(clippy::too_many_lines)]
async fn main() {
    let mut addr: Option<String> = None;
    let mut host_peer: Option<String> = None;
    let mut print_peer_only = false;
    let mut compute_session: Option<String> = None;
    let mut compute_world: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--addr" => addr = Some(args.next().expect("--addr value")),
            "--host-peer" => host_peer = Some(args.next().expect("--host-peer value")),
            "--compute-session" => {
                compute_session = Some(args.next().expect("--compute-session value"));
            }
            "--world" => compute_world = Some(args.next().expect("--world value")),
            "--print-peer-only" => print_peer_only = true,
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    if compute_session.is_some() && compute_world.is_none() {
        eprintln!("--world <world-id> is required with --compute-session");
        std::process::exit(2);
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
        invoke_handler_v2: None,
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

    // N-C2 read-half surface: the manifest advertises exactly the served ops.
    let manifest_json =
        serde_json::to_string(session.remote_manifest()).expect("manifest serializes");
    println!("MANIFEST_JSON={manifest_json}");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_json).expect("manifest json parses");
    let served_ops: Vec<&str> = manifest["extensions"]["nexus"]["served_ops"]
        .as_array()
        .map_or_default(|ops| ops.iter().filter_map(|op| op.as_str()).collect());
    println!("SERVED_OPS={}", served_ops.join(","));

    // Refusal attribution (QC1-02). The wire family collapses every denial
    // into `op_unsupported`; the message is the observable discriminator, so
    // report BOTH a truly unserved op and a served op this peer's `op_scope`
    // excludes. The smoke asserts the second is specifically the op-scope
    // denial, not request validation.
    match session.invoke("project", serde_json::json!({})).await {
        Err(spoke_connect::InvokeError::Wire(envelope)) if envelope.code == "op_unsupported" => {
            report_refusal("UNSERVED_REFUSAL", &envelope);
        }
        other => {
            println!("REFUSAL_UNEXPECTED: project {other:?}");
            node.shutdown().await.expect("shutdown");
            std::process::exit(1);
        }
    }
    match session
        .invoke("check", serde_json::json!({ "extensions": {} }))
        .await
    {
        Err(spoke_connect::InvokeError::Wire(envelope)) if envelope.code == "op_unsupported" => {
            report_refusal("OP_SCOPE_REFUSAL", &envelope);
            println!("SESSION_OK");
        }
        other => {
            println!("SESSION_CHECK_FAILED: {other:?}");
            node.shutdown().await.expect("shutdown");
            std::process::exit(1);
        }
    }

    // Opt-in retained half: `--world` seeds two stored entries through the
    // served `upsert` op (the session-alive proof after the refusal above);
    // `--compute-session` additionally runs the real compute lane. The module
    // bytes live host-locally (never peer-supplied).
    if let Some(world) = compute_world {
        if let Err(error) = run_retained_ops(&session, compute_session.as_deref(), &world).await {
            println!("RETAINED_OP_FAILED: {error}");
            node.shutdown().await.expect("shutdown");
            std::process::exit(1);
        }
    }

    node.shutdown().await.expect("shutdown");
}

/// Print one refusal's code and reason on separate machine-readable lines.
///
/// The product keeps ONE `op_unsupported` code for served-op, op-scope and
/// payload denials (no cross-peer scope leak); the reason text is what
/// distinguishes "not provided" from "outside the granted scope".
fn report_refusal(
    tag: &str,
    envelope: &spoke_schemas::connect::connect_invoke_response::ErrorEnvelope,
) {
    println!("{tag}_CODE={}", envelope.code);
    println!("{tag}_MESSAGE={}", envelope.message);
}

/// One wire-shape combatant: the flat `body.attributes` the installed
/// deterministic `basic-combat` module's manifest requires, plus the
/// `body.state.character` block `compute` merges into.
fn combat_entry(
    entry_id: &str,
    world: &str,
    max_hp: i64,
    base_atk: i64,
    base_def: i64,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "entry_id": entry_id,
        "entry_type": "character",
        "canonical_name": entry_id,
        "status": "confirmed",
        "revision": null,
        "body": {
            "summary": format!("{entry_id} summary"),
            "attributes": [
                { "trait_type": "max_hp", "value": max_hp },
                { "trait_type": "base_atk", "value": base_atk },
                { "trait_type": "base_def", "value": base_def },
            ],
            "state": { "character": { "current_hp": max_hp, "max_hp": max_hp } },
        },
        "extensions": { "nexus": { "world_id": world } },
    })
}

/// Seed `kb_atk`/`kb_def` through the served `upsert` op (the in-scope write
/// that proves the session still works after the refusal) and, when a compute
/// session is supplied, run the staged `compute`. Deterministic math:
/// 30 HP − (ATK 20 − DEF 5) = 15.
async fn run_retained_ops(
    session: &spoke_connect::PeerSession,
    session_id: Option<&str>,
    world: &str,
) -> Result<(), String> {
    session
        .invoke(
            "upsert",
            serde_json::json!({
                "knowledge_entries": [
                    combat_entry("kb_atk", world, 100, 20, 10),
                    combat_entry("kb_def", world, 30, 5, 5),
                ],
            }),
        )
        .await
        .map_err(|e| format!("upsert rejected: {e:?}"))?;
    println!("UPSERT_OK");
    println!("SESSION_ALIVE_OK");

    let Some(session_id) = session_id else {
        return Ok(());
    };

    let computed = session
        .invoke(
            "compute",
            serde_json::json!({
                "session_id": session_id,
                "entry_id": "kb_atk",
                "computable": { "attacker_id": "kb_atk", "defender_id": "kb_def" },
                "settle": false,
            }),
        )
        .await
        .map_err(|e| format!("compute rejected: {e:?}"))?;
    let current_hp = computed.payload["computable"]["character"]["current_hp"]
        .as_i64()
        .ok_or_else(|| {
            format!(
                "compute response has no numeric current_hp: {}",
                computed.payload
            )
        })?;
    // Print a tag, not the raw id: a session id is sensitive for the
    // cleartext-logging rule, and the proof only needs "this result belongs to
    // THAT session". The test derives the same tag with `DefaultHasher`, so
    // both sides must build with the same pinned toolchain.
    let session_tag = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        session_id.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    };
    println!("COMPUTE_OK session_tag={session_tag} entry=kb_atk current_hp={current_hp}");
    Ok(())
}
