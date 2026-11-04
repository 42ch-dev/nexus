//! Signed-loopback assembly tests: the full `MemoryPort` over a real
//! `connect_responder` session, the authenticated admission table, the D15
//! raw-request digest and the §14.2 error envelopes.
//!
//! Every call here rides the SDK's own signed Connect transport: the host is a
//! `connect_responder` serving the assembled fixture through the unchanged
//! `MemoryPortService`, and the client is the typed `MemoryPort` implementation
//! on `RemoteAdapter`. Nothing is mocked, forwarded or echoed — the provider
//! behind the service is the durable SQLite fixture, so each assertion is about
//! state that survives in that file.
//!
//! The one deliberately raw client is used for the D15 request-digest proof:
//! `source_context_ids: []` and an omitted member are the same typed request but
//! different wire objects, so only a raw signed invoke can send both.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use serde_json::{from_value, json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use spoke_connect::core::{derive_peer_id_from_ed25519_pubkey, sign_hello_ed25519};
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, loopback_transport_pair, ConnectResponder,
    ConnectResponderOptions, LoopbackTransport, LoopbackTransportPair, RemoteAdapter,
    RemoteAdapterOptions, RemoteIdentity, RemoteServePorts, RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::connect::{ConnectHello, ConnectSession};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, MemoryCaptureRequest, Principal, Relation,
    Rule, Scope, TimelineEvent,
};
use spoke_operations::{
    jcs_sha256_hex, FindingPort, HostManifestPort, KnowledgeEntryPort, MemoryPort, RelationPort,
    RuleQueryPort, ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult,
};
use tempfile::TempDir;

use nexus_memory_conformance::{FaultPoint, FixtureMemoryProvider, PeerDirectory};

/// The dialing client's connect identity.
const CLIENT_SEED: [u8; 32] = [0x31; 32];
/// A second allowlisted peer the operator deliberately leaves unmapped.
const UNMAPPED_SEED: [u8; 32] = [0x52; 32];
/// The host's connect identity.
const HOST_SEED: [u8; 32] = [0x42; 32];

/// The single capability both ends negotiate: the fixture serves no other
/// family.
const MEMORY_CAPABILITIES: [&str; 1] = ["spoke-memory"];

// ---------------------------------------------------------------------------
// A memory-only responder host
// ---------------------------------------------------------------------------

/// The baseline half of the responder's ports face.
///
/// This host serves `spoke-memory` only, so `spoke-baseline` is never
/// negotiated and the dispatch gate answers every `port.*` op before it could
/// reach this type. The implementation exists because `RemoteServePorts` is a
/// baseline-port supertrait, and it refuses honestly rather than fabricating
/// knowledge entries.
struct MemoryOnlyHost;

fn no_baseline<T>() -> SpokeResult<T> {
    SpokeResult::Reject(SpokeReject {
        code: SpokeRejectCode::CapabilityPortMissing,
        message: "the fixture host serves no baseline family".to_owned(),
        details: None,
    })
}

#[async_trait]
impl KnowledgeEntryPort for MemoryOnlyHost {
    async fn get_knowledge_entry(&self, _entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        no_baseline()
    }

    async fn put_knowledge_entry(
        &self,
        _entry: KnowledgeEntry,
        _expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        no_baseline()
    }
}

#[async_trait]
impl RelationPort for MemoryOnlyHost {
    async fn get_relation(&self, _relation_id: &str) -> SpokeResult<Relation> {
        no_baseline()
    }

    async fn put_relation(
        &self,
        _relation: Relation,
        _expected_base_revision: Option<u64>,
    ) -> SpokeResult<Relation> {
        no_baseline()
    }
}

#[async_trait]
impl ScopeQueryPort for MemoryOnlyHost {
    async fn list_knowledge_entries(&self, _scope: &Scope) -> SpokeResult<Vec<KnowledgeEntry>> {
        no_baseline()
    }

    async fn list_timeline_events(&self, _scope: &Scope) -> SpokeResult<Vec<TimelineEvent>> {
        no_baseline()
    }
}

#[async_trait]
impl FindingPort for MemoryOnlyHost {
    async fn put_findings(&self, _findings: Vec<Finding>) -> SpokeResult<Vec<Finding>> {
        no_baseline()
    }
}

#[async_trait]
impl RuleQueryPort for MemoryOnlyHost {
    async fn list_rules(&self, _rule_refs: &[String]) -> SpokeResult<Vec<Rule>> {
        no_baseline()
    }
}

#[async_trait]
impl HostManifestPort for MemoryOnlyHost {
    async fn get_host_capability_manifest(&self) -> SpokeResult<HostCapabilityManifest> {
        no_baseline()
    }

    async fn list_peer_host_capability_manifests(
        &self,
    ) -> SpokeResult<Vec<HostCapabilityManifest>> {
        no_baseline()
    }
}

// ---------------------------------------------------------------------------
// Session plumbing
// ---------------------------------------------------------------------------

fn manifest(host_id: &str, capabilities: &[&str]) -> HostCapabilityManifest {
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["data-store"],
        "capabilities": capabilities,
        "namespaces": [],
        "extensions": {},
    }))
    .expect("valid manifest")
}

fn connect_manifest(manifest: &HostCapabilityManifest) -> ConnectHelloManifest {
    serde_json::from_value(serde_json::to_value(manifest).expect("manifest serializes"))
        .expect("field-identical connect manifest")
}

type ConnectHelloManifest =
    spoke_operations::spoke_schemas::connect::connect_hello::HostCapabilityManifest;

fn peer_id(seed: &[u8; 32]) -> String {
    derive_peer_id_from_ed25519_pubkey(&SigningKey::from_bytes(seed).verifying_key().to_bytes())
}

fn pubkey(seed: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

fn principal(principal_id: &str) -> Principal {
    serde_json::from_value(json!({
        "principal_id": principal_id,
        "extensions": {},
    }))
    .expect("valid principal")
}

/// Start a signed responder serving `provider` for the listed client seeds.
async fn start_host(
    provider: &Arc<FixtureMemoryProvider>,
    directory: &Arc<PeerDirectory>,
    client_seeds: &[[u8; 32]],
    invoke_timeout_ms: Option<u64>,
) -> (Arc<ConnectResponder>, LoopbackTransportPair) {
    let pair = loopback_transport_pair();
    let mut allowlist = Vec::new();
    let mut peer_keys = HashMap::new();
    for seed in client_seeds {
        let id = peer_id(seed);
        allowlist.push(id.clone());
        peer_keys.insert(id, pubkey(seed));
    }
    let ports: Arc<dyn RemoteServePorts + Send + Sync> =
        Arc::new(
            RemoteServePortsComposite::new(Arc::new(MemoryOnlyHost), None, None)
                .with_memory(provider.server(directory.admission())),
        );
    let responder = connect_responder(ConnectResponderOptions {
        transport: Arc::new(pair.server.clone()),
        identity: RemoteIdentity { seed: HOST_SEED },
        manifest: manifest("fixture-conformance-host", &MEMORY_CAPABILITIES),
        allowlist,
        peer_keys,
        ports: Some(ports),
        invoke_timeout_ms,
    })
    .await;
    (responder, pair)
}

/// Dial the typed `MemoryPort` client over `pair`.
async fn dial(
    pair: LoopbackTransportPair,
    seed: [u8; 32],
    invoke_timeout_ms: Option<u64>,
) -> Arc<RemoteAdapter> {
    connect_remote_adapter(RemoteAdapterOptions {
        transport: Arc::new(pair.client),
        local_identity: RemoteIdentity { seed },
        local_manifest: manifest("fixture-conformance-client", &MEMORY_CAPABILITIES),
        remote_pubkey: pubkey(&HOST_SEED),
        allowlist: vec![peer_id(&HOST_SEED)],
        invoke_timeout_ms,
        capability_token: None,
    })
    .await
    .expect("dial the signed fixture host")
}

// ---------------------------------------------------------------------------
// The assembled fixture over a real signed session
// ---------------------------------------------------------------------------

struct Fixture {
    _home: TempDir,
    db_path: PathBuf,
    directory: Arc<PeerDirectory>,
    provider: Arc<FixtureMemoryProvider>,
    responder: Arc<ConnectResponder>,
    client: Arc<RemoteAdapter>,
    principal: Principal,
    principal_id: String,
    space_id: String,
}

impl Fixture {
    /// Open a fresh durable fixture, provision the operator's admin grant for
    /// the dialing peer and serve it over one signed loopback session.
    async fn start(host_budget_ms: Option<u64>, client_budget_ms: Option<u64>) -> Self {
        let home = tempfile::tempdir().expect("temporary fixture home");
        let db_path = home.path().join("fixture.sqlite");
        let provider = FixtureMemoryProvider::open(&db_path)
            .await
            .expect("open the fixture store");
        let space_id = provider.identity().space_id.clone();
        let principal_id = format!("prn_{}", peer_id(&CLIENT_SEED));
        provider
            .provision_operator_admin(&principal_id)
            .await
            .expect("operator provisions the first admin");
        let directory = Arc::new(
            PeerDirectory::new().with_peer(peer_id(&CLIENT_SEED), principal_id.clone()),
        );
        let provider = Arc::new(provider);
        let (responder, pair) =
            start_host(&provider, &directory, &[CLIENT_SEED], host_budget_ms).await;
        let client = dial(pair, CLIENT_SEED, client_budget_ms).await;
        Self {
            _home: home,
            db_path,
            directory,
            provider,
            responder,
            client,
            principal: principal(&principal_id),
            principal_id,
            space_id,
        }
    }

    /// Reconnect to the same durable file with a fresh responder and client.
    async fn reconnect(&mut self) {
        let (responder, pair) = start_host(
            &self.provider,
            &self.directory,
            &[CLIENT_SEED],
            None,
        )
        .await;
        self.client.close();
        self.responder.close();
        self.responder = responder;
        self.client = dial(pair, CLIENT_SEED, None).await;
    }

    async fn shutdown(self) {
        self.client.close();
        self.responder.close();
    }

    /// Ensure the subject every capture/record op places into.
    async fn ensure_subject(&self) {
        let ensured = wire(&expect_ok(
            self.client
                .ensure_subject(
                    &self.principal,
                    from_value(json!({
                        "space_id": self.space_id,
                        "subject_id": SUBJECT,
                    }))
                    .expect("subjects.ensure request"),
                )
                .await,
        ));
        assert_eq!(ensured["result"]["created"], json!(true));
    }

    /// The signed client's receipt view of one mutation answer.
    async fn capture(&self, request: Value) -> Value {
        let request: MemoryCaptureRequest = serde_json::from_value(request).expect("capture request");
        let response = expect_ok(self.client.capture(&self.principal, request).await);
        wire(&response)
    }

    fn capture_request(&self, key: &str, text: &str, context: &str) -> Value {
        json!({
            "space_id": self.space_id,
            "subject_id": SUBJECT,
            "content": { "text": text },
            "capture_scope": { "visibility": "context", "context_id": context },
            "source_context_ids": [context],
            "kind_hint": "fact",
            "idempotency_key": key,
        })
    }

    fn record_create_request(&self, key: &str, text: &str, shared: bool) -> Value {
        json!({
            "space_id": self.space_id,
            "subject_id": SUBJECT,
            "kind": "fact",
            "content": { "text": text },
            "scope": if shared { json!({"visibility": "shared"}) } else { json!({"visibility": "context", "context_id": CONTEXT}) },
            "idempotency_key": key,
        })
    }

    /// Count rows in one fixture table through a read-only side connection, so
    /// the assertions are about durable state, not about the answer alone.
    async fn count(&self, table: &str) -> u64 {
        let options = SqliteConnectOptions::new()
            .filename(&self.db_path)
            .read_only(true);
        let pool = SqlitePool::connect_with(options)
            .await
            .expect("read-only fixture connection");
        let query = match table {
            "candidates" => "SELECT COUNT(*) AS total FROM candidates",
            "records" => "SELECT COUNT(*) AS total FROM records",
            "receipts" => "SELECT COUNT(*) AS total FROM receipts",
            other => panic!("unknown fixture table {other}"),
        };
        let row = sqlx::query(query)
            .fetch_one(&pool)
            .await
            .expect("count rows");
        let total: i64 = row.try_get("total").expect("count column");
        pool.close().await;
        u64::try_from(total).unwrap_or(0)
    }

    /// Whether any receipt-side row carries `needle` verbatim.
    async fn receipt_metadata_contains(&self, needle: &str) -> bool {
        let options = SqliteConnectOptions::new()
            .filename(&self.db_path)
            .read_only(true);
        let pool = SqlitePool::connect_with(options)
            .await
            .expect("read-only fixture connection");
        let pattern = format!("%{needle}%");
        let mut found = false;
        for query in [
            "SELECT COUNT(*) AS hits FROM receipts WHERE COALESCE(result_json,'') LIKE ? OR COALESCE(error_json,'') LIKE ?",
            "SELECT COUNT(*) AS hits FROM receipt_targets WHERE subject_id LIKE ? OR context_ids_json LIKE ?",
            "SELECT COUNT(*) AS hits FROM idempotency_namespace WHERE payload_digest LIKE ? OR idempotency_key LIKE ?",
        ] {
            let row = sqlx::query(query)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_one(&pool)
                .await
                .expect("scan receipt metadata");
            let hits: i64 = row.try_get("hits").expect("hits column");
            found |= hits > 0;
        }
        pool.close().await;
        found
    }
}

const SUBJECT: &str = "sub_arc_warden";
const CONTEXT: &str = "ctx_harbor_run_7";
const OTHER_CONTEXT: &str = "ctx_market_run_2";

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

fn expect_ok<T>(result: SpokeResult<T>) -> T {
    match result {
        SpokeResult::Ok(value) => value,
        SpokeResult::Reject(reject) => panic!("expected success, got {reject:?}"),
    }
}

fn expect_reject<T: std::fmt::Debug>(result: SpokeResult<T>) -> SpokeReject {
    match result {
        SpokeResult::Ok(value) => panic!("expected a refusal, got {value:?}"),
        SpokeResult::Reject(reject) => reject,
    }
}

fn wire<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("protocol object serializes")
}

fn retry_class(reject: &SpokeReject) -> Option<String> {
    reject
        .details
        .as_ref()
        .and_then(|details| details.get("retry_class"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn digest_of(value: &Value) -> String {
    match jcs_sha256_hex(value) {
        SpokeResult::Ok(digest) => digest,
        SpokeResult::Reject(reject) => panic!("payload digest: {reject:?}"),
    }
}

// ---------------------------------------------------------------------------
// 1. Provider self-description and the unserved families
// ---------------------------------------------------------------------------

#[tokio::test]
async fn describe_and_unavailable_families_are_honest_over_the_signed_session() {
    let fixture = Fixture::start(None, None).await;
    let described = wire(&expect_ok(
        fixture
            .client
            .describe(&fixture.principal, from_value(json!({})).expect("describe request"))
            .await,
    ));
    assert_eq!(described["capabilities"], json!(["spoke-memory"]));
    assert_eq!(
        described["engine_policy_version"],
        json!("nexus-fixture-spoke-memory/0.15.1")
    );
    assert_ne!(
        described["engine_policy_version"],
        json!("nexus-local-memory-v1"),
        "the fixture must not claim the local provider's policy identity"
    );
    assert_eq!(described["memory_protocol_version"], json!(1));
    assert_eq!(described["limits"]["max_page_size"], json!(100));
    assert_eq!(described["limits"]["max_content_bytes"], json!(65_536));
    assert_eq!(described["limits"]["max_candidates_per_capture"], json!(1));
    assert_eq!(
        described["limits"]["dedup_retention_seconds"],
        json!(5_184_000)
    );
    assert_eq!(
        described["limits"]["receipt_retention_seconds"],
        json!(2_592_000)
    );
    assert_eq!(described["required_profile_kinds"], json!([]));

    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 2. Every base operation over one signed session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn all_twenty_five_base_operations_reach_the_durable_fixture() {
    let fixture = Fixture::start(None, None).await;
    let client = Arc::clone(&fixture.client);
    let principal = fixture.principal.clone();
    let space = fixture.space_id.clone();

    // 1. memory.describe
    let described = wire(&expect_ok(
        client
            .describe(&principal, from_value(json!({})).expect("describe request"))
            .await,
    ));
    assert_eq!(described["capabilities"], json!(["spoke-memory"]));

    // 2. memory.spaces.list
    let listed = wire(&expect_ok(
        client
            .list_spaces(
                &principal,
                from_value(json!({})).expect("spaces.list request"),
            )
            .await,
    ));
    assert_eq!(listed["spaces"].as_array().expect("spaces").len(), 1);
    assert_eq!(listed["spaces"][0]["space_id"], json!(space));
    assert_eq!(listed["spaces"][0]["status"], json!("active"));
    assert_eq!(listed["spaces"][0]["revision"], json!(1));
    assert_eq!(listed["spaces"][0]["pending_receipts"], json!(0));

    // 3. memory.subjects.ensure — the sole keyless mutation.
    let ensured = wire(&expect_ok(
        client
            .ensure_subject(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "subject_id": SUBJECT,
                    "kind": { "ns": "toy_world", "id": "warden" },
                }))
                .expect("ensure request"),
            )
            .await,
    ));
    assert_eq!(ensured["status"], json!("committed"));
    assert_eq!(ensured["result"]["created"], json!(true));
    assert_eq!(ensured["result"]["status"], json!("active"));
    assert_eq!(ensured["result"]["revision"], json!(1));
    assert_eq!(ensured["result"]["subject_id"], json!(SUBJECT));

    // 4. memory.subjects.get
    let subject = wire(&expect_ok(
        client
            .get_subject(
                &principal,
                from_value(json!({ "space_id": space, "subject_id": SUBJECT }))
                    .expect("subject request"),
            )
            .await,
    ));
    assert_eq!(subject["subject"]["status"], json!("active"));
    assert_eq!(subject["subject"]["revision"], json!(1));

    // 5. memory.grants.list
    let grants = wire(&expect_ok(
        client
            .list_grants(
                &principal,
                from_value(json!({ "space_id": space })).expect("grants.list request"),
            )
            .await,
    ));
    let admin_grant = grants["grants"]
        .as_array()
        .expect("grants")
        .iter()
        .find(|grant| grant["capabilities"] == json!(["admin"]))
        .expect("the operator admin grant is visible")
        .clone();
    assert_eq!(admin_grant["principal_id"], json!(fixture.principal_id));

    // 6. memory.grants.grant — a scoped grant for a different grantee.
    let issued = wire(&expect_ok(
        client
            .grant(
                &principal,
                from_value(json!({
                    "capabilities": ["capture", "review", "records_read", "records_write"],
                    "context_ids": [CONTEXT],
                    "idempotency_key": "grant-1",
                    "principal_id": "prn_grantee",
                    "space_id": space,
                    "subject_id": SUBJECT,
                }))
                .expect("grant request"),
            )
            .await,
    ));
    let granted_id = issued["result"]["grant_id"]
        .as_str()
        .expect("grant id")
        .to_owned();
    assert_eq!(issued["result"]["revision"], json!(1));

    // 7. memory.grants.revoke
    let revoked = wire(&expect_ok(
        client
            .revoke_grant(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "grant_id": granted_id,
                    "expected_revision": 1,
                    "idempotency_key": "revoke-1",
                }))
                .expect("revoke request"),
            )
            .await,
    ));
    assert_eq!(revoked["result"]["revoked"], json!(true));
    assert_eq!(revoked["result"]["revision"], json!(2));

    // 8. memory.capture
    let captured = fixture
        .capture(fixture.capture_request("cap-1", "The gate key is under the third barrel.", CONTEXT))
        .await;
    assert_eq!(captured["status"], json!("committed"));
    assert_eq!(captured["op"], json!("memory.capture"));
    assert_eq!(
        captured["requester_principal_id"],
        json!(fixture.principal_id)
    );
    assert_eq!(captured["payload_digest"], json!(digest_of(&fixture.capture_request("cap-1", "The gate key is under the third barrel.", CONTEXT))));
    let candidate_id = captured["result"]["candidate_ids"][0]
        .as_str()
        .expect("first candidate id")
        .to_owned();
    let capture_receipt_id = captured["receipt_id"].as_str().expect("receipt id").to_owned();

    // 9. memory.candidates.list
    let candidates_page = wire(&expect_ok(
        client
            .list_candidates(
                &principal,
                from_value(json!({ "space_id": space, "subject_id": SUBJECT }))
                    .expect("candidates.list request"),
            )
            .await,
    ));
    let pending = candidates_page["candidates"]
        .as_array()
        .expect("candidates")
        .clone();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["candidate_id"], json!(candidate_id));
    assert_eq!(pending[0]["status"], json!("pending"));
    assert_eq!(
        pending[0]["capture_scope"],
        json!({ "visibility": "context", "context_id": CONTEXT })
    );

    // 10. memory.candidates.get
    let candidate = wire(&expect_ok(
        client
            .get_candidate(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "candidate_id": candidate_id,
                }))
                .expect("candidates.get request"),
            )
            .await,
    ));
    assert_eq!(candidate["candidate"]["proposed"]["kind"], json!("fact"));

    // 11. memory.candidates.review
    let reviewed = wire(&expect_ok(
        client
            .review_candidate(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "candidate_id": candidate_id,
                    "expected_revision": 1,
                    "decision": { "action": "promote" },
                    "idempotency_key": "review-1",
                }))
                .expect("review request"),
            )
            .await,
    ));
    assert_eq!(reviewed["result"]["outcome"], json!("promoted"));
    let record_ids = reviewed["result"]["record_ids"]
        .as_array()
        .expect("record ids")
        .clone();
    assert_eq!(record_ids.len(), 1);
    let promoted_record = record_ids[0].as_str().expect("record id").to_owned();

    // 12. memory.records.get
    let record = wire(&expect_ok(
        client
            .get_record(
                &principal,
                from_value(json!({ "space_id": space, "record_id": promoted_record }))
                    .expect("records.get request"),
            )
            .await,
    ));
    assert_eq!(record["record"]["revision"], json!(1));
    assert_eq!(
        record["record"]["scope"],
        json!({ "visibility": "context", "context_id": CONTEXT })
    );
    assert!(record["record"]["content"]["text"]
        .as_str()
        .expect("record text")
        .contains("third barrel"));

    // 13. memory.records.update
    let updated = wire(&expect_ok(
        client
            .update_record(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "record_id": promoted_record,
                    "expected_revision": 1,
                    "content": { "text": "The gate key moved under the fourth barrel." },
                    "idempotency_key": "record-update-1",
                }))
                .expect("records.update request"),
            )
            .await,
    ));
    assert_eq!(updated["result"]["revision"], json!(2));

    // A stale CAS precondition is a terminal refusal with no effect.
    let stale = expect_reject(
        client
            .update_record(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "record_id": promoted_record,
                    "expected_revision": 1,
                    "content": { "text": "stale overwrite" },
                    "idempotency_key": "record-update-stale",
                }))
                .expect("stale update request"),
            )
            .await,
    );
    assert_eq!(stale.code, SpokeRejectCode::RevisionConflict);
    assert_eq!(retry_class(&stale).as_deref(), Some("terminal"));

    // 14. memory.records.create
    let created = wire(&expect_ok(
        client
            .create_record(
                &principal,
                from_value(fixture.record_create_request("record-create-1", "Shared field note.", true))
                    .expect("records.create request"),
            )
            .await,
    ));
    let shared_record = created["result"]["record_id"]
        .as_str()
        .expect("shared record id")
        .to_owned();
    assert_eq!(created["result"]["revision"], json!(1));

    // 15. memory.records.list
    let records_page = wire(&expect_ok(
        client
            .list_records(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                }))
                .expect("records.list request"),
            )
            .await,
    ));
    assert_eq!(records_page["total_authorized"], json!(2));

    // 16. memory.records.promote
    let promoted = wire(&expect_ok(
        client
            .promote_record(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "record_id": promoted_record,
                    "expected_revision": 2,
                    "target_scope": { "visibility": "shared" },
                    "idempotency_key": "record-promote-1",
                }))
                .expect("records.promote request"),
            )
            .await,
    ));
    assert_eq!(promoted["result"]["revision"], json!(3));
    assert_eq!(
        promoted["result"]["scope"],
        json!({ "visibility": "shared" })
    );

    // 17. memory.records.delete
    let deleted = wire(&expect_ok(
        client
            .delete_record(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "record_id": shared_record,
                    "expected_revision": 1,
                    "reason": "superseded",
                    "idempotency_key": "record-delete-1",
                }))
                .expect("records.delete request"),
            )
            .await,
    ));
    assert_eq!(deleted["result"]["tombstoned"], json!(true));

    // 18. memory.candidates.dismiss — a second capture, then consumption
    // without a review decision.
    let second_capture = fixture
        .capture(fixture.capture_request("cap-2", "Market gossip for dismissal.", OTHER_CONTEXT))
        .await;
    let second_candidate = second_capture["result"]["candidate_ids"][0]
        .as_str()
        .expect("second candidate id")
        .to_owned();
    let dismissed = wire(&expect_ok(
        client
            .dismiss_candidate(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "candidate_id": second_candidate,
                    "expected_revision": 1,
                    "idempotency_key": "dismiss-1",
                }))
                .expect("candidates.dismiss request"),
            )
            .await,
    ));
    assert_eq!(dismissed["result"]["outcome"], json!("dismissed"));
    assert_eq!(dismissed["result"]["candidate_id"], json!(second_candidate));

    // 19. memory.recall — the promoted record is now shared.
    let recalled = wire(&expect_ok(
        client
            .recall(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "shared_only" },
                    "query": "gate",
                    "budget": { "max_items": 5 },
                }))
                .expect("recall request"),
            )
            .await,
    ));
    let recalled_texts: Vec<&str> = recalled["records"]
        .as_array()
        .expect("recalled records")
        .iter()
        .filter_map(|record| record["content"]["text"].as_str())
        .collect();
    assert!(
        recalled_texts.iter().any(|text| text.contains("fourth barrel")),
        "recall returns the promoted shared record: {recalled_texts:?}"
    );
    assert_eq!(
        recalled["engine_policy_version"],
        json!("nexus-fixture-spoke-memory/0.15.1")
    );
    assert_eq!(recalled["truncated"], json!(false));

    // 20. memory.profile.update
    let profile = wire(&expect_ok(
        client
            .update_profile(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "subject_id": SUBJECT,
                    "profile_kind": "personality",
                    "expected_revision": null,
                    "document": { "format": "markdown", "body": "# Warden\nCautious and precise." },
                    "idempotency_key": "profile-1",
                }))
                .expect("profile.update request"),
            )
            .await,
    ));
    assert_eq!(profile["result"]["profile_kind"], json!("personality"));
    assert_eq!(profile["result"]["revision"], json!(1));

    // 21. memory.profile.get
    let profiles = wire(&expect_ok(
        client
            .get_profile(
                &principal,
                from_value(json!({ "space_id": space, "subject_id": SUBJECT }))
                    .expect("profile.get request"),
            )
            .await,
    ));
    assert_eq!(profiles["profiles"].as_array().expect("profiles").len(), 1);
    assert_eq!(
        profiles["profiles"][0]["document"]["body"],
        json!("# Warden\nCautious and precise.")
    );

    // 22. memory.receipts.get — the capture receipt stays content-free.
    let receipt = wire(&expect_ok(
        client
            .get_receipt(
                &principal,
                from_value(json!({ "space_id": space, "receipt_id": capture_receipt_id }))
                    .expect("receipts.get request"),
            )
            .await,
    ));
    assert_eq!(receipt["receipt"]["status"], json!("committed"));
    assert_eq!(receipt["receipt"]["op"], json!("memory.capture"));
    assert_eq!(
        receipt["receipt"]["result"]["candidate_ids"][0],
        json!(candidate_id)
    );
    assert!(!receipt.to_string().contains("third barrel"));

    // 23. memory.spaces.freeze — a frozen space refuses fresh writes and the
    // refusal leaves the candidate rows untouched.
    let candidates_before = fixture.count("candidates").await;
    let frozen = wire(&expect_ok(
        client
            .freeze_space(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "expected_revision": 1,
                    "idempotency_key": "freeze-1",
                }))
                .expect("freeze request"),
            )
            .await,
    ));
    assert_eq!(frozen["result"]["status"], json!("frozen"));
    assert_eq!(frozen["result"]["revision"], json!(2));
    let frozen_refusal = expect_reject(
        client
            .capture(
                &principal,
                from_value(fixture.capture_request("cap-frozen", "blocked", CONTEXT))
                    .expect("capture request"),
            )
            .await,
    );
    assert_eq!(frozen_refusal.code, SpokeRejectCode::SpaceFrozen);
    assert_eq!(retry_class(&frozen_refusal).as_deref(), Some("terminal"));
    assert_eq!(fixture.count("candidates").await, candidates_before);

    // 24. memory.spaces.unfreeze
    let unfrozen = wire(&expect_ok(
        client
            .unfreeze_space(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "expected_revision": 2,
                    "idempotency_key": "unfreeze-1",
                }))
                .expect("unfreeze request"),
            )
            .await,
    ));
    assert_eq!(unfrozen["result"]["status"], json!("active"));
    assert_eq!(unfrozen["result"]["revision"], json!(3));

    // 25. memory.subjects.fence — a fenced subject refuses fresh writes.
    let fenced = wire(&expect_ok(
        client
            .fence_subject(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "subject_id": SUBJECT,
                    "target": { "type": "subject" },
                    "action": "fence",
                    "expected_revision": 1,
                    "idempotency_key": "fence-1",
                }))
                .expect("fence request"),
            )
            .await,
    ));
    assert_eq!(fenced["result"]["status"], json!("fenced"));
    assert_eq!(fenced["result"]["revision"], json!(2));
    let fenced_refusal = expect_reject(
        client
            .capture(
                &principal,
                from_value(fixture.capture_request("cap-fenced", "blocked", CONTEXT))
                    .expect("capture request"),
            )
            .await,
    );
    assert_eq!(fenced_refusal.code, SpokeRejectCode::SubjectFenced);
    assert_eq!(retry_class(&fenced_refusal).as_deref(), Some("terminal"));

    // Replay of the first capture returns the original receipt and no second
    // candidate (MEM-R021), even after every later revision.
    let replayed = fixture
        .capture(fixture.capture_request("cap-1", "The gate key is under the third barrel.", CONTEXT))
        .await;
    assert_eq!(replayed["receipt_id"], json!(capture_receipt_id));
    assert_eq!(replayed["payload_digest"], captured["payload_digest"]);
    assert_eq!(fixture.count("candidates").await, candidates_before);

    // The dismissed receipt replays from the same key too.
    let dismissed_replay = wire(&expect_ok(
        client
            .dismiss_candidate(
                &principal,
                from_value(json!({
                    "space_id": space,
                    "candidate_id": second_candidate,
                    "expected_revision": 1,
                    "idempotency_key": "dismiss-1",
                }))
                .expect("dismiss replay request"),
            )
            .await,
    ));
    assert_eq!(dismissed_replay["receipt_id"], dismissed["receipt_id"]);

    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 3. Admission: mapped peers, unmapped peers, forged claims
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admission_is_the_explicit_peer_principal_table_only() {
    let fixture = Fixture::start(None, None).await;
    fixture.ensure_subject().await;

    // An authenticated peer the operator never mapped holds no principal.
    let (unmapped_responder, unmapped_pair) = start_host(
        &fixture.provider,
        &fixture.directory,
        &[UNMAPPED_SEED],
        None,
    )
    .await;
    let unmapped_client = dial(unmapped_pair, UNMAPPED_SEED, None).await;
    let unmapped_refusal = expect_reject(
        unmapped_client
            .describe(
                &principal(&format!("prn_{}", peer_id(&UNMAPPED_SEED))),
                from_value(json!({})).expect("describe request"),
            )
            .await,
    );
    assert_eq!(unmapped_refusal.code, SpokeRejectCode::NotAuthorized);
    assert_eq!(retry_class(&unmapped_refusal).as_deref(), Some("terminal"));
    unmapped_client.close();
    unmapped_responder.close();

    // A peer outside the transport allowlist never gets a session at all.
    let (deny_responder, deny_pair) =
        start_host(&fixture.provider, &fixture.directory, &[], None).await;
    let unknown = connect_remote_adapter(RemoteAdapterOptions {
        transport: Arc::new(deny_pair.client),
        local_identity: RemoteIdentity { seed: [0x66; 32] },
        local_manifest: manifest("fixture-conformance-client", &MEMORY_CAPABILITIES),
        remote_pubkey: pubkey(&HOST_SEED),
        allowlist: vec![peer_id(&HOST_SEED)],
        invoke_timeout_ms: Some(500),
        capability_token: None,
    })
    .await;
    assert!(
        unknown.is_err(),
        "an unallowlisted dialer cannot establish a signed session"
    );
    deny_responder.close();

    // The mapped peer's own writes always carry its authenticated identity.
    let receipt = fixture
        .capture(fixture.capture_request("identity-1", "identity proof", CONTEXT))
        .await;
    assert_eq!(
        receipt["requester_principal_id"],
        json!(fixture.principal_id)
    );

    // A grant issued to another principal is never attributed to the caller.
    let issued = wire(&expect_ok(
        fixture
            .client
            .grant(
                &fixture.principal,
                from_value(json!({
                    "capabilities": ["capture"],
                    "context_ids": [CONTEXT],
                    "idempotency_key": "grant-other",
                    "principal_id": "prn_grantee",
                    "space_id": fixture.space_id,
                    "subject_id": SUBJECT,
                }))
                .expect("grant request"),
            )
            .await,
    ));
    assert_eq!(
        issued["requester_principal_id"],
        json!(fixture.principal_id)
    );
    let filtered = wire(&expect_ok(
        fixture
            .client
            .list_grants(
                &fixture.principal,
                from_value(json!({
                    "space_id": fixture.space_id,
                    "principal_id": "prn_grantee",
                }))
                .expect("grants.list request"),
            )
            .await,
    ));
    for grant in filtered["grants"].as_array().expect("grants") {
        assert_eq!(grant["principal_id"], json!("prn_grantee"));
    }

    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 4. D15: the exact received-object digest over the signed wire
// ---------------------------------------------------------------------------

fn next_nonce() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Sign one post-hello invoke envelope over its JCS signed object
/// (`spoke-connect-invoke-request-jcs-v1`), the same form `RemoteAdapter` emits.
fn signed_invoke(
    seed: &[u8; 32],
    session_id: &str,
    sequence: i64,
    request_id: &str,
    op: &str,
    payload: &Value,
) -> Value {
    let signed = json!({
        "session_id": session_id,
        "sequence": sequence,
        "request_id": request_id,
        "op": op,
        "payload": payload,
    });
    let bytes = serde_jcs::to_vec(&signed).expect("signed object canonicalizes");
    let signature = SigningKey::from_bytes(seed).sign(&bytes);
    let mut envelope = signed.as_object().expect("signed object").clone();
    envelope.insert("extensions".into(), json!({}));
    envelope.insert(
        "signature".into(),
        json!(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes())),
    );
    Value::Object(envelope)
}

async fn raw_session(pair: LoopbackTransportPair) -> (LoopbackTransport, String) {
    let client = pair.client;
    let hello = sign_hello_ed25519(
        &CLIENT_SEED,
        &format!("fixture-raw-nonce-{:x}", next_nonce()),
        &connect_manifest(&manifest("fixture-conformance-client", &MEMORY_CAPABILITIES)),
        None,
    )
    .expect("sign the initiator hello");
    client
        .send(&serde_json::to_vec(&hello).expect("hello bytes"))
        .await
        .expect("hello send");
    let _responder_hello: ConnectHello = serde_json::from_slice(
        &client
            .recv()
            .await
            .expect("responder hello"),
    )
    .expect("decode the responder hello");
    let session: ConnectSession = serde_json::from_slice(
        &client
            .recv()
            .await
            .expect("session snapshot"),
    )
    .expect("decode the session snapshot");
    (client, session.session_id.to_string())
}

async fn raw_invoke(
    client: &LoopbackTransport,
    session_id: &str,
    sequence: i64,
    request_id: &str,
    op: &str,
    payload: &Value,
) -> Value {
    let request = signed_invoke(&CLIENT_SEED, session_id, sequence, request_id, op, payload);
    client
        .send(&serde_json::to_vec(&request).expect("request bytes"))
        .await
        .expect("request send");
    serde_json::from_slice(&client.recv().await.expect("response recv"))
        .expect("decode the invoke response")
}

#[tokio::test]
async fn raw_empty_and_omitted_source_context_ids_conflict_at_the_signed_boundary() {
    let fixture = Fixture::start(None, None).await;
    fixture.ensure_subject().await;
    // A second signed session on the same provider, driven raw so the exact
    // wire object is under this test's control.
    let (_responder, pair) =
        start_host(&fixture.provider, &fixture.directory, &[CLIENT_SEED], None).await;
    let (transport, session_id) = raw_session(pair).await;

    let mut explicit = fixture.capture_request("d15-raw", "D15 raw digest proof", CONTEXT);
    explicit["source_context_ids"] = json!([]);
    let mut omitted = explicit.clone();
    omitted
        .as_object_mut()
        .expect("payload object")
        .remove("source_context_ids");

    // The two wire objects are the same typed request: serde defaults the
    // omitted member, so only the raw object distinguishes them.
    let typed_explicit: MemoryCaptureRequest =
        from_value(explicit.clone()).expect("typed explicit request");
    let typed_omitted: MemoryCaptureRequest =
        from_value(omitted.clone()).expect("typed omitted request");
    assert_eq!(
        wire(&typed_explicit),
        wire(&typed_omitted),
        "the typed views of both wire objects are identical"
    );
    assert_ne!(digest_of(&explicit), digest_of(&omitted));

    let receipts_before = fixture.count("receipts").await;
    let first = raw_invoke(&transport, &session_id, 0, "d15-raw-1", "memory.capture", &explicit).await;
    let receipt = first.get("payload").expect("committed receipt payload");
    assert_eq!(receipt["status"], json!("committed"));
    assert_eq!(
        receipt["payload_digest"],
        json!(digest_of(&explicit)),
        "the receipt binds the digest of the exact received object"
    );

    let second = raw_invoke(&transport, &session_id, 1, "d15-raw-2", "memory.capture", &omitted).await;
    let error = second.get("error").expect("conflict envelope");
    assert_eq!(error["code"], json!("IDEMPOTENCY_CONFLICT"));
    assert_eq!(error["details"]["retry_class"], json!("terminal"));
    assert_eq!(fixture.count("candidates").await, 1);
    assert_eq!(fixture.count("receipts").await, receipts_before + 1);
}

// ---------------------------------------------------------------------------
// 5. Concurrency: overlapping invokes keep their own digests and effects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn overlapping_invokes_keep_their_own_digests_and_operations_isolated() {
    let fixture = Fixture::start(None, None).await;
    fixture.ensure_subject().await;
    let first_payload = fixture.capture_request("overlap-a", "alpha material", CONTEXT);
    let second_payload = fixture.capture_request("overlap-b", "beta material", OTHER_CONTEXT);
    let record_payload = fixture.record_create_request("overlap-c", "authored alongside", true);

    let (first, second, record) = tokio::join!(
        fixture.client.capture(
            &fixture.principal,
            from_value(first_payload.clone()).expect("first capture")
        ),
        fixture.client.capture(
            &fixture.principal,
            from_value(second_payload.clone()).expect("second capture")
        ),
        fixture.client.create_record(
            &fixture.principal,
            from_value(record_payload.clone()).expect("record create")
        ),
    );
    let first = wire(&expect_ok(first));
    let second = wire(&expect_ok(second));
    let record = wire(&expect_ok(record));

    assert_eq!(first["payload_digest"], json!(digest_of(&first_payload)));
    assert_eq!(second["payload_digest"], json!(digest_of(&second_payload)));
    assert_eq!(record["payload_digest"], json!(digest_of(&record_payload)));
    assert_eq!(first["op"], json!("memory.capture"));
    assert_eq!(record["op"], json!("memory.records.create"));
    assert_ne!(first["receipt_id"], second["receipt_id"]);
    assert_ne!(
        first["result"]["candidate_ids"], second["result"]["candidate_ids"],
        "each capture produced its own candidate"
    );
    assert_eq!(fixture.count("receipts").await, 4);
    assert_eq!(fixture.count("candidates").await, 2);
    assert_eq!(fixture.count("records").await, 1);
    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 6. Detached completion keeps the request digest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detached_completion_preserves_the_request_digest() {
    // The responder answers the uncertain envelope at 150 ms while the dialer
    // keeps waiting long enough to receive it.
    let fixture = Fixture::start(Some(150), Some(5_000)).await;
    fixture.ensure_subject().await;
    let payload = fixture.capture_request("detached-1", "detached digest proof", CONTEXT);
    let request: MemoryCaptureRequest = from_value(payload.clone()).expect("capture request");

    let mut hold = fixture
        .provider
        .faults()
        .hold_next(FaultPoint::ResponseDrop);
    let client = Arc::clone(&fixture.client);
    let principal = fixture.principal.clone();
    let served = tokio::spawn(async move { client.capture(&principal, request).await });

    hold.wait_reached()
        .await
        .expect("the provider reached the response-drop point");
    let reject = expect_reject(served.await.expect("detached invoke joined"));
    assert_eq!(reject.code, SpokeRejectCode::OutcomeUncertain);
    assert_eq!(retry_class(&reject).as_deref(), Some("uncertain"));

    // The effect is already durable and content-free while the response is held.
    assert_eq!(fixture.count("candidates").await, 1);
    assert_eq!(fixture.count("receipts").await, 2);

    hold.release();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The replay resolves the uncertainty and carries the same exact digest.
    let replay: MemoryCaptureRequest = from_value(payload.clone()).expect("capture request");
    let receipt = wire(&expect_ok(
        fixture.client.capture(&fixture.principal, replay).await,
    ));
    assert_eq!(receipt["status"], json!("committed"));
    assert_eq!(receipt["payload_digest"], json!(digest_of(&payload)));
    assert_eq!(fixture.count("candidates").await, 1);
    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 7. A missing request scope fails closed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn missing_request_scope_fails_closed_without_effects() {
    let fixture = Fixture::start(None, None).await;
    let request: MemoryCaptureRequest =
        from_value(fixture.capture_request("scope-1", "scope proof", CONTEXT))
            .expect("capture request");

    // A direct in-process port call has no serve scope, so the typed method
    // refuses before it can touch storage.
    let reject = expect_reject(
        MemoryPort::capture(fixture.provider.as_ref(), &fixture.principal, request).await,
    );
    assert_eq!(reject.code, SpokeRejectCode::InternalError);
    assert_eq!(
        reject
            .details
            .as_ref()
            .and_then(|details| details.get("kind")),
        Some(&json!("request_scope_missing"))
    );
    assert_eq!(fixture.count("candidates").await, 0);
    assert_eq!(fixture.count("receipts").await, 0);
    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 8. §14.2 retry classes and preserved details at the signed boundary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_classes_and_details_survive_the_signed_boundary() {
    let fixture = Fixture::start(None, None).await;
    fixture.ensure_subject().await;

    // Terminal: a space this provider does not serve.
    let mut wrong_space = fixture.capture_request("retry-space", "bad space", CONTEXT);
    wrong_space["space_id"] = json!("spc_not_served");
    let refusal = expect_reject(
        fixture
            .client
            .capture(&fixture.principal, from_value(wrong_space).expect("capture"))
            .await,
    );
    assert_eq!(refusal.code, SpokeRejectCode::NotFound);
    assert_eq!(retry_class(&refusal).as_deref(), Some("terminal"));

    // Retryable internal, proven to have committed nothing: the pre-commit hold
    // expires before the transaction commits.
    let receipts_before = fixture.count("receipts").await;
    let payload = fixture.capture_request("retry-precommit", "precommit proof", CONTEXT);
    let hold = fixture
        .provider
        .faults()
        .hold_next_bounded(FaultPoint::BeforeCommit, Duration::from_millis(40));
    let refusal = expect_reject(
        fixture
            .client
            .capture(
                &fixture.principal,
                from_value(payload.clone()).expect("capture request"),
            )
            .await,
    );
    assert_eq!(refusal.code, SpokeRejectCode::InternalError);
    assert_eq!(retry_class(&refusal).as_deref(), Some("retryable"));
    drop(hold);
    assert_eq!(fixture.count("candidates").await, 0);
    assert_eq!(fixture.count("receipts").await, receipts_before);

    // The key is free again: the same request now commits for real.
    let committed = wire(&expect_ok(
        fixture
            .client
            .capture(
                &fixture.principal,
                from_value(payload).expect("capture request"),
            )
            .await,
    ));
    assert_eq!(committed["status"], json!("committed"));

    // Uncertain after commit: the effect is durable, the answer says so, and
    // the original receipt is recoverable.
    let post_payload = fixture.capture_request("retry-postcommit", "postcommit proof", CONTEXT);
    let candidates_after_postcommit = {
        fixture.provider.faults().fail_next(FaultPoint::AfterCommitBeforeResponse);
        let refusal = expect_reject(
            fixture
                .client
                .capture(
                    &fixture.principal,
                    from_value(post_payload.clone()).expect("capture request"),
                )
                .await,
        );
        assert_eq!(refusal.code, SpokeRejectCode::OutcomeUncertain);
        assert_eq!(retry_class(&refusal).as_deref(), Some("uncertain"));
        fixture.count("candidates").await
    };
    assert_eq!(
        candidates_after_postcommit, 2,
        "the post-commit capture left its candidate behind"
    );
    let recovered = wire(&expect_ok(
        fixture
            .client
            .capture(
                &fixture.principal,
                from_value(post_payload.clone()).expect("capture request"),
            )
            .await,
    ));
    assert_eq!(recovered["status"], json!("committed"));
    assert_eq!(recovered["payload_digest"], json!(digest_of(&post_payload)));
    assert_eq!(fixture.count("candidates").await, 2);
    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 9. Receipts and dedup rows never carry the payload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn receipts_and_dedup_rows_never_store_the_payload() {
    let fixture = Fixture::start(None, None).await;
    fixture.ensure_subject().await;
    let secret = "the vault code is seven-red-heron";
    let payload = fixture.capture_request("privacy-1", secret, CONTEXT);
    let receipt = fixture.capture(payload).await;

    assert_eq!(receipt["status"], json!("committed"));
    assert!(
        !receipt.to_string().contains(secret),
        "the receipt answer is content-free"
    );
    assert!(
        !fixture.receipt_metadata_contains(secret).await,
        "receipt, target and dedup rows never carry the request text"
    );
    // The candidate legitimately keeps the captured material; that separation
    // is what makes the receipt content-free.
    assert_eq!(fixture.count("candidates").await, 1);
    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 10. Reconnect and restart keep identity, authority and receipts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reconnect_keeps_identity_authority_and_receipts() {
    let mut fixture = Fixture::start(None, None).await;
    fixture.ensure_subject().await;
    let space_before = fixture.space_id.clone();
    let authority_before = fixture.provider.identity().authority_host_id.clone();
    let generation_before = fixture.provider.identity().authority_generation;
    let payload = fixture.capture_request("restart-1", "survives reconnect", CONTEXT);
    let first = fixture.capture(payload.clone()).await;

    fixture.reconnect().await;

    assert_eq!(fixture.provider.identity().space_id, space_before);
    assert_eq!(
        fixture.provider.identity().authority_host_id,
        authority_before
    );
    assert_eq!(
        fixture.provider.identity().authority_generation,
        generation_before
    );

    let receipt = wire(&expect_ok(
        fixture
            .client
            .get_receipt(
                &fixture.principal,
                from_value(json!({
                    "space_id": space_before,
                    "receipt_id": first["receipt_id"],
                }))
                .expect("receipts.get request"),
            )
            .await,
    ));
    assert_eq!(receipt["receipt"]["receipt_id"], first["receipt_id"]);

    // The dedup namespace follows the stable authenticated principal, so the
    // retry after a reconnect replays instead of re-executing.
    let replay = fixture.capture(payload).await;
    assert_eq!(replay["receipt_id"], first["receipt_id"]);
    assert_eq!(replay["payload_digest"], first["payload_digest"]);
    assert_eq!(fixture.count("candidates").await, 1);

    fixture.shutdown().await;
}

// ---------------------------------------------------------------------------
// 11. The raw signed wire: dispatch denials, unknown ops, forged claims
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unavailable_families_forged_claims_and_unknown_ops_are_refused_on_the_wire() {
    let fixture = Fixture::start(None, None).await;
    fixture.ensure_subject().await;
    let (_responder, pair) =
        start_host(&fixture.provider, &fixture.directory, &[CLIENT_SEED], None).await;
    let (transport, session_id) = raw_session(pair).await;

    // The optional families are not negotiated, so the dispatch gate refuses
    // them before any payload is read.
    let mut sequence = 0_i64;
    for op in [
        "memory.reflections.state",
        "memory.reflections.list",
        "memory.reflections.generate",
        "memory.exports.create",
        "memory.exports.get",
        "memory.imports.stage",
        "memory.imports.activate",
        "memory.imports.discard",
    ] {
        let response = raw_invoke(
            &transport,
            &session_id,
            sequence,
            &format!("wire-deny-{sequence}"),
            op,
            &json!({}),
        )
        .await;
        sequence += 1;
        let error = response.get("error").expect("dispatch-deny envelope");
        let code = error["code"].as_str().expect("wire code");
        assert!(
            code == "capability_missing" || code == "op_unsupported",
            "{op} answered {code}"
        );
        assert_eq!(error["details"]["retry_class"], json!("terminal"), "{op}");
    }

    // An op with no dispatch row at all stays op_unsupported.
    let unknown = raw_invoke(
        &transport,
        &session_id,
        sequence,
        "wire-unknown",
        "memory.not_an_op",
        &json!({}),
    )
    .await;
    assert_eq!(
        unknown["error"]["code"],
        json!("op_unsupported"),
        "an unknown memory op is not silently served"
    );

    // A payload that claims a principal is not a request field: the service's
    // typed decode refuses it before the provider runs.
    sequence += 1;
    let mut forged = fixture.capture_request("forged-wire", "forged requester", CONTEXT);
    forged["principal_id"] = json!("prn_someone_else");
    let forged_response = raw_invoke(
        &transport,
        &session_id,
        sequence,
        "wire-forged",
        "memory.capture",
        &forged,
    )
    .await;
    let error = forged_response
        .get("error")
        .expect("invalid-input envelope");
    assert_eq!(error["code"], json!("INVALID_INPUT"));
    assert_eq!(error["details"]["retry_class"], json!("terminal"));
    assert_eq!(fixture.count("candidates").await, 0);
}
