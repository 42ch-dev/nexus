//! P3 T4 — two-node loopback interop golden tests for the Connect Host
//! (DF-72 N-C0). Compiled only with `--features connect-host` (the module is
//! `#[cfg(all(test, feature = "connect-host"))]` from `super`).
//!
//! Coverage per the architect lock's interop table:
//! - (a) an allowlisted reference peer completes the signed-hello handshake
//!   and reads the Nexus `HostCapabilityManifest` (single-builder SSOT);
//! - (b) a non-allowlisted peer is rejected at the handshake (no session, no
//!   manifest leak) while the allowlisted pair keeps working;
//! - (c) every core op (`upsert`/`promote`/`relate`/`check`/`assemble`/
//!   `project`/`compute`) and a garbage op → `op_unsupported`, with the
//!   session staying open and no dispatch path running (`invoke_handler =
//!   None` — refusal is the crate default, statically no handler exists, so
//!   zero side effects are structural);
//! - (d) the capability-token gate is structural: a host with
//!   `require_capability_token = true` + a test issuer completes the
//!   challenge with provider-minted tokens (invokes then still refused
//!   `op_unsupported`), and a tokenless peer is rejected `auth_failed` while
//!   a valid per-invoke `auth` proof passes the gate;
//! - (e) feature-off: `cargo check -p nexus42` has no `spoke-connect` in the
//!   graph (verified separately by commands, see the task brief).
//!
//! Determinism: fixed Ed25519 seeds for host/peer/outsider keypairs (stable
//! peer ids), loopback listen with an ephemeral port (`/ip4/127.0.0.1/tcp/0`
//! resolved via `listen_addrs()`), mDNS off (feature not compiled), and a
//! handshake timeout ≥ `DEFAULT_HANDSHAKE_TIMEOUT` (10 s). All waits are
//! bounded event waits on `connect`/`invoke` futures — no sleeps. Scenarios
//! run one at a time under a process-wide mutex (macOS SO_REUSEPORT loopback
//! port allocation can collide across concurrently running network tests —
//! the same guard the upstream spoke-connect reference tests use).

use super::identity;
use libp2p::identity::Keypair;
use libp2p::PeerId;
use nexus_home_layout::device_id::get_or_create_device_id;
use nexus_spoke_adapter::manifest::{build_connect_hello_manifest, ConnectHelloManifest};
use nexus_spoke_adapter::SpokeResult;
use spoke_connect::core::{
    derive_peer_id_from_ed25519_pubkey, issue_capability_token, CapabilityClaims,
};
use spoke_connect::{parse_multiaddr, ConnectConfig, ConnectError, InvokeError, SpokeConnectNode};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Serializes the network scenarios (see module docs).
static NETWORK_TEST_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

async fn network_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    NETWORK_TEST_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// Handshake / invoke timeout for the test nodes — ≥ `DEFAULT_HANDSHAKE_TIMEOUT`
/// (10 s) per the interop lock; every wait in these tests is bounded by it.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// The Nexus host_id used by the golden tests (deterministic; the builder is
/// host_id-injectable so tests are hermetic — no `~/.nexus42` writes).
const TEST_HOST_ID: &str = "test-device-uuid-0000";

/// A deterministic Ed25519 keypair from a fixed seed.
fn fixed_keypair(seed: u8) -> Keypair {
    Keypair::ed25519_from_bytes([seed; 32]).expect("fixed seed is a valid ed25519 secret")
}

/// The Nexus N-C0 manifest through the single-builder SSOT (same path the
/// CLI's `connect start` uses).
fn nexus_manifest(host_id: &str) -> ConnectHelloManifest {
    match build_connect_hello_manifest(host_id) {
        SpokeResult::Ok(manifest) => manifest,
        SpokeResult::Reject(reject) => panic!("manifest builder rejected: {reject:?}"),
    }
}

/// A Connect Host node config — architect-locked N-C0 wiring with a fixed
/// identity seed: `invoke_handler = None`, no token policy, no capability
/// requirements, mDNS not compiled.
fn host_config(identity: Keypair, allowlist: Vec<PeerId>) -> ConnectConfig {
    ConnectConfig {
        identity,
        peer_allowlist: allowlist,
        listen_addrs: vec![parse_multiaddr("/ip4/127.0.0.1/tcp/0").expect("listen addr")],
        local_manifest: nexus_manifest(TEST_HOST_ID),
        handshake_timeout: Some(HANDSHAKE_TIMEOUT),
        invoke_handler: None,
        op_capability_requirements: HashMap::new(),
        trusted_issuers: Vec::new(),
        require_capability_token: false,
        capability_token_provider: None,
    }
}

/// A reference-peer node config (the upstream spoke-connect client shape):
/// same wiring, a distinct manifest host_id.
fn peer_config(identity: Keypair, allowlist: Vec<PeerId>) -> ConnectConfig {
    ConnectConfig {
        identity,
        peer_allowlist: allowlist,
        listen_addrs: vec![parse_multiaddr("/ip4/127.0.0.1/tcp/0").expect("listen addr")],
        local_manifest: nexus_manifest("peer-device-uuid-0000"),
        handshake_timeout: Some(HANDSHAKE_TIMEOUT),
        invoke_handler: None,
        op_capability_requirements: HashMap::new(),
        trusted_issuers: Vec::new(),
        require_capability_token: false,
        capability_token_provider: None,
    }
}

async fn start(config: ConnectConfig) -> SpokeConnectNode {
    SpokeConnectNode::start(config).await.expect("node starts")
}

/// Unix time now + `offset` seconds, for token expiry fixtures.
fn now_plus(offset: u64) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("post-epoch clock")
        .as_secs()
        + offset
}

/// The `peer_id` string derived from an Ed25519 issuer secret (the issuer's
/// identity, used in `trusted_issuers` and `claims.iss`).
fn issuer_peer_id(issuer_secret: &[u8; 32]) -> String {
    let keypair = Keypair::ed25519_from_bytes(*issuer_secret).expect("issuer secret is valid");
    let public = keypair
        .public()
        .try_into_ed25519()
        .expect("issuer key is ed25519")
        .to_bytes();
    derive_peer_id_from_ed25519_pubkey(&public)
}

/// A challenge-response token provider for `subject`: mints a token from
/// `issuer_secret` with the challenger as the audience (per the upstream
/// spoke-connect test pattern). Issued at `now` (Unix seconds, the 0.9.1
/// issuance-time parameter) with a one-hour lifetime (`exp = now + 3600`),
/// which passes the 0.9.1 fail-fast issuance guards.
fn token_provider(
    issuer_secret: [u8; 32],
    subject: PeerId,
    capabilities: Vec<String>,
    now: u64,
) -> Arc<spoke_connect::CapabilityTokenProvider> {
    Arc::new(move |audience: &str| {
        let proof = issue_capability_token(
            &issuer_secret,
            CapabilityClaims {
                iss: issuer_peer_id(&issuer_secret),
                sub: subject.to_string(),
                aud: audience.to_string(),
                capabilities: capabilities.clone(),
                exp: now + 3600,
                iat: None,
                jti: None,
            },
            now,
        )
        .map_err(|e| e.to_string())?;
        serde_json::to_value(&proof).map_err(|e| e.to_string())
    })
}

/// A valid per-invoke `auth` proof (wire `proof` object), issued at `now`
/// (Unix seconds) with a one-hour lifetime (`exp = now + 3600`) — the
/// 0.9.1 `issue_capability_token` signature and issuance-guard shape.
fn token_proof(
    issuer_secret: &[u8; 32],
    subject: &str,
    audience: &str,
    capabilities: &[&str],
    now: u64,
) -> serde_json::Value {
    let proof = issue_capability_token(
        issuer_secret,
        CapabilityClaims {
            iss: issuer_peer_id(issuer_secret),
            sub: subject.to_string(),
            aud: audience.to_string(),
            capabilities: capabilities.iter().map(|c| (*c).to_string()).collect(),
            exp: now + 3600,
            iat: None,
            jti: None,
        },
        now,
    )
    .expect("issuer key derives iss and claims pass the 0.9.1 issuance guards");
    serde_json::to_value(&proof).expect("proof serializes")
}

/// Assert one invoke is answered with the `op_unsupported` wire envelope
/// (the N-C0 refusal contract).
async fn assert_op_unsupported(session: &spoke_connect::PeerSession, op: &str) {
    match session
        .invoke(op, serde_json::json!({ "extensions": {} }))
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "op_unsupported",
            "op {op} must be refused with op_unsupported"
        ),
        other => panic!("op {op} expected op_unsupported, got {other:?}"),
    }
}

/// (a) An allowlisted reference peer completes the signed-hello handshake on
/// loopback and reads the Nexus N-C0 manifest fields from the session.
#[tokio::test]
async fn allowlisted_peer_handshakes_and_reads_nexus_manifest() {
    let _guard = network_test_guard().await;
    let host_key = fixed_keypair(1);
    let peer_key = fixed_keypair(2);
    let host_peer = host_key.public().to_peer_id();
    let peer_peer = peer_key.public().to_peer_id();

    let host = start(host_config(host_key, vec![peer_peer])).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;

    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("allowlisted peer handshake succeeds");
    assert_eq!(
        session.remote_peer_id(),
        host_peer,
        "session binds the host"
    );
    assert!(!session.session_id().is_empty());

    // The manifest is delivered inside the signed hello (§2.5 — no separate
    // get-manifest op). Assert the full N-C0 field contract.
    let wire = serde_json::to_value(session.remote_manifest()).expect("manifest serializes");
    assert_eq!(wire["host_id"], serde_json::json!(TEST_HOST_ID));
    assert_eq!(wire["schema_version"], serde_json::json!(1));
    assert_eq!(wire["roles"], serde_json::json!(["data-store"]));
    assert_eq!(
        wire["capabilities"],
        serde_json::json!(["spoke-baseline", "l2-computable", "l5-fork"])
    );
    assert_eq!(wire["namespaces"], serde_json::json!(["nexus"]));
    assert_eq!(
        wire["extensions"]["nexus"]["connect_host_slice"],
        serde_json::json!("n-c0")
    );
    assert_eq!(
        wire["extensions"]["nexus"]["daemon_http_coexists"],
        serde_json::json!(true)
    );

    // Negotiated capabilities: intersection of both manifests (both built by
    // the same builder → all three in local-manifest order).
    assert_eq!(
        session.negotiated_capabilities(),
        &["spoke-baseline", "l2-computable", "l5-fork"]
            .map(ToString::to_string)
            .to_vec()
    );

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// (b) A non-allowlisted peer is rejected at the handshake — no session, no
/// manifest — while the allowlisted pair keeps working afterwards.
#[tokio::test]
async fn non_allowlisted_peer_is_rejected_at_handshake() {
    let _guard = network_test_guard().await;
    let host_key = fixed_keypair(11);
    let peer_key = fixed_keypair(12);
    let outsider_key = fixed_keypair(13);
    let host_peer = host_key.public().to_peer_id();
    let peer_peer = peer_key.public().to_peer_id();

    // Host allowlists only `peer`; the outsider is NOT on it.
    let host = start(host_config(host_key, vec![peer_peer])).await;
    let outsider = start(peer_config(outsider_key, vec![host_peer])).await;

    let err = outsider
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect_err("non-allowlisted peer must not establish a session");
    assert!(
        matches!(
            err,
            ConnectError::HandshakeFailed { .. }
                | ConnectError::NotAllowlisted { .. }
                | ConnectError::Timeout(_)
                | ConnectError::Transport(_)
        ),
        "unexpected rejection error: {err:?}"
    );

    // No manifest leak: the rejected dial returned no session (asserted by
    // expect_err above). The allowlisted peer is unaffected.
    let peer = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("allowlisted peer still connects after the rejection");
    assert_eq!(session.remote_peer_id(), host_peer);
    assert_eq!(
        serde_json::to_value(session.remote_manifest()).expect("manifest serializes")["host_id"],
        serde_json::json!(TEST_HOST_ID)
    );

    peer.shutdown().await.expect("peer shuts down");
    outsider.shutdown().await.expect("outsider shuts down");
    host.shutdown().await.expect("host shuts down");
}

/// (c) Every core op + a garbage op is refused with `op_unsupported`; the
/// refusal consumes the sequence but leaves the session open, and the host
/// has no dispatch path at all (`invoke_handler = None` ⇒ zero side effects
/// is structural — there is no handler to run).
#[tokio::test]
async fn every_core_op_and_garbage_op_is_refused_without_side_effects() {
    let _guard = network_test_guard().await;
    let host_key = fixed_keypair(21);
    let peer_key = fixed_keypair(22);
    let host_peer = host_key.public().to_peer_id();
    let peer_peer = peer_key.public().to_peer_id();

    let host = start(host_config(host_key, vec![peer_peer])).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("allowlisted handshake");

    // All core ops + an unknown/garbage op → op_unsupported (product draft
    // §3.2 coverage).
    let ops = [
        "upsert",
        "promote",
        "relate",
        "check",
        "assemble",
        "project",
        "compute",
        "garbage-op",
    ];
    for (index, op) in ops.iter().enumerate() {
        assert_eq!(
            session.next_sequence(),
            index as u64,
            "each invoke consumes one outbound sequence"
        );
        assert_op_unsupported(&session, op).await;
    }

    // A refused invoke does not terminate the session: the next invoke is
    // still answered (op_unsupported again, not session_not_found), proving
    // the refusal path is the crate default and the session stays usable.
    assert_eq!(session.next_sequence(), ops.len() as u64);
    assert_op_unsupported(&session, "check").await;
    assert_eq!(session.next_sequence(), ops.len() as u64 + 1);

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// (d-i) Capability-token gate, positive: a host with a test issuer +
/// `require_capability_token = true` completes the challenge with
/// provider-minted tokens; the dialer's session reports `capability_token_ok`
/// and invokes are STILL refused (`op_unsupported` — the token gate is
/// structural, it does not open a dispatch path).
#[tokio::test]
async fn capability_token_gate_authorizes_and_ops_stay_refused() {
    let _guard = network_test_guard().await;
    let issuer_secret = [31u8; 32];
    let issuer_peer = issuer_peer_id(&issuer_secret);
    let host_key = fixed_keypair(32);
    let peer_key = fixed_keypair(33);
    let host_peer = host_key.public().to_peer_id();
    let peer_peer = peer_key.public().to_peer_id();

    let mut host_cfg = host_config(host_key, vec![peer_peer]);
    host_cfg.trusted_issuers = vec![issuer_peer.clone()];
    host_cfg.require_capability_token = true;
    host_cfg.capability_token_provider = Some(token_provider(
        issuer_secret,
        host_peer,
        vec!["spoke-baseline".into()],
        now_plus(0),
    ));
    let mut peer_cfg = peer_config(peer_key, vec![host_peer]);
    peer_cfg.trusted_issuers = vec![issuer_peer];
    peer_cfg.require_capability_token = true;
    peer_cfg.capability_token_provider = Some(token_provider(
        issuer_secret,
        peer_peer,
        vec!["spoke-baseline".into()],
        now_plus(0),
    ));

    let host = start(host_cfg).await;
    let peer_node = start(peer_cfg).await;

    // The connect completes only after the challenge exchange (token gate is
    // part of session establishment when the policy is active).
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("token-authorized session");
    assert!(
        session.capability_token_ok(),
        "dialer session must complete the token challenge"
    );

    // Even token-authorized invokes are refused: N-C0 has no dispatch path.
    assert_op_unsupported(&session, "check").await;
    assert_op_unsupported(&session, "upsert").await;

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// (d-ii) Capability-token gate, negative: a token-required host rejects
/// invokes from a session that never completed the challenge (`auth_failed`),
/// and a valid per-invoke `auth` proof passes the gate — landing on the
/// still-refused `op_unsupported`.
#[tokio::test]
async fn token_required_host_rejects_tokenless_peer_then_accepts_valid_auth() {
    let _guard = network_test_guard().await;
    let issuer_secret = [41u8; 32];
    let issuer_peer = issuer_peer_id(&issuer_secret);
    let host_key = fixed_keypair(42);
    let peer_key = fixed_keypair(43);
    let host_peer = host_key.public().to_peer_id();
    let peer_peer = peer_key.public().to_peer_id();

    // Host requires a token from the trusted issuer; the peer has NO token
    // policy and NO provider (it cannot answer the host's challenge).
    let mut host_cfg = host_config(host_key, vec![peer_peer]);
    host_cfg.trusted_issuers = vec![issuer_peer];
    host_cfg.require_capability_token = true;

    let host = start(host_cfg).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("hello handshake succeeds; token gate is session-level");
    assert!(
        !session.capability_token_ok(),
        "tokenless peer's session carries no grant"
    );

    // No `auth` on a not-token-authorized session → auth_failed.
    match session
        .invoke("check", serde_json::json!({ "extensions": {} }))
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(envelope.code, "auth_failed"),
        other => panic!("expected auth_failed wire error, got {other:?}"),
    }

    // A valid per-invoke proof from the trusted issuer passes the gate — and
    // the invoke is still refused (op_unsupported; N-C0 dispatch is closed).
    let auth = token_proof(
        &issuer_secret,
        &peer_peer.to_string(),
        &host_peer.to_string(),
        &["spoke-baseline"],
        now_plus(0),
    );
    match session
        .invoke_with_auth("check", serde_json::json!({ "extensions": {} }), Some(auth))
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(envelope.code, "op_unsupported"),
        other => panic!("expected op_unsupported after valid auth, got {other:?}"),
    }

    // An expired proof is rejected auth_failed (the gate validates on every
    // invoke). 0.9.1 `issue_capability_token` fail-fast refuses `exp` within
    // the clock-skew window of its `now` argument, so the fixture backdates
    // the issuance time two hours: the mint-time guards pass, but the token's
    // `exp = now + 3600` lands an hour in the past of the verifier's clock.
    let expired = token_proof(
        &issuer_secret,
        &peer_peer.to_string(),
        &host_peer.to_string(),
        &["spoke-baseline"],
        now_plus(0).saturating_sub(7200),
    );
    match session
        .invoke_with_auth(
            "check",
            serde_json::json!({ "extensions": {} }),
            Some(expired),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(envelope.code, "auth_failed"),
        other => panic!("expected auth_failed for expired token, got {other:?}"),
    }

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// The CLI wiring end-to-end: persisted identity + allowlist file/overlay +
/// device-id host_id + shared manifest builder assemble the architect-locked
/// `ConnectConfig` (exactly `connect start`'s code path) and the node starts.
#[tokio::test]
async fn cli_wiring_starts_a_node_with_persisted_identity_and_allowlist() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();

    // Allowlist file with one entry + a `--allow-peer` overlay entry.
    let file_peer = fixed_keypair(52).public().to_peer_id();
    let cli_peer = fixed_keypair(53).public().to_peer_id();
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [file_peer.to_string()] }).to_string(),
    )
    .expect("write allowlist");

    let (config, host_id, allowlist_len) = super::build_config(
        home,
        &[cli_peer.to_string()],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
    )
    .expect("config builds through the CLI path");

    // The CLI path resolved a real device id + the file ∪ CLI allowlist.
    // `get_or_create_device_id` takes the RAW home (joins `.nexus42` itself).
    let expected_host_id = get_or_create_device_id(home).expect("device id");
    assert_eq!(
        host_id, expected_host_id,
        "host_id must use the shared host_manifest_port resolution"
    );
    assert_eq!(allowlist_len, 2);
    // `PeerScope.peer_ids()` is a sorted set — compare order-independently.
    let mut expected_allowlist = vec![file_peer, cli_peer];
    expected_allowlist.sort();
    assert_eq!(config.peer_allowlist, expected_allowlist);
    assert!(
        config.invoke_handler.is_none(),
        "N-C0 must never install an invoke handler"
    );
    // Manifest fields come from the shared builder (single SSOT).
    let manifest_json = serde_json::to_value(&config.local_manifest).expect("serialize");
    assert_eq!(manifest_json["host_id"], serde_json::json!(host_id));
    assert_eq!(manifest_json["roles"], serde_json::json!(["data-store"]));

    let expected_peer = identity::load_or_create_identity(home)
        .expect("identity reloads")
        .public()
        .to_peer_id();
    let node = SpokeConnectNode::start(config).await.expect("node starts");
    assert_eq!(
        node.local_peer_id(),
        expected_peer,
        "stable persisted identity"
    );
    assert!(!node.listen_addrs().is_empty());

    node.shutdown().await.expect("node shuts down");
}
