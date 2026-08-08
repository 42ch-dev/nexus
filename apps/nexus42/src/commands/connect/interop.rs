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
use nexus_spoke_adapter::{HostManifestPort, NexusAdapter, SpokeResult};
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

/// A wire-shape `KnowledgeEntry` carrying the flat `body.attributes` the
/// basic-combat WASM module's manifest requires (`max_hp` / `base_atk` /
/// `base_def`) plus a `body.state.character` block — the P2 compute
/// round-trip fixture (the plain [`entry_fixture`] shape has no attributes
/// and would fail the module's input validation).
fn combat_entry_fixture(
    entry_id: &str,
    world_id: &str,
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
            "state": {
                "character": { "current_hp": max_hp, "max_hp": max_hp },
            },
        },
        "extensions": { "nexus": { "world_id": world_id } },
    })
}

/// Install one embedded module into the hermetic home's host-local module
/// store (`~/.nexus42/modules/<id>/<id>.wasm` + `manifest.json`) — the
/// operator-install step the P2 compute route requires (spec §2.1: the peer
/// can name only a module already installed under `~/.nexus42/modules/`).
#[cfg(not(nexus42_no_wasm_target))]
async fn install_test_module(home: &std::path::Path, module_id: &str) {
    install_test_module_as(home, module_id, module_id).await;
}

/// Like [`install_test_module`] but under an arbitrary store id: copies the
/// embedded bytes of `source_id` into `<store>/<module_id>/`. Used to prove
/// the module-id pin denies an unrelated INSTALLED module (only the store
/// id differs — the bytes are the embedded `basic-combat` ones).
#[cfg(not(nexus42_no_wasm_target))]
async fn install_test_module_as(home: &std::path::Path, module_id: &str, source_id: &str) {
    let dir = nexus_home_layout::user_modules_dir(home).join(module_id);
    std::fs::create_dir_all(&dir).expect("mkdir module store dir");
    let bytes = nexus_wasm_host::embedded_module_bytes(source_id)
        .unwrap_or_else(|| panic!("embedded module {source_id:?} must ship bytes"));
    let manifest = nexus_wasm_host::embedded_module_manifest(source_id)
        .unwrap_or_else(|| panic!("embedded module {source_id:?} must ship a manifest"));
    std::fs::write(dir.join(format!("{module_id}.wasm")), bytes).expect("write module wasm");
    std::fs::write(dir.join("manifest.json"), manifest).expect("write module manifest");
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
    // delivered N-C2 read-half slice + the enlarged served op set).
    let wire = serde_json::to_value(session.remote_manifest()).expect("manifest serializes");
    assert_eq!(wire["host_id"], serde_json::json!(TEST_HOST_ID));
    assert_eq!(wire["schema_version"], serde_json::json!(1));
    assert_eq!(
        wire["roles"],
        serde_json::json!(["data-store", "checker", "assembler", "computable-engine"])
    );
    assert_eq!(
        wire["capabilities"],
        serde_json::json!(["spoke-baseline", "l2-computable", "l5-fork"])
    );
    assert_eq!(wire["namespaces"], serde_json::json!(["nexus"]));
    assert_eq!(
        wire["extensions"]["nexus"]["connect_host_slice"],
        serde_json::json!("n-c2")
    );
    assert_eq!(
        wire["extensions"]["nexus"]["served_ops"],
        serde_json::json!(["upsert", "promote", "relate", "check", "assemble", "compute"])
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

/// N-C1 → N-C2 E2 honesty machine-check (P1 spec § Manifest honesty + P2
/// spec §4, both directions): the manifest a peer reads off the signed
/// hello must advertise exactly the ops the invoke dispatcher serves
/// (`upsert`/`promote`/`relate`/`check`/`assemble`/`compute` — the full
/// N-C2 E2 set). (a) Every op the manifest advertises
/// (`extensions.nexus.served_ops`) is actually served by the dispatch; (b)
/// every op the dispatch serves (`super::invoke::SERVED_OPS`) is advertised
/// by the manifest. The manifest comes from the single shared builder
/// (`build_connect_hello_manifest` — the same bytes `connect start` puts in
/// `ConnectConfig.local_manifest`), so this is the wire-truth cross-crate
/// check; the crate-level honesty test (`n_c1_manifest_is_honest` in
/// `nexus-spoke-adapter`) covers the manifest-side contract (exact op list +
/// production-orchestrator backing + the `computable-engine` role). Product
/// lock (spec §3/§5.6): the literal `"reasoning-complete"` string stays
/// absent — the semantic milestone is `computable-engine` +
/// `l2-computable`.
///
/// No network needed — the builder is host_id-injectable and hermetic.
#[test]
fn n_c1_manifest_served_ops_match_dispatch_both_directions() {
    let manifest = nexus_manifest(TEST_HOST_ID);
    let wire = serde_json::to_value(&manifest).expect("manifest serializes");
    let wire_string = wire.to_string();
    assert!(
        !wire_string.contains("reasoning-complete"),
        "the literal \"reasoning-complete\" MUST stay absent from the wire manifest \
         (product lock — the semantic milestone is computable-engine + l2-computable)"
    );
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
    let (config, host_id, allowlist_len, _) = super::build_host_config(
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
    assert_eq!(
        manifest_json["roles"],
        serde_json::json!(["data-store", "checker", "assembler", "computable-engine"])
    );

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

/// The N-C3 peer-list assertion helper: the port answers `Ok` (a reject is
/// a test failure — `SpokeResult` has no `expect`).
async fn assert_peer_list_ok(
    adapter: &NexusAdapter<'_>,
) -> Vec<spoke_schemas::HostCapabilityManifest> {
    match adapter.list_peer_host_capability_manifests().await {
        SpokeResult::Ok(peers) => peers,
        SpokeResult::Reject(r) => panic!("peer list is Ok: {r:?}"),
    }
}

/// N-C3 (V1.155 P0): two-node bidirectional-outbound peer recording over
/// real Connect sessions. Each side is a full nexus Connect Host (persisted
/// identity + device-id host_id + hermetic workspace DB + per-process
/// adapter, all through the production `build_host_config` boot).
///
/// - A dials B → the outbound `connect()` return carries B's manifest
///   (`PeerSession::remote_manifest()`), recorded into A's store
///   (`record_dialed_peer` — the production wiring at `connect()` return).
/// - B dials A → B's store records A's manifest.
/// - Each side's `list_peer_host_capability_manifests` returns the dialed
///   peer's manifest (the AC-1 honesty contract).
///
/// Bidirectional-outbound is deliberate (spec lock #1 fallback): an inbound
/// session (peer dials us) carries no manifest at the invoke boundary, so
/// an inbound-only peer is NOT recorded — asserted: after A dials B, B's
/// store stays empty until B itself dials A.
#[tokio::test(flavor = "multi_thread")]
async fn n_c3_two_node_bidirectional_outbound_records_dialed_peer_manifests() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home_a = temp.path().join("host-a");
    let home_b = temp.path().join("host-b");
    std::fs::create_dir_all(&home_a).expect("mkdir host-a");
    std::fs::create_dir_all(&home_b).expect("mkdir host-b");

    // Pre-create both persisted identities so each host can allowlist the
    // other before boot (the CLI boot generates the key create-once).
    let peer_a = identity::load_or_create_identity(&home_a)
        .expect("identity A")
        .public()
        .to_peer_id();
    let peer_b = identity::load_or_create_identity(&home_b)
        .expect("identity B")
        .public()
        .to_peer_id();

    let db_a = home_a.join("workspace").join("state.db");
    let db_b = home_b.join("workspace").join("state.db");
    let (config_a, host_id_a, _, adapter_a) = super::build_host_config(
        &home_a,
        &[peer_b.to_string()],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_a),
    )
    .await
    .expect("host A config builds");
    let (config_b, host_id_b, _, adapter_b) = super::build_host_config(
        &home_b,
        &[peer_a.to_string()],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_b),
    )
    .await
    .expect("host B config builds");

    let node_a = start(config_a).await;
    let node_b = start(config_b).await;

    // Both stores start empty (the port's empty-store contract).
    assert!(
        assert_peer_list_ok(&adapter_a).await.is_empty(),
        "A's store starts empty"
    );
    assert!(
        assert_peer_list_ok(&adapter_b).await.is_empty(),
        "B's store starts empty"
    );

    // A dials B: the outbound connect() return records B's manifest into
    // A's store (the production wiring at the observation point).
    let session_ab = node_a
        .connect(node_b.listen_addrs()[0].clone())
        .await
        .expect("A dials B");
    super::record_dialed_peer(&adapter_a, &session_ab)
        .await
        .expect("A records B at connect() return");
    let manifest_b = session_ab.remote_manifest();

    let peers = assert_peer_list_ok(&adapter_a).await;
    assert_eq!(peers.len(), 1, "A's store records exactly the dialed peer");
    assert_eq!(
        peers[0].host_id.as_str(),
        manifest_b.host_id.as_str(),
        "A's list returns the dialed peer's manifest"
    );
    assert_eq!(
        peers[0].capabilities, manifest_b.capabilities,
        "capabilities round-trip through the typed wire"
    );
    assert_ne!(
        peers[0].host_id.as_str(),
        host_id_a,
        "honesty: A's own host_id is never recorded"
    );

    // Inbound-only observation (A dialed B ⇒ B saw an inbound session) is
    // NOT recorded — the invoke boundary carries no manifest (lock #1
    // fallback): B's store is still empty.
    assert!(
        assert_peer_list_ok(&adapter_b).await.is_empty(),
        "inbound-only peers are not recorded (spec lock #1 fallback)"
    );

    // B dials A: B's store records A's manifest.
    let session_ba = node_b
        .connect(node_a.listen_addrs()[0].clone())
        .await
        .expect("B dials A");
    super::record_dialed_peer(&adapter_b, &session_ba)
        .await
        .expect("B records A at connect() return");
    let manifest_a = session_ba.remote_manifest();

    let peers = assert_peer_list_ok(&adapter_b).await;
    assert_eq!(peers.len(), 1, "B's store records exactly the dialed peer");
    assert_eq!(
        peers[0].host_id.as_str(),
        manifest_a.host_id.as_str(),
        "B's list returns the dialed peer's manifest"
    );
    assert_eq!(
        peers[0].capabilities, manifest_a.capabilities,
        "capabilities round-trip through the typed wire"
    );
    assert_ne!(
        peers[0].host_id.as_str(),
        host_id_b,
        "honesty: B's own host_id is never recorded"
    );

    // A's list is untouched by B's dial (still exactly B's manifest).
    let peers = assert_peer_list_ok(&adapter_a).await;
    assert_eq!(peers.len(), 1);
    assert_eq!(
        peers[0].host_id.as_str(),
        manifest_b.host_id.as_str(),
        "A's store keeps the dialed peer's manifest"
    );

    node_a.shutdown().await.expect("host A shuts down");
    node_b.shutdown().await.expect("host B shuts down");
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
    let (config, _, _, _) = super::build_host_config(
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

    // 7. Non-served op (N-C0 refusal contract extends into the handler):
    //    `project` stays refused. `compute` is served as of P2 but this
    //    peer's op_scope lists only the write ops, so it is still denied
    //    through the op-scope gate (op_unsupported — fail-closed).
    assert_op_unsupported(&session, "compute").await;

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
    let (config, _, _, _) = super::build_host_config(
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
    assert_eq!(
        claimless.payload["knowledge_entries"][0]["entry_id"],
        "kb_s2"
    );

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
    assert_eq!(
        matching.payload["knowledge_entries"][0]["entry_id"],
        "kb_s3"
    );

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

    let (config, _, _, _) = super::build_host_config(
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

    let (config, _, _, _) = super::build_host_config(
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

    // Must match the ComputeInput world_ref pattern `^wld_[a-zA-Z0-9]+$`
    // (the loop's compute arm reaches WASM execution).
    const WORLD_A: &str = "wld_loop1";

    // Hermetic workspace DB with the world seeded.
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // The peer is scoped to exactly the op set the const advertises (the
    // allowlist file is built from the const itself), so the loop below can
    // only fail if a served op does not route. The module_scope allowlists
    // basic-combat for the compute arm (P2: compute is a served op).
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": super::invoke::SERVED_OPS,
            "module_scope": ["basic-combat"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    // P2 compute: install the host-local module store entry so the compute
    // arm can round-trip (spec §2.1 — never peer-supplied bytes). When the
    // wasm target is absent there are no embedded bytes to install and the
    // compute arm is excluded from the loop (the cfg-gated round-trip test
    // covers it wherever the target exists).
    let mut loop_ops: Vec<&str> = super::invoke::SERVED_OPS.to_vec();
    if nexus_wasm_host::embedded_module_bytes("basic-combat").is_some() {
        install_test_module(home, "basic-combat").await;
        // Stage the compute session the loop's compute arm targets (project
        // is not a served op; the session row is the staging surface).
        nexus_local_db::compute_session::insert_compute_session(
            &pool,
            "ses_loop_compute",
            "kb_loop_pair_1",
            &serde_json::json!({
                "module_id": "basic-combat",
                "attacker_id": "kb_loop_pair_1",
                "defender_id": "kb_loop_pair_2",
            })
            .to_string(),
        )
        .await
        .expect("stage compute session for the routing loop");
    } else {
        loop_ops.retain(|op| *op != "compute");
    }

    let (config, _, _, _) = super::build_host_config(
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
    // The pair is seeded with combat attributes so the compute arm's WASM
    // invocation passes the module's manifest input validation.
    let pair = session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    combat_entry_fixture("kb_loop_pair_1", WORLD_A, 100, 20, 10),
                    combat_entry_fixture("kb_loop_pair_2", WORLD_A, 30, 5, 5),
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
    for op in loop_ops {
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
            "check" => serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "rule_refs": [],
            }),
            "assemble" => serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
            }),
            "compute" => serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_loop_compute",
                "entry_id": "kb_loop_pair_1",
                "computable": {
                    "attacker_id": "kb_loop_pair_1",
                    "defender_id": "kb_loop_pair_2",
                },
                "settle": false,
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

    let (config, _, _, _) = super::build_host_config(
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

/// N-C2 (V1.154 P1): the `check` op round-trips over Connect through the
/// real handler. The peer is scoped to WORLD_A with the full served-op set;
/// the invoke payload deserializes directly into `spoke_schemas::CheckRequest`
/// (spec §5.1 lock) and runs `orchestrate_check` with the production
/// baseline no-op checker (the V1.148 daemon cutover shape) — the response
/// carries the orchestrator's findings (empty for the baseline checker) and
/// the checked world's data flowed through the read ports.
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_peer_runs_check_over_connect() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(70);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";

    // Hermetic workspace DB with the world seeded (FK rows for the read
    // ports and the finding-persist path).
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // The peer is scoped to WORLD_A with exactly the const's served-op set.
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

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // Seed one entry so the orchestrator's read paths
    // (list_knowledge_entries / list_timeline_events) run against data.
    session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [entry_fixture("kb_c2_1", "Checked", WORLD_A, "confirmed", None)],
            }),
        )
        .await
        .expect("seed entry for the check");

    // check round-trip: scope.scope_id is the world selector (spec §5.1 —
    // the schema's scope object, not an ad-hoc world field); the baseline
    // checker produces zero findings.
    let checked = session
        .invoke(
            "check",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "rule_refs": [],
                "checker_kinds": ["baseline"],
            }),
        )
        .await
        .expect("scoped check is served");
    assert_eq!(
        checked.payload["findings"],
        serde_json::json!([]),
        "baseline checker produces zero findings"
    );

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P1): the `assemble` op round-trips over Connect through the
/// real handler. The payload deserializes directly into
/// `spoke_schemas::AssembleRequest` (spec §5.1 lock) and runs
/// `orchestrate_assemble` — the response carries the assembled packet, and
/// `max_entries` flows through as the packet truncation hint (spec §5.4's
/// amplification guard: a huge max_entries is capped before the orchestrator).
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_peer_runs_assemble_over_connect() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(71);
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
            "op_scope": super::invoke::SERVED_OPS,
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // Two entries in the world so the packet has content.
    session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    entry_fixture("kb_c2_a1", "AssembledOne", WORLD_A, "confirmed", None),
                    entry_fixture("kb_c2_a2", "AssembledTwo", WORLD_A, "confirmed", None),
                ],
            }),
        )
        .await
        .expect("seed entries for the assemble");

    // assemble round-trip, no truncation: the packet carries both entries
    // and the orchestrator-derived packet id (scope-anchored).
    let assembled = session
        .invoke(
            "assemble",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "max_entries": 10,
            }),
        )
        .await
        .expect("scoped assemble is served");
    assert_eq!(
        assembled.payload["packet"]["packet_id"],
        serde_json::json!("assemble:wld_test_a"),
        "packet id is derived from the request scope"
    );
    let entries = assembled.payload["packet"]["entries"]
        .as_array()
        .expect("packet entries array");
    assert_eq!(entries.len(), 2, "both seeded entries assembled");

    // max_entries flows through as the truncation hint (packet builder
    // truncates, not the wire).
    let truncated = session
        .invoke(
            "assemble",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "max_entries": 1,
            }),
        )
        .await
        .expect("scoped assemble with max_entries is served");
    let truncated_entries = truncated.payload["packet"]["entries"]
        .as_array()
        .expect("packet entries array");
    assert_eq!(
        truncated_entries.len(),
        1,
        "max_entries=1 must truncate the assembled packet"
    );

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P1) read world-scoping (spec §5.5 — fail-closed reads,
/// identical to writes): `check` / `assemble` require the session peer's
/// `world_scope` to contain the request `scope.scope_id` before either
/// orchestrator is called. Wrong-world scope ⇒ `op_unsupported`; absent
/// scope object / missing scope_id ⇒ `op_unsupported` (cannot verify
/// scope). All denials have zero side effects and leave the session usable.
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_check_and_assemble_wrong_world_and_absent_scope_denied() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(72);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";
    const WORLD_B: &str = "wld_test_b";

    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;
    seed_world(&pool, "ctr_test", WORLD_B).await;

    // The peer is scoped to WORLD_A only — WORLD_B reads must be denied.
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

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // 1. Wrong-world check: scope.scope_id = WORLD_B, peer scoped to
    //    WORLD_A ⇒ denied before the orchestrator (zero side effects).
    match session
        .invoke(
            "check",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_B },
                "rule_refs": [],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "wrong-world check must be denied"
            );
        }
        other => panic!("wrong-world check must be denied, got {other:?}"),
    }

    // 2. Wrong-world assemble: same fail-closed rule on reads.
    match session
        .invoke(
            "assemble",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_B },
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "wrong-world assemble must be denied"
            );
        }
        other => panic!("wrong-world assemble must be denied, got {other:?}"),
    }

    // 3. Absent scope object ⇒ cannot verify the world ⇒ denied.
    match session
        .invoke(
            "check",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "rule_refs": [],
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "check without a scope object must be denied"
            );
        }
        other => panic!("check without scope must be denied, got {other:?}"),
    }

    // 4. Scope object without scope_id ⇒ cannot verify the world ⇒ denied.
    match session
        .invoke(
            "assemble",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": {},
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => {
            assert_eq!(
                envelope.code, "op_unsupported",
                "assemble without scope_id must be denied"
            );
        }
        other => panic!("assemble without scope_id must be denied, got {other:?}"),
    }

    // Zero side effects: no findings rows persisted by any denied check.
    let findings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings")
        .fetch_one(&pool)
        .await
        .expect("count findings rows");
    assert_eq!(findings, 0, "denied checks must persist zero findings");

    // The session stays usable: a same-world check is served afterwards.
    let served = session
        .invoke(
            "check",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "rule_refs": [],
            }),
        )
        .await
        .expect("same-world check still served after denials");
    assert_eq!(served.payload["findings"], serde_json::json!([]));

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P2) refusal matrix: through the real handler, `project`
/// and unknown ops are refused with `op_unsupported` and zero side effects
/// — even for a peer whose `op_scope` covers the full served set (the
/// SERVED_OPS gate refuses before any scope logic). `compute` is SERVED as
/// of P2: a malformed compute payload passes the served-op gate and maps
/// through the typed parse to `invalid_input` (NOT `op_unsupported`),
/// pinning that the gate really admits it. The session stays usable.
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_refusal_matrix_project_and_unknown_ops() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(73);
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
            "op_scope": super::invoke::SERVED_OPS,
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");

    // project and unknown ops are refused by the served-op gate regardless
    // of op_scope / payload shape.
    for op in ["project", "garbage-op"] {
        assert_op_unsupported(&session, op).await;
    }

    // compute IS served (P2): a malformed compute payload passes the
    // served-op gate and fails the typed ComputeRequest parse — mapped to
    // the locked `invalid_input` envelope, not `op_unsupported` (pins the
    // gate admits compute; the parse is the next gate).
    match session
        .invoke("compute", serde_json::json!({ "extensions": {} }))
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "invalid_input",
            "a malformed compute payload must map to invalid_input (compute is served)"
        ),
        other => panic!("malformed compute expected invalid_input, got {other:?}"),
    }

    // Refusals consumed sequences but left the session open: a served op
    // still round-trips afterwards.
    let served = session
        .invoke(
            "check",
            serde_json::json!({
                "scope": { "scope_id": WORLD_A },
                "rule_refs": [],
            }),
        )
        .await
        .expect("session stays usable after refusals");
    assert_eq!(served.payload["findings"], serde_json::json!([]));

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P2, spec §2): the `compute` op round-trips over Connect
/// through the real handler against the host-local `basic-combat` module
/// installed under the hermetic home's `~/.nexus42/modules/` (spec §2.1 —
/// the peer names an installed module; bytes are never peer-supplied).
/// The payload deserializes directly into `spoke_schemas::ComputeRequest`
/// (spec §2.2 lock) and runs `orchestrate_compute` through the adapter's
/// `ComputablePort` on the P1 bounded lane. The response is deterministic:
/// ATK 20 − DEF 5 = 15 damage applied to the defender's 30 HP → 15, and
/// `settle: false` returns no settled state (read-only compute lock).
///
/// Gated on the wasm32 target exactly like the adapter's own embedded-WASM
/// tests (`nexus42_no_wasm_target` — the module store entry cannot be
/// installed without the embedded bytes).
#[cfg(not(nexus42_no_wasm_target))]
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_peer_runs_compute_over_connect() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(80);
    let peer_peer = peer_key.public().to_peer_id();

    // The world id must match the ComputeInput world_ref pattern
    // `^wld_[a-zA-Z0-9]+$` (underscores are rejected by the wire type).
    const WORLD_A: &str = "wld_rt1";

    // Operator-install step: the module the peer will name must already be
    // installed under `~/.nexus42/modules/` (host-local store, fail-closed).
    install_test_module(home, "basic-combat").await;

    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // The peer is scoped to WORLD_A with the full served-op set AND a
    // `module_scope` allowlisting basic-combat (architect lock, spec §6.1 —
    // missing/empty module_scope denies ALL compute).
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": super::invoke::SERVED_OPS,
            "module_scope": ["basic-combat"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // Seed the two combatants over the wire (the plain entry_fixture has no
    // attributes and would fail the module's manifest input validation).
    session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    combat_entry_fixture("kb_atk", WORLD_A, 100, 20, 10),
                    combat_entry_fixture("kb_def", WORLD_A, 30, 5, 5),
                ],
            }),
        )
        .await
        .expect("seed combatants");

    // Stage the compute session directly (project is not a served op — the
    // session row is the out-of-band staging surface; `module_id` lives in
    // the staged state per the locked resolution precedence, spec §2.2).
    let state = serde_json::json!({
        "module_id": "basic-combat",
        "attacker_id": "kb_atk",
        "defender_id": "kb_def",
        "character": { "current_hp": 30, "max_hp": 30 },
    });
    nexus_local_db::compute_session::insert_compute_session(
        &pool,
        "ses_combat",
        "kb_atk",
        &state.to_string(),
    )
    .await
    .expect("stage compute session");

    // compute round-trip: the deterministic combat math lands in the
    // response's merged computable state (30 HP − 15 damage = 15).
    let computed = session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_combat",
                "entry_id": "kb_atk",
                "computable": { "attacker_id": "kb_atk", "defender_id": "kb_def" },
                "settle": false,
            }),
        )
        .await
        .expect("scoped compute is served");
    assert_eq!(computed.payload["session_id"], "ses_combat");
    assert_eq!(computed.payload["entry_id"], "kb_atk");
    assert_eq!(
        computed.payload["computable"]["character"]["current_hp"], 15,
        "deterministic combat delta (ATK 20 − DEF 5 = 15) applied to the merged state"
    );
    assert_eq!(
        computed.payload.get("state"),
        None,
        "settle:false returns no settled state (read-only compute lock — the empty state map is omitted on the wire)"
    );

    // Read-only compute: the stored defender entry is untouched (no settle).
    let stored_hp: Option<i64> = sqlx::query_scalar(
        "SELECT json_extract(body_json, '$.state.character.current_hp') \
         FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind("kb_def")
    .fetch_optional(&pool)
    .await
    .expect("read stored defender state");
    assert_eq!(
        stored_hp,
        Some(30),
        "compute with settle:false must not mutate the stored entry"
    );

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P2) compute denial matrix — world + module gates (spec
/// §2.1–§2.3): wrong-world ⇒ `op_unsupported` (the same fail-closed family
/// as every other op); missing module name ⇒ defined `module_not_found`;
/// module not installed under `~/.nexus42/modules/` ⇒ defined
/// `module_not_found`; `settle: true` ⇒ defined `settle_not_enabled`
/// (read-only compute lock, spec §5 / §6.5). All denials happen before any
/// WASM execution with zero side effects, and the session stays usable.
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_compute_wrong_world_missing_module_uninstalled_and_settle_denied() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(81);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";
    const WORLD_B: &str = "wld_test_b";

    // NOTE: no module is installed in this home — the module-scope'd peer
    // still exists, so the not-installed denial is reachable.
    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;
    seed_world(&pool, "ctr_test", WORLD_B).await;

    // The peer is scoped to WORLD_A with the full served-op set and a
    // `module_scope` allowlisting basic-combat — which is NOT installed in
    // this hermetic home (fail-closed: the gate must deny before execution).
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": super::invoke::SERVED_OPS,
            "module_scope": ["basic-combat"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    // A same-world combatant for the module-scope'd scenarios.
    session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    combat_entry_fixture("kb_cmp_a", WORLD_A, 100, 20, 10),
                ],
            }),
        )
        .await
        .expect("seed same-world combatant");

    // (b) Wrong-world: the target entry is stored in WORLD_B (seeded
    // directly — the peer cannot write there), so the stored-world gate
    // denies with the same op_unsupported family as every other op.
    seed_key_block(&pool, "kb_cmp_b", WORLD_B, "Banished", "confirmed", 1).await;
    nexus_local_db::compute_session::insert_compute_session(
        &pool,
        "ses_wrong_world",
        "kb_cmp_b",
        &serde_json::json!({ "module_id": "basic-combat" }).to_string(),
    )
    .await
    .expect("stage wrong-world session");
    match session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_wrong_world",
                "entry_id": "kb_cmp_b",
                "computable": {},
                "settle": false,
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "op_unsupported",
            "wrong-world compute must be denied like every other op"
        ),
        other => panic!("wrong-world compute must be denied, got {other:?}"),
    }

    // (d) Missing module name: the staged session carries no `module_id`
    // and the entry has no `body.computable` — the locked resolution
    // precedence finds no module identity ⇒ defined module_not_found.
    nexus_local_db::compute_session::insert_compute_session(
        &pool,
        "ses_no_module",
        "kb_cmp_a",
        "{}",
    )
    .await
    .expect("stage no-module session");
    match session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_no_module",
                "entry_id": "kb_cmp_a",
                "computable": {},
                "settle": false,
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "module_not_found",
            "a compute request with no module identity must be denied with module_not_found"
        ),
        other => panic!("missing-module compute must be denied, got {other:?}"),
    }

    // (d-ii) Module not installed: the resolved module IS in the peer's
    // module_scope, but the host-local store under ~/.nexus42/modules/ does
    // not contain it ⇒ defined module_not_found (never peer-supplied bytes).
    nexus_local_db::compute_session::insert_compute_session(
        &pool,
        "ses_uninstalled",
        "kb_cmp_a",
        &serde_json::json!({ "module_id": "basic-combat" }).to_string(),
    )
    .await
    .expect("stage uninstalled-module session");
    match session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_uninstalled",
                "entry_id": "kb_cmp_a",
                "computable": {},
                "settle": false,
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "module_not_found",
            "a scoped module that is not installed host-locally must be denied with module_not_found"
        ),
        other => panic!("uninstalled-module compute must be denied, got {other:?}"),
    }

    // (e) settle:true ⇒ defined settle_not_enabled (read-only compute lock,
    // spec §5 / §6.5 — the compute settlement helper is NOT enabled on the
    // N-C2 surface). The gate fires before any module/WASM work.
    match session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_uninstalled",
                "entry_id": "kb_cmp_a",
                "computable": {},
                "settle": true,
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "settle_not_enabled",
            "settle:true must be rejected on the read-only compute surface"
        ),
        other => panic!("settle:true compute must be rejected, got {other:?}"),
    }

    // Zero side effects: no session state advanced, no entry mutated — the
    // staged session rows still carry their original state_json.
    let stored: Option<String> =
        sqlx::query_scalar("SELECT state_json FROM compute_sessions WHERE session_id = ?")
            .bind("ses_uninstalled")
            .fetch_optional(&pool)
            .await
            .expect("read staged session");
    assert_eq!(
        stored.as_deref(),
        Some(r#"{"module_id":"basic-combat"}"#),
        "denied computes must not advance session state"
    );

    // The session stays usable: a served op still round-trips afterwards.
    let served = session
        .invoke(
            "check",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "rule_refs": [],
            }),
        )
        .await
        .expect("session stays usable after compute denials");
    assert_eq!(served.payload["findings"], serde_json::json!([]));

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P2) unscoped-module denial (architect lock, spec §6.1):
/// a peer whose allowlist entry has NO `module_scope` (absent ⇒ empty ⇒
/// fail-closed) is denied ALL compute with the defined `module_not_scoped`
/// envelope — even when the resolved module would otherwise be valid. The
/// session stays usable.
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_compute_unscoped_module_denied() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(82);
    let peer_peer = peer_key.public().to_peer_id();

    const WORLD_A: &str = "wld_test_a";

    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // The allowlist entry deliberately omits `module_scope` (the V1.153 →
    // V1.154 file shape without the new field — backward-compatible parse,
    // fail-closed semantics: no module access).
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

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    combat_entry_fixture("kb_usc_a", WORLD_A, 100, 20, 10),
                ],
            }),
        )
        .await
        .expect("seed combatant");

    nexus_local_db::compute_session::insert_compute_session(
        &pool,
        "ses_unscoped",
        "kb_usc_a",
        &serde_json::json!({ "module_id": "basic-combat" }).to_string(),
    )
    .await
    .expect("stage session");

    match session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_unscoped",
                "entry_id": "kb_usc_a",
                "computable": {},
                "settle": false,
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "module_not_scoped",
            "a peer without module_scope must be denied ALL compute (fail-closed)"
        ),
        other => panic!("unscoped-module compute must be denied, got {other:?}"),
    }

    // The session stays usable.
    let served = session
        .invoke(
            "check",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "rule_refs": [],
            }),
        )
        .await
        .expect("session stays usable after the module-scope denial");
    assert_eq!(served.payload["findings"], serde_json::json!([]));

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P2, L2 review C-1 regression): the module-id pin. The
/// adapter's `ComputablePort::compute` merges `request.computable` over the
/// session state before re-resolving the module id, so a request-carried
/// `computable.module_id` naming a DIFFERENT installed module would execute
/// an unscoped module. The gate must deny the override with the defined
/// `module_not_scoped` envelope before any WASM execution — even though the
/// override names an installed module and the staged session id is in
/// scope. Zero side effects; session stays usable.
#[cfg(not(nexus42_no_wasm_target))]
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_compute_request_module_override_denied() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(83);
    let peer_peer = peer_key.public().to_peer_id();

    // Must match the ComputeInput world_ref pattern `^wld_[a-zA-Z0-9]+$`.
    const WORLD_A: &str = "wld_pin1";

    // Both module ids are INSTALLED — without the pin, the override would
    // execute real WASM under the unscoped id.
    install_test_module(home, "basic-combat").await;
    install_test_module_as(home, "basic-combat-alt", "basic-combat").await;

    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // The peer's module_scope allowlists ONLY basic-combat.
    let allow_path = nexus_home_layout::connect_allowlist_path(home);
    std::fs::create_dir_all(allow_path.parent().expect("parent dir")).expect("mkdir");
    std::fs::write(
        &allow_path,
        serde_json::json!({ "peer_ids": [{
            "peer_id": peer_peer.to_string(),
            "world_scope": [WORLD_A],
            "op_scope": super::invoke::SERVED_OPS,
            "module_scope": ["basic-combat"],
        }] })
        .to_string(),
    )
    .expect("write allowlist");

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    session
        .invoke(
            "upsert",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "knowledge_entries": [
                    combat_entry_fixture("kb_pin_a", WORLD_A, 100, 20, 10),
                    combat_entry_fixture("kb_pin_d", WORLD_A, 30, 5, 5),
                ],
            }),
        )
        .await
        .expect("seed combatants");

    // The staged session declares the in-scope module id (plus the combat
    // invocation the module needs)...
    nexus_local_db::compute_session::insert_compute_session(
        &pool,
        "ses_pin",
        "kb_pin_a",
        &serde_json::json!({
            "module_id": "basic-combat",
            "attacker_id": "kb_pin_a",
            "defender_id": "kb_pin_d",
        })
        .to_string(),
    )
    .await
    .expect("stage session");

    // ...but the request's dynamic computable tries to override it with an
    // installed-but-unscoped module id. The pin must deny.
    match session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_pin",
                "entry_id": "kb_pin_a",
                "computable": { "module_id": "basic-combat-alt" },
                "settle": false,
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "module_not_scoped",
            "a request-carried module_id override outside the peer's module_scope must be denied"
        ),
        other => panic!("module override must be denied, got {other:?}"),
    }

    // P2 QC fix wave FW-1 regression: NON-STRING `module_id` overrides
    // (42 / {} / null) must ALSO be denied `module_not_scoped` — the old
    // as_str-only pin let them bypass while the execution-time merge still
    // shadowed the session-staged id with the non-string value. Key
    // presence is the pin trigger; the value must be a JSON string EQUAL to
    // the gated id, else deny (zero side effects — asserted below).
    for (label, override_value) in [
        ("number", serde_json::json!(42)),
        ("object", serde_json::json!({})),
        ("null", serde_json::Value::Null),
    ] {
        match session
            .invoke(
                "compute",
                serde_json::json!({
                    "extensions": { "nexus": { "peer_id": peer_claim } },
                    "session_id": "ses_pin",
                    "entry_id": "kb_pin_a",
                    "computable": { "module_id": override_value },
                    "settle": false,
                }),
            )
            .await
        {
            Err(InvokeError::Wire(envelope)) => assert_eq!(
                envelope.code, "module_not_scoped",
                "non-string module_id override ({label}) must be denied module_not_scoped"
            ),
            other => {
                panic!("non-string module_id override ({label}) must be denied, got {other:?}")
            }
        }
    }

    // Zero side effects: the staged session state is untouched (all four
    // denials above — string-differ + 42 / {} / null — must not advance it).
    let stored: Option<String> =
        sqlx::query_scalar("SELECT state_json FROM compute_sessions WHERE session_id = ?")
            .bind("ses_pin")
            .fetch_optional(&pool)
            .await
            .expect("read staged session");
    assert_eq!(
        stored.as_deref(),
        Some(r#"{"attacker_id":"kb_pin_a","defender_id":"kb_pin_d","module_id":"basic-combat"}"#),
        "the denied overrides must not advance session state"
    );

    // A same-id override (repeat of the gated id) is legal and served —
    // proves the pin compares, it does not blanket-ban computable.module_id.
    let served = session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_pin",
                "entry_id": "kb_pin_a",
                "computable": { "module_id": "basic-combat" },
                "settle": false,
            }),
        )
        .await
        .expect("a same-id module_id override is served");
    assert_eq!(served.payload["session_id"], "ses_pin");

    host.shutdown().await.expect("host shuts down");
    peer_node.shutdown().await.expect("peer shuts down");
}

/// N-C2 (V1.154 P2, P2 QC fix wave FW-3): a compute request targeting a
/// missing `entry_id` must be denied with the defined `invalid_input`
/// envelope (client-input family — the same code the check/assemble paths
/// use for client-input rejects) — never the `internal_error` the generic
/// reject table would have produced. The gate's stored-entry read fires
/// before any module/WASM work (no module scope or store is even needed);
/// the session stays usable. Runs on every CI leg (no wasm target needed —
/// the denial never reaches module resolution).
#[tokio::test(flavor = "multi_thread")]
async fn n_c2_compute_missing_entry_denied_invalid_input() {
    let _guard = network_test_guard().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    let peer_key = fixed_keypair(84);
    let peer_peer = peer_key.public().to_peer_id();

    // Must match the ComputeInput world_ref pattern `^wld_[a-zA-Z0-9]+$`.
    const WORLD_A: &str = "wld_miss1";

    let db_path = temp.path().join("workspace").join("state.db");
    let pool = crate::db::Schema::init(&db_path)
        .await
        .expect("workspace DB initializes");
    seed_world(&pool, "ctr_test", WORLD_A).await;

    // No module_scope / module store needed: the missing-entry denial fires
    // at the stored-entry gate, before module resolution.
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

    let (config, _, _, _) = super::build_host_config(
        home,
        &[],
        &["/ip4/127.0.0.1/tcp/0".to_string()],
        Some(&db_path),
    )
    .await
    .expect("N-C2 host config builds");
    let host_peer = config.identity.public().to_peer_id();
    let host = start(config).await;
    let peer_node = start(peer_config(peer_key, vec![host_peer])).await;
    let session = peer_node
        .connect(host.listen_addrs()[0].clone())
        .await
        .expect("scoped peer handshake");
    let peer_claim = serde_json::json!(peer_peer.to_string());

    match session
        .invoke(
            "compute",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "session_id": "ses_ghost",
                "entry_id": "kb_never_stored",
                "computable": {},
                "settle": false,
            }),
        )
        .await
    {
        Err(InvokeError::Wire(envelope)) => assert_eq!(
            envelope.code, "invalid_input",
            "compute on a missing entry must map to the invalid_input family, not internal_error"
        ),
        other => panic!("missing-entry compute must be denied, got {other:?}"),
    }

    // The session stays usable (a served op still round-trips).
    let served = session
        .invoke(
            "check",
            serde_json::json!({
                "extensions": { "nexus": { "peer_id": peer_claim } },
                "scope": { "scope_id": WORLD_A },
                "rule_refs": [],
            }),
        )
        .await
        .expect("session stays usable after the missing-entry denial");
    assert_eq!(served.payload["findings"], serde_json::json!([]));

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

/// R2 closure for compute (V1.154 P2, spec §2.4 / E1 P0 QC note): WASM must
/// never execute inline on a tokio worker — compute runs on the P1 bounded
/// `spawn_blocking` lane under the shared semaphore. Checked against the
/// handler source: the connect handler must not touch the WASM engine
/// directly (no `nexus_wasm_host` import, no engine/compute call), so the
/// only execution path is the adapter's `ComputablePort` invoked from
/// inside the lane closure. Any reintroduction of inline engine usage fails
/// this test.
#[test]
fn invoke_compute_executes_only_inside_the_bounded_lane() {
    let source = include_str!("invoke.rs");
    assert!(
        source.contains("tokio::task::spawn_blocking"),
        "invoke.rs must dispatch every served op — including compute — through \
         the bounded spawn_blocking lane"
    );
    assert!(
        !source.contains("nexus_wasm_host"),
        "invoke.rs must not import nexus_wasm_host: the Connect handler never \
         executes WASM inline — compute routes through the adapter's \
         ComputablePort inside the lane closure"
    );
    assert!(
        !source.contains("WasmEngine"),
        "invoke.rs must not construct or call a WasmEngine directly: WASM \
         execution stays on the bounded lane (off the tokio worker)"
    );
}
