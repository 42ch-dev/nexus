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

/// Seed a `narrative_worlds` row (plus its `creators` FK row) so the
/// workspace DB satisfies the WAL-adjacent FK constraints the production
/// adapter's `put_*` ports hit (PRAGMA foreign_keys = ON).
async fn seed_world(pool: &sqlx::SqlitePool, creator_id: &str, world_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES (?, 'test creator', 'active', 'now', '{}')",
    )
    .bind(creator_id)
    .execute(pool)
    .await
    .expect("creator seed");
    // SAFETY: test-only static INSERT with bind params against known schema.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
         VALUES (?, 'wrk_test', ?, ?, ?, 'active', 'private', 'manual', '{}')",
    )
    .bind(world_id)
    .bind(creator_id)
    .bind(world_id)
    .bind(world_id)
    .execute(pool)
    .await
    .expect("world seed");
}

/// Seed a `kb_key_blocks` row directly (test-only). Used by the fix-loop
/// regression tests to plant rows in a world the peer is NOT scoped to —
/// the dispatch gate must never let the peer reach rows seeded this way.
async fn seed_key_block(
    pool: &sqlx::SqlitePool,
    entry_id: &str,
    world_id: &str,
    canonical_name: &str,
    status: &str,
    revision: i64,
) {
    // SAFETY: test-only static INSERT with bind params against known schema
    // (20260731000001_pack_import_provenance.sql; `created_at` has a default).
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, revision) \
         VALUES (?, ?, 'character', ?, ?, ?)",
    )
    .bind(entry_id)
    .bind(world_id)
    .bind(canonical_name)
    .bind(status)
    .bind(revision)
    .execute(pool)
    .await
    .expect("seed key block");
}

/// A wire-shape `KnowledgeEntry` JSON fixture (the pack-test sample shape +
/// the `extensions.nexus.world_id` carrier the dispatch gate reads).
///
/// `revision: None` serializes to JSON `null` (== absent for the wire
/// `Option<u64>`), which is the legal create-path shape; the OCC test cases
/// pass `Some(...)` to drive the update-path revision checks.
fn entry_fixture(
    entry_id: &str,
    canonical_name: &str,
    world_id: &str,
    status: &str,
    revision: Option<u64>,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "entry_id": entry_id,
        "entry_type": "character",
        "canonical_name": canonical_name,
        "status": status,
        "revision": revision,
        "body": { "summary": format!("{canonical_name} summary") },
        "extensions": { "nexus": { "world_id": world_id } },
    })
}

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
        invoke_handler_v2: None,
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
        invoke_handler_v2: None,
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
    // get-manifest op). Assert the full N-C0 baseline + N-C1 extension of
    // the field contract (the single shared builder now advertises the
    // delivered N-C1 slice + served write ops).
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
        serde_json::json!("n-c1")
    );
    assert_eq!(
        wire["extensions"]["nexus"]["served_ops"],
        serde_json::json!(["upsert", "promote", "relate"])
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

/// N-C1 honesty machine-check (P1 spec § Manifest honesty, both directions):
/// the manifest a peer reads off the signed hello must advertise exactly the
/// write ops the invoke dispatcher serves. (a) Every op the manifest
/// advertises (`extensions.nexus.served_ops`) is actually served by the
/// dispatch; (b) every op the dispatch serves (`super::invoke::SERVED_OPS`)
/// is advertised by the manifest. The manifest comes from the single shared
/// builder (`build_connect_hello_manifest` — the same bytes
/// `connect start` puts in `ConnectConfig.local_manifest`), so this is the
/// wire-truth cross-crate check; the crate-level honesty test
/// (`n_c1_manifest_is_honest` in `nexus-spoke-adapter`) covers the
/// manifest-side contract (exact op list + production-orchestrator backing).
///
/// No network needed — the builder is host_id-injectable and hermetic.
#[test]
fn n_c1_manifest_served_ops_match_dispatch_both_directions() {
    let manifest = nexus_manifest(TEST_HOST_ID);
    let wire = serde_json::to_value(&manifest).expect("manifest serializes");
    let advertised = wire["extensions"]["nexus"]["served_ops"]
        .as_array()
        .expect("extensions.nexus.served_ops array present")
        .iter()
        .map(|op| op.as_str().expect("served op is a string").to_string())
        .collect::<Vec<_>>();

    // (a) Every capability advertised in the manifest is actually served by
    //     the dispatch.
    for op in &advertised {
        assert!(
            super::invoke::SERVED_OPS.contains(&op.as_str()),
            "manifest advertises op {op:?} but the dispatch does not serve it"
        );
    }

    // (b) Every op served by the dispatch is advertised in the manifest.
    for op in super::invoke::SERVED_OPS {
        assert!(
            advertised
                .iter()
                .any(|advertised| advertised.as_str() == op),
            "dispatch serves op {op:?} but the manifest does not advertise it"
        );
    }

    // The two sets are identical — no extras on either side.
    assert_eq!(
        advertised,
        super::invoke::SERVED_OPS
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "advertised served_ops must equal the dispatch served-op table exactly"
    );
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
/// device-id host_id + shared manifest builder + the N-C1 workspace-DB open +
/// per-process adapter + invoke handler assemble the full `connect start`
/// boot path (`build_host_config`) and the node starts.
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

    // Hermetic workspace DB (the `build_host_config` override seam keeps the
    // boot out of the real `~/.nexus42` config).
    let db_path = temp.path().join("workspace").join("state.db");
    let (config, host_id, allowlist_len) = super::build_host_config(
        home,
        &[cli_peer.to_string()],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
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
        config.invoke_handler_v2.is_some(),
        "N-C1 host boot must install the session-peer invoke dispatch handler (v2)"
    );
    assert!(
        config.invoke_handler.is_none(),
        "clean cutover: the legacy payload-identity handler must not be selected (spec §5.2)"
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

/// N-C1 (V1.153 P1): the Connect invoke dispatch layer end-to-end over the
/// wire. An allowlisted, world-scoped peer runs the three write ops against
/// a hermetic workspace DB inside the host process (per-process adapter);
/// the OCC reject mapping is exercised with a stale-revision second upsert;
/// wrong-world, absent-scope, and non-served ops are denied with
/// `op_unsupported` and zero side effects; the session stays usable.
///
/// `flavor = "multi_thread"` mirrors the CLI runtime (`connect start` runs
/// on a multi-thread tokio runtime — the R2 bounded bridge parks the
/// caller inside the runtime while the orchestrator runs on a
/// `spawn_blocking` lane, which requires a multi-thread runtime).
#[tokio::test(flavor = "multi_thread")]
async fn n_c1_peer_upserts_promotes_relates_with_world_scoping() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(62);
    let outsider_key = fixed_keypair(63);
    let peer_peer = peer_key.public().to_peer_id();
    let outsider_peer = outsider_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";
    const WORLD_B: &str = "wld_test_b";

    // Hermetic workspace DB (FK rows for both worlds so the production
    // adapter's put paths can persist).
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;
    seed_world(&pool, "ctr_test", WORLD_B).await;

    // Allowlist file: `peer` is world-scoped to WORLD_A with all three write
    // ops; `outsider` is allowlisted via the CLI overlay only (no scope ⇒
    // handshake-ok but every write denied — fail-closed).
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": ["upsert", "promote", "relate"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    // Boot the host through the full N-C1 CLI path (hermetic DB override).
    // The host identity is the persisted `identity.key` from the temp home —
    // its real peer id is what the dialing peers must allowlist.
    let (config, _, _) = super::build_host_config(
        home,
        &[outsider_peer.to_string()],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C1 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let outsider_node = start(peer_config(outsider_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let outsider_session = outsider_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("outsider handshake");

    let peer_claim = serde_json::json!(peer_peer.to_string());
    let outsider_claim = serde_json::json!(outsider_peer.to_string());

    // 1. upsert round-trip: the scoped peer persists a confirmed entry in
    // WORLD_A; the response carries the post-create revision (Some(1)).
    let upsert = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture("kb_a1", "Mira", WORLD_A, "confirmed", None)],
            }),
        )
        .await
        .expect("scoped upsert is served");
    assert_eq!(upsert.payload["knowledge_entries"][0]["entry_id"], "kb_a1");
    assert_eq!(
        upsert.payload["knowledge_entries"][0]["revision"], 1,
        "fresh create must persist with revision 1"
    );
    let stored_rev: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM kb_key_blocks WHERE world_id = ? AND key_block_id = ?",
    )
    .bind(WORLD_A)
    .bind("kb_a1")
    .fetch_optional(&pool)
    .await
    .expect("read persisted row");
    assert_eq!(stored_rev, Some(1), "entry persisted in the workspace DB");

    // 2. OCC CAS-accept: an update carrying the current revision (1) passes
    // the revision match and bumps the persisted revision to 2.
    let updated = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture("kb_a1", "Mira", WORLD_A, "confirmed", Some(1))],
            }),
        )
        .await
        .expect("CAS-accept upsert is served");
    assert_eq!(updated.payload["knowledge_entries"][0]["revision"], 2);
    let stored_rev: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM kb_key_blocks WHERE world_id = ? AND key_block_id = ?",
    )
    .bind(WORLD_A)
    .bind("kb_a1")
    .fetch_optional(&pool)
    .await
    .expect("read persisted row");
    assert_eq!(stored_rev, Some(2), "CAS update persisted revision 2");

    // 3. OCC stale: a candidate revision behind the stored revision maps to
    // the locked `stored_revision_stale` envelope code (retry-safe) and the
    // stored row is untouched.
    match session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture("kb_a1", "Mira", WORLD_A, "confirmed", Some(0))],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(envelope.code, "stored_revision_stale");
        }
        other => panic!("stale upsert must reject with stored_revision_stale, got {other:?}"),
    }

    // 4. OCC conflict: a candidate revision ahead of the stored revision
    // maps to the locked `revision_conflict` envelope code.
    match session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture("kb_a1", "Mira", WORLD_A, "confirmed", Some(3))],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(envelope.code, "revision_conflict");
        }
        other => panic!("conflicting upsert must reject with revision_conflict, got {other:?}"),
    }
    let stored_rev: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM kb_key_blocks WHERE world_id = ? AND key_block_id = ?",
    )
    .bind(WORLD_A)
    .bind("kb_a1")
    .fetch_optional(&pool)
    .await
    .expect("read persisted row");
    assert_eq!(
        stored_rev,
        Some(2),
        "OCC rejects must not mutate the stored row"
    );

    // 3. promote round-trip: a fresh provisional candidate is confirmed.
    let promoted = session
        .invoke(
            "promote",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "candidate": entry_fixture("kb_a2", "Ashford", WORLD_A, "provisional", None),
            }),
        )
        .await
        .expect("scoped promote is served");
    assert_eq!(promoted.payload["knowledge_entry"]["entry_id"], "kb_a2");
    assert_eq!(promoted.payload["knowledge_entry"]["status"], "confirmed");

    // 4. relate round-trip: a fresh relation between the two entries.
    let related = session
        .invoke(
            "relate",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "relation": {
                    "schema_version": 1,
                    "relation_id": "rel_a1",
                    "relation_type": "related_to",
                    "from_id": "kb_a1",
                    "to_id": "kb_a2",
                    "extensions": { "nexus": { "world_id": WORLD_A } },
                },
            }),
        )
        .await
        .expect("scoped relate is served");
    assert_eq!(related.payload["relation"]["relation_id"], "rel_a1");
    let rel_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_relationships WHERE world_id = ? AND relationship_id = ?",
    )
    .bind(WORLD_A)
    .bind("rel_a1")
    .fetch_one(&pool)
    .await
    .expect("count relation rows");
    assert_eq!(rel_count, 1, "relation persisted in the workspace DB");

    // 5. Wrong-world denied: the same peer targeting WORLD_B gets
    // `op_unsupported` and the entry is NOT persisted (zero side effects).
    match session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture("kb_b1", "Banished", WORLD_B, "confirmed", None)],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(envelope.code, "op_unsupported", "wrong-world upsert denied");
        }
        other => panic!("wrong-world upsert must be denied, got {other:?}"),
    }
    let leaked: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM kb_key_blocks WHERE world_id = ? AND key_block_id = ?")
            .bind(WORLD_B)
            .bind("kb_b1")
            .fetch_optional(&pool)
            .await
            .expect("check for leaked row");
    assert!(leaked.is_none(), "wrong-world entry must not be persisted");

    // 6. Absent-scope peer denied: the CLI-overlay outsider (no world/op
    // scope in the file) is handshake-allowlisted but every write is
    // refused — fail-closed.
    match outsider_session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": outsider_claim } },
                "knowledge_entries": [entry_fixture("kb_x1", "Nobody", WORLD_A, "confirmed", None)],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "absent-scope peer write denied"
            );
        }
        other => panic!("absent-scope upsert must be denied, got {other:?}"),
    }

    // 7. Non-served op (N-C0 refusal contract extends into the handler).
    assert_op_unsupported(&session, "check").await;

    // 8. Denials consume sequences but leave the session open: a served op
    // still round-trips afterwards.
    let after = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture("kb_a3", "Rook", WORLD_A, "confirmed", None)],
            }),
        )
        .await
        .expect("session stays usable after denials");
    assert_eq!(after.payload["knowledge_entries"][0]["entry_id"], "kb_a3");

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
    outsider_node.shutdown().await.expect("outsider shuts down");
}

/// N-C1 → E2 (V1.154 P0 T2): caller identity resolves from the
/// noise-authenticated **session peer** (spoke-connect 0.9.2
/// `InvokeHandlerV2`), never from the payload's
/// `extensions.nexus.peer_id` claim.
///
/// Spec §5.1 lock (hard deny, fail-closed): a payload that still carries
/// `extensions.nexus.peer_id` must have it EQUAL the session peer; a
/// differing, non-string, unparseable, or oversized claim is denied through
/// the existing allowlist-denial path (`op_unsupported` family) inside
/// `dispatch`, before the orchestrator/storage bridge — zero side effects.
/// The spoofed identity B is itself a scoped allowlisted peer, so the legacy
/// payload-trusting dispatch would ACCEPT the invoke (the R1 vulnerability
/// this migration closes); only session-peer identity can deny it.
///
/// Covers all §5.1 branches: mismatch ⇒ denied + no row persisted;
/// non-string claim ⇒ denied + no row; unparseable claim ⇒ denied + no row;
/// oversized claim (>128 chars, invoke.rs parse cap) ⇒ denied + no row;
/// absent claim ⇒ served under the session peer's scope (proves the identity
/// source really switched); equal claim ⇒ served (V1.153 clients sending the
/// correct payload identity keep working).
#[tokio::test(flavor = "multi_thread")]
async fn n_c1_session_peer_identity_denies_spoofed_payload_claim_and_serves_claimless() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(72);
    let spoofed_key = fixed_keypair(73);
    let peer_peer = peer_key.public().to_peer_id();
    let spoofed_peer = spoofed_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";

    // Hermetic workspace DB (FK rows so the production adapter's put paths
    // can persist).
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // BOTH peers are scoped write peers for WORLD_A: the spoofed identity B
    // must carry real write scope, or the legacy payload-trusting dispatch
    // would deny the invoke for scope reasons and the test could not tell
    // "denied because spoofed" apart from "denied because B has no scope".
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [
            {
                "peer_id": peer_peer.to_string(),
                "world_scope": [WORLD_A],
                "op_scope": ["upsert", "promote", "relate"],
            },
            {
                "peer_id": spoofed_peer.to_string(),
                "world_scope": [WORLD_A],
                "op_scope": ["upsert", "promote", "relate"],
            },
        ] })
        .to_string(),
    )
    .expect("write allowlist");

    // Boot the host through the full N-C1 CLI path (hermetic DB override).
    let (config, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C1 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("session peer handshake");

    // 1. Spoof-mismatch (spec §5.1 hard deny): the session peer (A) sends a
    //    payload claiming the OTHER scoped peer (B). Must be denied with
    //    `op_unsupported` and ZERO side effects — no row persisted.
    let spoofed_claim = serde_json::json!(spoofed_peer.to_string());
    match session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": spoofed_claim } },
                "knowledge_entries": [
                    entry_fixture("kb_s1", "Spoofed", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "spoofed payload peer_id must be denied through the allowlist-denial path"
            );
        }
        other => panic!("spoofed payload peer_id must be denied, got {other:?}"),
    }
    let leaked: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM kb_key_blocks WHERE world_id = ? AND key_block_id = ?")
            .bind(WORLD_A)
            .bind("kb_s1")
            .fetch_optional(&pool)
            .await
            .expect("check for leaked row");
    assert!(
        leaked.is_none(),
        "spoofed invoke must have zero side effects (no row persisted)"
    );

    // 1b. Non-string claim (spec §5.1 fail-closed): `peer_id` is a number,
    //     not a PeerId string — same hard deny, zero side effects.
    let non_string = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": 123 } },
                "knowledge_entries": [
                    entry_fixture("kb_s1b", "NonString", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await;
    assert!(
        matches!(&non_string, Err(InvokeError::Wire(envelope)) if envelope.code == "op_unsupported"),
        "non-string payload peer_id must be denied, got {non_string:?}"
    );

    // 1c. Unparseable claim: a string that is not a PeerId — fail-closed.
    let unparseable = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": "not-a-peer-id" } },
                "knowledge_entries": [
                    entry_fixture("kb_s1c", "Unparseable", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await;
    assert!(
        matches!(&unparseable, Err(InvokeError::Wire(envelope)) if envelope.code == "op_unsupported"),
        "unparseable payload peer_id must be denied, got {unparseable:?}"
    );

    // 1d. Oversized claim: >128 chars — denied by the invoke.rs parse cap
    //     (mirrors the spoke session-core 128-char decode input cap) before
    //     any decode work; zero side effects.
    let oversized = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": "z".repeat(129) } },
                "knowledge_entries": [
                    entry_fixture("kb_s1d", "Oversized", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await;
    assert!(
        matches!(&oversized, Err(InvokeError::Wire(envelope)) if envelope.code == "op_unsupported"),
        "oversized payload peer_id must be denied, got {oversized:?}"
    );

    // 1e. Zero side effects across every deny branch above: none of the
    //     denied invokes may persist a row.
    for denied_id in ["kb_s1b", "kb_s1c", "kb_s1d"] {
        let leaked: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM kb_key_blocks WHERE world_id = ? AND key_block_id = ?",
        )
        .bind(WORLD_A)
        .bind(denied_id)
        .fetch_optional(&pool)
        .await
        .expect("check for leaked row");
        assert!(
            leaked.is_none(),
            "denied invoke must have zero side effects (no row {denied_id} persisted)"
        );
    }

    // 2. Absent claim ⇒ the session peer is authoritative: no payload
    //    peer_id, still served under A's scope (the branch the 0.9.1-shaped
    //    payload-identity dispatch could not serve at all).
    let claimless = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": {} },
                "knowledge_entries": [
                    entry_fixture("kb_s2", "Claimless", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await
        .expect("claimless upsert is served under the session peer identity");
    assert_eq!(claimless.payload["knowledge_entries"][0]["entry_id"], "kb_s2");

    // 3. Equal claim ⇒ still served (V1.153 clients sending the correct
    //    payload identity keep working — spec §5.1).
    let peer_claim = serde_json::json!(peer_peer.to_string());
    let matching = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    entry_fixture("kb_s3", "Matching", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await
        .expect("matching payload peer_id is served");
    assert_eq!(matching.payload["knowledge_entries"][0]["entry_id"], "kb_s3");

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C1 fix loop (L2 Critical regression): the orchestrators' stored
/// lookups and CAS updates match on id + revision only (world-agnostic —
/// `WHERE key_block_id = ?` / `AND COALESCE(revision,0) = ?`), so a payload
/// claiming WORLD_A can rewrite a row stored in WORLD_B by replaying the
/// revision the OCC rejects disclose. The dispatch layer must verify the
/// stored row's world against the payload-claimed world BEFORE the
/// orchestrator CAS runs and deny with zero side effects. Covers the
/// update, promote, and relate paths (relate shares the same
/// world-agnostic lookup/CAS shape).
#[tokio::test(flavor = "multi_thread")]
async fn n_c1_cross_world_update_promote_and_relate_are_denied_with_zero_mutation() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(64);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";
    const WORLD_B: &str = "wld_test_b";

    // Hermetic workspace DB with both worlds seeded.
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;
    seed_world(&pool, "ctr_test", WORLD_B).await;

    // Plant world-B rows the world-A peer must NOT be able to touch: a
    // confirmed entry (update target), a provisional entry (promote target),
    // and a relation between them. Revisions are 1 — the value a hostile
    // peer learns from OCC reject details (`actualRevision`/`storeRevision`).
    seed_key_block(&pool, "kb_b_update", WORLD_B, "Banished", "confirmed", 1).await;
    seed_key_block(&pool, "kb_b_promote", WORLD_B, "Seer", "provisional", 1).await;
    // SAFETY: test-only static INSERT with bind params against the known
    // 202606290001_kb_relationships.sql schema.
    sqlx::query(
        "INSERT INTO kb_relationships \
         (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, created_at, updated_at, revision) \
         VALUES (?, ?, ?, ?, 'related_to', 'now', 'now', 1)",
    )
    .bind("rel_b1")
    .bind(WORLD_B)
    .bind("kb_b_update")
    .bind("kb_b_promote")
    .execute(&pool)
    .await
    .expect("seed world-B relation");

    // Allowlist file: the peer is world-scoped to WORLD_A only.
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": ["upsert", "promote", "relate"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C1 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // 1. Cross-world UPDATE denied: the correct stored revision (1) is
    //    replayed while the payload claims WORLD_A.
    match session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture(
                    "kb_b_update",
                    "Banished",
                    WORLD_A,
                    "confirmed",
                    Some(1),
                )],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "cross-world update must be denied"
            );
        }
        other => panic!("cross-world update must be denied, got {other:?}"),
    }

    // 2. Cross-world PROMOTE denied (same replay, candidate claims WORLD_A).
    match session
        .invoke(
            "promote",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "candidate": entry_fixture(
                    "kb_b_promote",
                    "Seer",
                    WORLD_A,
                    "provisional",
                    Some(1),
                ),
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "cross-world promote must be denied"
            );
        }
        other => panic!("cross-world promote must be denied, got {other:?}"),
    }

    // 3. Cross-world RELATE denied (relation row stored in WORLD_B, payload
    //    claims WORLD_A with the stored revision replayed).
    match session
        .invoke(
            "relate",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "relation": {
                    "schema_version": 1,
                    "relation_id": "rel_b1",
                    "relation_type": "related_to",
                    "from_id": "kb_b_update",
                    "to_id": "kb_b_promote",
                    "revision": 1,
                    "extensions": { "nexus": { "world_id": WORLD_A } },
                },
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "cross-world relate must be denied"
            );
        }
        other => panic!("cross-world relate must be denied, got {other:?}"),
    }

    // Zero side effects: every world-B row is untouched (same world, same
    // revision, same status).
    let row: (String, i64, String) = sqlx::query_as(
        "SELECT world_id, revision, status FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind("kb_b_update")
    .fetch_one(&pool)
    .await
    .expect("read update row");
    assert_eq!(
        row,
        (WORLD_B.to_string(), 1, "confirmed".to_string()),
        "world-B update target must be untouched"
    );
    let row: (String, i64, String) = sqlx::query_as(
        "SELECT world_id, revision, status FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind("kb_b_promote")
    .fetch_one(&pool)
    .await
    .expect("read promote row");
    assert_eq!(
        row,
        (WORLD_B.to_string(), 1, "provisional".to_string()),
        "world-B promote target must be untouched"
    );
    let row: (String, i64) =
        sqlx::query_as("SELECT world_id, revision FROM kb_relationships WHERE relationship_id = ?")
            .bind("rel_b1")
            .fetch_one(&pool)
            .await
            .expect("read relation row");
    assert_eq!(
        row,
        (WORLD_B.to_string(), 1),
        "world-B relation must be untouched"
    );

    // 4. Denials consume sequences but leave the session open: a legitimate
    //    world-A write still round-trips afterwards.
    let after = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture(
                    "kb_a_ok",
                    "Mira",
                    WORLD_A,
                    "confirmed",
                    None,
                )],
            }),
        )
        .await
        .expect("session stays usable after cross-world denials");
    assert_eq!(after.payload["knowledge_entries"][0]["entry_id"], "kb_a_ok");

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C1 fix loop (L2 Important regression): a multi-entry upsert mixing one
/// scoped entry with one world-less entry used to pass the world gate (the
/// world-less entry was filter-mapped OUT of the gate's world set), persist
/// the scoped entry, then fail the world-less entry in the adapter — a
/// partial-batch write surfacing as `internal_error` for a client input
/// error. Every entry must carry `extensions.nexus.world_id`; one missing ⇒
/// the WHOLE payload is denied and zero entries persist.
#[tokio::test(flavor = "multi_thread")]
async fn n_c1_mixed_payload_missing_world_id_denies_whole_payload() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(65);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";

    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": ["upsert", "promote", "relate"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C1 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // The world-less fixture: identical to the canonical entry fixture
    // except `extensions.nexus.world_id` is absent.
    let worldless = serde_json::json!({
        "schema_version": 1,
        "entry_id": "kb_m2",
        "entry_type": "character",
        "canonical_name": "Drifter",
        "status": "confirmed",
        "revision": null,
        "body": { "summary": "Drifter summary" },
        "extensions": { "nexus": {} },
    });

    match session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    entry_fixture("kb_m1", "Mira", WORLD_A, "confirmed", None),
                    worldless,
                ],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "mixed payload must be denied as a whole"
            );
        }
        other => panic!("mixed payload must be denied, got {other:?}"),
    }

    // Zero entries persisted — the scoped entry must NOT have been written
    // before the denial (whole-payload gate, no partial write).
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id IN ('kb_m1', 'kb_m2')",
    )
    .fetch_one(&pool)
    .await
    .expect("count persisted rows");
    assert_eq!(count, 0, "no partial write: zero entries persisted");

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C1 fix loop (L2 Important regression): `SERVED_OPS` is the load-bearing
/// serving gate — `dispatch()` consults it before routing, so the honesty
/// machine check (`n_c1_manifest_served_ops_match_dispatch_both_directions`)
/// transitively enforces manifest ⇔ actual dispatch routing. This test
/// pins the other half of that lockstep: EVERY op the const advertises must
/// actually round-trip through a dispatch match arm. Removing an arm (or
/// growing `SERVED_OPS` without a matching arm) makes the op fall through
/// to the `op_unsupported` refusal and this loop fails — drift the honesty
/// check alone cannot see, because it only compares manifest ⇔ const.
#[tokio::test(flavor = "multi_thread")]
async fn n_c1_every_served_op_advertised_by_the_const_actually_routes() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(66);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";

    // Hermetic workspace DB with the world seeded.
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // The peer is scoped to exactly the op set the const advertises (the
    // allowlist file is built from the const itself), so the loop below can
    // only fail if a served op does not route.
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": super::invoke::SERVED_OPS,
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C1 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // Relate's `kb_relationships` FKs require both endpoints to exist, so
    // pre-create the pair it references before the loop; the loop itself
    // then only uses fresh ids (order-independent of the const's iteration).
    let pair = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    entry_fixture("kb_loop_pair_1", "PairOne", WORLD_A, "confirmed", None),
                    entry_fixture("kb_loop_pair_2", "PairTwo", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await
        .expect("pre-create the relate pair");
    assert_eq!(
        pair.payload["knowledge_entries"][0]["entry_id"],
        "kb_loop_pair_1"
    );

    // The regression loop: every op the const advertises must route through
    // a dispatch match arm. A removed arm surfaces here as the `op_unsupported`
    // refusal (the gate passes the op, the match falls through) and fails
    // the invoke — the exact drift the const-binding prevents.
    for op in super::invoke::SERVED_OPS {
        let payload = match op {
            "upsert" => serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    entry_fixture("kb_loop_upsert", "LoopUpsert", WORLD_A, "confirmed", None),
                ],
            }),
            "promote" => serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "candidate": entry_fixture(
                    "kb_loop_promote",
                    "LoopPromote",
                    WORLD_A,
                    "provisional",
                    None,
                ),
            }),
            "relate" => serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "relation": {
                    "schema_version": 1,
                    "relation_id": "rel_loop_1",
                    "relation_type": "related_to",
                    "from_id": "kb_loop_pair_1",
                    "to_id": "kb_loop_pair_2",
                    "extensions": { "nexus": { "world_id": WORLD_A } },
                },
            }),
            other => panic!(
                "SERVED_OPS advertises op {other:?} but the routing-loop test has no \
                 payload fixture for it — add one so the new op is proven to route"
            ),
        };
        session.invoke(op, payload).await.unwrap_or_else(|error| {
            panic!(
                "SERVED_OPS advertises op {op:?} but dispatch does not route it \
                 (dispatch-arm drift?): {error:?}"
            )
        });
    }

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C1 fix loop (plan QC, QC1 F-001 + QC2 W-2 regression): the relate
/// CREATE path used to skip the stored-world gate entirely — the relation
/// row does not exist yet, so the relation-row world check is a no-op, and
/// `kb_relationships` FKs are single-column on `key_block_id` (world-
/// agnostic). A peer scoped ONLY to WORLD_A could therefore mint a world-A
/// relation whose endpoints are world-B entry ids (cross-world edge + an
/// id-existence oracle via insert success vs FK/`internal_error`
/// differential). The gate must resolve `from_id` / `to_id` on the create
/// path and require their stored worlds to equal the claimed relation
/// world; mismatch or missing endpoint denies the whole payload with zero
/// insert. Endpoints are immutable on the update path (the update port
/// carries no endpoint fields), so the create-path check closes the gap.
#[tokio::test(flavor = "multi_thread")]
async fn n_c1_relate_create_rejects_foreign_world_endpoints() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(67);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";
    const WORLD_B: &str = "wld_test_b";

    // Hermetic workspace DB with both worlds seeded.
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;
    seed_world(&pool, "ctr_test", WORLD_B).await;

    // Endpoint rows: a same-world pair in WORLD_A and a foreign pair in
    // WORLD_B (the peer is scoped to WORLD_A only — the world-B entries
    // must be unreachable as relation endpoints).
    seed_key_block(&pool, "kb_a_e1", WORLD_A, "Mira", "confirmed", 1).await;
    seed_key_block(&pool, "kb_a_e2", WORLD_A, "Ashford", "confirmed", 1).await;
    seed_key_block(&pool, "kb_b_e1", WORLD_B, "Banished", "confirmed", 1).await;
    seed_key_block(&pool, "kb_b_e2", WORLD_B, "Seer", "confirmed", 1).await;

    // Allowlist file: the peer is world-scoped to WORLD_A only.
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": ["upsert", "promote", "relate"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C1 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    let relate_payload = |relation_id: &str, from_id: &str, to_id: &str| {
        serde_json::json!({
            "extensions": { "nexus": { "peer_id": peer_claim } },
            "relation": {
                "schema_version": 1,
                "relation_id": relation_id,
                "relation_type": "related_to",
                "from_id": from_id,
                "to_id": to_id,
                "extensions": { "nexus": { "world_id": WORLD_A } },
            },
        })
    };

    // 1. Both endpoints in WORLD_B: denied (`op_unsupported` family).
    match session
        .invoke("relate", relate_payload("rel_new_b", "kb_b_e1", "kb_b_e2"))
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "cross-world relate create must be denied"
            );
        }
        other => panic!("cross-world relate create must be denied, got {other:?}"),
    }

    // 2. Mixed endpoints (from WORLD_A, to WORLD_B): denied — BOTH
    //    endpoints must be verified, not just the first.
    match session
        .invoke(
            "relate",
            relate_payload("rel_new_mix", "kb_a_e1", "kb_b_e1"),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "mixed-world relate create must be denied"
            );
        }
        other => panic!("mixed-world relate create must be denied, got {other:?}"),
    }

    // 3. Missing endpoint (no stored row at all): denied — never surfaced
    //    as an FK/`internal_error` id-existence oracle.
    match session
        .invoke(
            "relate",
            relate_payload("rel_new_ghost", "kb_ghost", "kb_a_e1"),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "relate to a missing endpoint must be denied"
            );
        }
        other => panic!("relate to a missing endpoint must be denied, got {other:?}"),
    }

    // Zero inserts: none of the denied relations persisted.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_relationships \
         WHERE relationship_id IN ('rel_new_b', 'rel_new_mix', 'rel_new_ghost')",
    )
    .fetch_one(&pool)
    .await
    .expect("count relation rows");
    assert_eq!(count, 0, "denied relates must insert zero rows");

    // 4. Same-world relate (both endpoints in WORLD_A) still succeeds —
    //    denials consumed sequences but the session stays usable and the
    //    legitimate create path is not over-blocked.
    let ok = session
        .invoke("relate", relate_payload("rel_new_a", "kb_a_e1", "kb_a_e2"))
        .await
        .expect("same-world relate create is served");
    assert_eq!(ok.payload["relation"]["relation_id"], "rel_new_a");
    let stored: (String, String, String) = sqlx::query_as(
        "SELECT world_id, source_entity_id, target_entity_id FROM kb_relationships \
         WHERE relationship_id = ?",
    )
    .bind("rel_new_a")
    .fetch_one(&pool)
    .await
    .expect("read persisted relation");
    assert_eq!(
        stored,
        (
            WORLD_A.to_string(),
            "kb_a_e1".to_string(),
            "kb_a_e2".to_string()
        ),
        "same-world relation persisted in the claimed world"
    );

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// R2 source assertion (V1.154 P1): the invoke bridge must not use the
/// banned worker-blocking `block_in_place` bridge — spec §5.3 locks a
/// per-process `spawn_blocking` lane bounded by a `Semaphore` instead.
/// Checked against the handler source itself, so any reintroduction fails
/// this test.
#[test]
fn invoke_bridge_source_has_no_block_in_place() {
    let source = include_str!("invoke.rs");
    assert!(
        !source.contains("block_in_place"),
        "invoke.rs must keep the bounded spawn_blocking bridge (R2): \
         block_in_place is banned in the invoke path"
    );
}
