//! R2 (v1.229 N3 Task 2): remote subjects, capture, candidates, review and
//! dismiss — driven through the real public `CoreService`, not the port.
//!
//! Every scenario opens Core with the selected remote provider, so the whole
//! path under test is the product one: the pinned `MemoryProviderBinding`, the
//! SDK `RemoteAdapter` over a signed Connect session, and the Nexus-side
//! admission/refusal policy. The provider on the other end is the independent,
//! durable `FixtureMemoryProvider` sharing nothing with Core's local provider,
//! served by the SDK's `connect_responder` over a real `127.0.0.1` socket with
//! the one product WS transport on both ends. Nothing is mocked: the credential
//! file, the signed hello, the operator-provisioned grant, the fixture's own
//! space/authority and its durable candidates/receipts are the real ones.
//!
//! What these tests prove, and what they intentionally cannot:
//!
//! - Subject ensure, capture, candidate list/get, review and dismiss all run on
//!   the remote provider: the effects are asserted in the fixture's own
//!   database, and the local legacy queue, the local provider's tables and its
//!   file root stay untouched.
//! - The D18 boundary is observed where it is authoritative: the Nexus-side
//!   `remote_binding_memory_admissions`/`_objects` rows (content-free
//!   bookkeeping) and the real public `remove_binding` gate. An unknown wire
//!   attempt keeps the binding unremovable across a real close/reopen and is
//!   never cleared by a later same-key success, by a published receipt, or by a
//!   disposal acknowledgement; a proven no-effect refusal and a complete
//!   known-object disposal do permit the authoritative removal.
//! - Refusals reach the caller as the static classified `MemoryProviderRefused`
//!   carrier: the provider's own message text and any request content never
//!   travel with it.
//!
//! `MemoryProviderBinding` is crate-private, so this crate-external test cannot
//! read the pinned `(space, authority, client identity)` tuple back. The
//! namespace columns the admission/object rows carry are the observable
//! substitute: they are written from that pinned tuple by the product code.
//!
//! The adapter's `get_subject` read (`MemoryAdapter::subject_archived`) has no
//! caller on this task's public surface — its only consumer is the Character
//! mind projection (`soul.rs::family`), reachable from the Host composition or
//! the `test-hooks` seam, both of which this suite must not use. What is proved
//! here instead is the subject's durable identity: the adapters' `ensure_subject`
//! created the exact `(kind_ns, kind_id)` row the provider's `subjects.get`
//! addresses, and every candidate/record read and write below is subject-scoped.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;
use tokio::net::TcpListener;

use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::daemon_api::memory::create_pending_review_request::CreatePendingReviewRequest;
use nexus_contracts::generated::core::CoreCharacterTransitionRequest;
use nexus_contracts::{CoreRegisterCreatorRequest, SetActiveWorkspaceRequest};
use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::memory_provider::legacy;
use nexus_core::{
    CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreResult, CoreService,
    MemoryProviderRefusal, MemoryProviderSelection, Principal, RemoteMemoryProviderSelection,
};
use nexus_memory_conformance::{FixtureMemoryProvider, FixtureFaults, PeerDirectory};
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_responder, ConnectResponder, ConnectResponderOptions, RemoteIdentity,
    RemoteMemoryService, RemoteServePorts, RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, Relation, Rule, Scope, TimelineEvent,
};
use spoke_operations::{
    FindingPort, HostManifestPort, KnowledgeEntryPort, RelationPort, RuleQueryPort, ScopeQueryPort,
    SpokeReject, SpokeRejectCode, SpokeResult,
};

/// The host's Connect identity (the provider side of the signed session).
const HOST_SEED: [u8; 32] = [0x42; 32];
/// The dialing client's Connect identity; the credential file holds this seed.
const CLIENT_SEED: [u8; 32] = [0x31; 32];
/// The one capability both ends negotiate: the fixture serves no other family.
const MEMORY_CAPABILITIES: [&str; 1] = ["spoke-memory"];
const WORKSPACE: &str = "default";
const CREDENTIAL_ID: &str = "cred-r2-subjects";
const BOUND: Duration = Duration::from_secs(5);
/// A high-signal digest the deterministic classifier promotes for `brainstorm`.
const PROMOTE_DIGEST: &str =
    "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";
/// The fixture's own `max_content_bytes` is 65536 and it measures the whole
/// serialized `content`; a digest at Core's own 64 KiB field bound passes client
/// validation and still exceeds the provider's payload bound.
const OVER_LIMIT_DIGEST_BYTES: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// A memory-only responder host
// ---------------------------------------------------------------------------

/// The baseline half of the responder's ports face: `spoke-baseline` is never
/// negotiated here, so the dispatch gate answers every `port.*` op before it
/// could reach this type. It exists because `RemoteServePorts` is a
/// baseline-port supertrait, and it refuses honestly.
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
// Signed loopback provider
// ---------------------------------------------------------------------------

fn manifest(host_id: &str) -> HostCapabilityManifest {
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["data-store"],
        "capabilities": MEMORY_CAPABILITIES,
        "namespaces": [],
        "extensions": {},
    }))
    .expect("valid manifest")
}

fn pubkey(seed: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

fn peer_id(seed: &[u8; 32]) -> String {
    derive_peer_id_from_ed25519_pubkey(&pubkey(seed))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Bind a real `127.0.0.1:0` listener whose first accepted WebSocket becomes
/// the signed responder's transport.
async fn start_listener(
    memory_service: Arc<dyn RemoteMemoryService>,
    host_manifest: HostCapabilityManifest,
    client_peer: String,
    client_pubkey: [u8; 32],
) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let task = tokio::spawn(async move {
        let (tcp, peer) = tokio::time::timeout(BOUND, listener.accept())
            .await
            .expect("the client connects to the listener")
            .expect("accept the localhost peer");
        assert!(peer.ip().is_loopback(), "the fixture serves localhost only");
        let ws = tokio::time::timeout(
            BOUND,
            tokio_tungstenite::accept_async_with_config(
                tcp,
                Some(ws_config(DEFAULT_MAX_ENVELOPE_BYTES)),
            ),
        )
        .await
        .expect("the websocket upgrade is bounded")
        .expect("websocket upgrade");
        let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
        let ports: Arc<dyn RemoteServePorts + Send + Sync> = Arc::new(
            RemoteServePortsComposite::new(Arc::new(MemoryOnlyHost), None, None)
                .with_memory(memory_service),
        );
        connect_responder(ConnectResponderOptions {
            transport,
            identity: RemoteIdentity { seed: HOST_SEED },
            manifest: host_manifest,
            allowlist: vec![client_peer.clone()],
            peer_keys: HashMap::from([(client_peer, client_pubkey)]),
            ports: Some(ports),
            invoke_timeout_ms: Some(5_000),
        })
        .await
    });
    (addr, task)
}

/// Await one responder task without letting a fixture bug hang the suite.
async fn responder_of(
    task: tokio::task::JoinHandle<Arc<ConnectResponder>>,
) -> Arc<ConnectResponder> {
    tokio::time::timeout(BOUND, task)
        .await
        .expect("the responder task is bounded")
        .expect("the responder task completed")
}

// ---------------------------------------------------------------------------
// The disposable home, the selection and the open
// ---------------------------------------------------------------------------

/// One disposable user home, bootstrapped through the real public home
/// contract: a registered Creator, a selected workspace (which materializes the
/// state DB and the workspace `creators` row the FK prechecks rely on) and the
/// operator-provisioned credential for the dialing seed.
struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    creator_id: String,
}

impl Env {
    async fn boot(display_name: &str) -> Self {
        let tmp = TempDir::new().expect("temp home");
        let user_home = tmp.path().to_path_buf();
        let home = CoreHomeService::open(user_home.clone()).expect("open home");
        let creator_id = home
            .register_creator(CoreRegisterCreatorRequest {
                display_name: Some(display_name.parse().expect("valid display name")),
                platform_creator_id: None,
            })
            .await
            .expect("register the active creator")
            .creator_id;
        fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home,
            &creator_id,
            WORKSPACE,
        ))
        .expect("workspace dir");
        home.select_workspace(SetActiveWorkspaceRequest {
            creator_id: None,
            workspace_slug: WORKSPACE.to_string(),
        })
        .await
        .expect("select the workspace");

        let env = Self {
            _tmp: tmp,
            user_home,
            creator_id,
        };
        env.write_credential();
        env
    }

    /// Write the operator-provisioned credential for the dialing seed at
    /// mode `0600`.
    fn write_credential(&self) {
        let dir = self.user_home.join(".nexus42").join("memory-credentials");
        fs::create_dir_all(&dir).expect("credential dir");
        let path = dir.join(format!("{CREDENTIAL_ID}.json"));
        fs::write(
            &path,
            format!(
                "{{\"identity_seed_hex\":\"{}\",\"capability_token\":null}}",
                hex(&CLIENT_SEED)
            ),
        )
        .expect("write the credential");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .expect("credential mode 0600");
        }
    }

    /// The local provider's private file root: its absence is the observable
    /// that no local provider was constructed.
    fn local_provider_store(&self) -> PathBuf {
        nexus_home_layout::operational_workspace_dir(&self.user_home, &self.creator_id, WORKSPACE)
            .join("memory-store")
    }

    /// The legacy bearer file the remote binding must never touch.
    fn legacy_bearer(&self) -> PathBuf {
        self.user_home
            .join(".nexus42")
            .join("creators")
            .join(&self.creator_id)
            .join("memory")
            .join("long-term")
            .join("pinned.md")
    }

    fn state_db(&self) -> PathBuf {
        nexus_home_layout::workspace_state_db_path(&self.user_home, &self.creator_id, WORKSPACE)
    }

    fn options(&self, selection: MemoryProviderSelection) -> CoreOpenOptions {
        CoreOpenOptions {
            user_home: self.user_home.clone(),
            access: CoreAccess::EngineOwner,
            memory: Some(selection),
        }
    }
}

/// The durable, independent provider this suite serves, plus the exact
/// peer→principal admission the operator provisioned for the dialing seed.
struct Provider {
    _home: TempDir,
    db: PathBuf,
    inner: Arc<FixtureMemoryProvider>,
    directory: Arc<PeerDirectory>,
    space_id: String,
}

impl Provider {
    async fn open() -> Self {
        let home = TempDir::new().expect("fixture home");
        let db = home.path().join("memory.sqlite");
        let inner = Arc::new(
            FixtureMemoryProvider::open(&db)
                .await
                .expect("open the durable fixture"),
        );
        let client_peer = peer_id(&CLIENT_SEED);
        let principal_id = format!("prn_{client_peer}");
        inner
            .provision_operator_admin(&principal_id)
            .await
            .expect("operator provisions the admin grant");
        let space_id = inner.identity().space_id.clone();
        Self {
            _home: home,
            db,
            inner,
            directory: Arc::new(
                PeerDirectory::new().with_peer(client_peer.clone(), principal_id),
            ),
            space_id,
        }
    }

    fn faults(&self) -> FixtureFaults {
        self.inner.faults().clone()
    }

    /// Serve one signed session over a fresh loopback listener.
    async fn serve(&self) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        start_listener(
            self.inner.server(self.directory.admission()),
            manifest(self.inner.identity().authority_host_id.as_str()),
            peer_id(&CLIENT_SEED),
            pubkey(&CLIENT_SEED),
        )
        .await
    }

    fn selection(&self, endpoint: SocketAddr) -> MemoryProviderSelection {
        MemoryProviderSelection::Remote(RemoteMemoryProviderSelection {
            space_id: self.space_id.clone(),
            credential_id: CREDENTIAL_ID.to_string(),
            connect_endpoint: format!("ws://{endpoint}"),
            remote_pubkey_hex: hex(&pubkey(&HOST_SEED)),
            allowlist: vec![derive_peer_id_from_ed25519_pubkey(&pubkey(&HOST_SEED))],
        })
    }

    /// Open Core against this provider and return it with the responder task
    /// that served the session.
    async fn open_core(
        &self,
        env: &Env,
    ) -> (CoreService, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        let (addr, task) = self.serve().await;
        let core = CoreService::open(env.options(self.selection(addr)))
            .await
            .expect("the selected remote provider opens");
        (core, task)
    }

    /// The provider's own database, read directly: the durable effects are
    /// asserted there instead of through the port under test.
    async fn pool(&self) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&self.db)
                .read_only(true),
        )
        .await
        .expect("open the fixture database")
    }
}

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid request DTO")
}

// ---------------------------------------------------------------------------
// Nexus-side observation helpers
// ---------------------------------------------------------------------------

async fn core_db(env: &Env) -> SqlitePool {
    SqlitePool::connect_with(SqliteConnectOptions::new().filename(env.state_db()))
        .await
        .expect("open the workspace state database")
}

async fn count(pool: &SqlitePool, sql: &str, bind: &str) -> i64 {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
        .bind(bind)
        .fetch_one(pool)
        .await
        .expect("count query")
        .get::<i64, _>(0)
}

async fn scalar(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(pool)
        .await
        .expect("scalar query")
        .get::<i64, _>(0)
}

/// The binding's still-open D18 wire admissions.
async fn admission_count(pool: &SqlitePool, binding_id: &str) -> i64 {
    count(
        pool,
        "SELECT COUNT(*) FROM remote_binding_memory_admissions WHERE binding_id = ?",
        binding_id,
    )
    .await
}

/// The binding's recorded known provider-object facts as `(kind, id, state)`.
async fn object_facts(pool: &SqlitePool, binding_id: &str) -> Vec<(String, String, String)> {
    sqlx::query(
        "SELECT object_kind, object_id, state FROM remote_binding_memory_objects \
         WHERE binding_id = ? ORDER BY object_kind, object_id",
    )
    .bind(binding_id)
    .fetch_all(pool)
    .await
    .expect("object facts")
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("object_kind"),
            row.get::<String, _>("object_id"),
            row.get::<String, _>("state"),
        )
    })
    .collect()
}

/// Assert the authoritative removal gate refuses this binding for remote
/// memory, and that the refusal mutated nothing.
async fn assert_remote_memory_holds(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
) {
    let refused = core
        .remove_binding(
            principal,
            character_id.to_string(),
            binding_id.to_string(),
        )
        .await
        .expect_err("an open remote dependency must keep the binding");
    assert!(
        matches!(&refused, CoreError::ActorConflict { code, .. } if code == "binding_has_remote_memory"),
        "got {refused:?}"
    );
}

// ---------------------------------------------------------------------------
// Provider-side observation helpers
// ---------------------------------------------------------------------------

async fn provider_candidate(
    pool: &SqlitePool,
    candidate_id: &str,
) -> Option<(String, Option<String>, i64)> {
    sqlx::query(
        "SELECT status, capture_context_id, revision FROM candidates WHERE candidate_id = ?",
    )
    .bind(candidate_id)
    .fetch_optional(pool)
    .await
    .expect("candidate row")
    .map(|row| {
        (
            row.get::<String, _>("status"),
            row.get::<Option<String>, _>("capture_context_id"),
            row.get::<i64, _>("revision"),
        )
    })
}

/// The subject's candidate rows as `(status, visibility, context, kind)`.
async fn provider_candidates(
    pool: &SqlitePool,
    subject_id: &str,
) -> Vec<(String, String, Option<String>, String)> {
    sqlx::query(
        "SELECT status, capture_visibility, capture_context_id, kind FROM candidates \
         WHERE subject_id = ? ORDER BY created_at, candidate_id",
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await
    .expect("candidate rows")
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("status"),
            row.get::<String, _>("capture_visibility"),
            row.get::<Option<String>, _>("capture_context_id"),
            row.get::<String, _>("kind"),
        )
    })
    .collect()
}

/// The single candidate a capture receipt named, read off the provider.
async fn only_candidate_id(pool: &SqlitePool, subject_id: &str) -> String {
    let rows = sqlx::query("SELECT candidate_id, status FROM candidates WHERE subject_id = ?")
        .bind(subject_id)
        .fetch_all(pool)
        .await
        .expect("candidate id");
    assert_eq!(rows.len(), 1, "one candidate for the subject");
    assert_eq!(
        rows[0].get::<String, _>("status"),
        "pending",
        "the fresh candidate is pending"
    );
    rows[0].get::<String, _>("candidate_id")
}

async fn provider_subject(
    pool: &SqlitePool,
    subject_id: &str,
) -> Option<(Option<String>, Option<String>, String)> {
    sqlx::query("SELECT kind_ns, kind_id, status FROM subjects WHERE subject_id = ?")
        .bind(subject_id)
        .fetch_optional(pool)
        .await
        .expect("subject row")
        .map(|row| {
            (
                row.get::<Option<String>, _>("kind_ns"),
                row.get::<Option<String>, _>("kind_id"),
                row.get::<String, _>("status"),
            )
        })
}

async fn provider_receipts(pool: &SqlitePool, op: &str) -> i64 {
    count(
        pool,
        "SELECT COUNT(*) FROM receipts WHERE op = ? AND status = 'committed'",
        op,
    )
    .await
}

// ---------------------------------------------------------------------------
// Fixture setup through the real public Core actor surface
// ---------------------------------------------------------------------------

async fn create_world(core: &CoreService, principal: &Principal, title: &str) -> String {
    core.create_world(
        principal,
        dto(json!({ "title": title })),
    )
    .await
    .expect("create an owned World")
    .world_id
}

async fn create_character(
    core: &CoreService,
    principal: &Principal,
    display_name: &str,
    world_id: &str,
) -> (String, String, i64) {
    let created = core
        .create_character(
            principal,
            dto(json!({ "display_name": display_name, "world_id": world_id })),
        )
        .await
        .expect("create an owned Character with its initial binding");
    (
        created.character.character_id.as_str().to_string(),
        created.binding.binding_id.as_str().to_string(),
        created.character.revision,
    )
}

async fn add_binding(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    world_id: &str,
) -> String {
    core.add_binding(
        principal,
        character_id.to_string(),
        world_id.to_string(),
        None,
    )
    .await
    .expect("add an active binding")
    .binding
    .binding_id
    .as_str()
    .to_string()
}

fn capture_request(binding_id: Option<&str>, pending_id: &str, digest: &str) -> Value {
    let mut request = json!({
        "pending_id": pending_id,
        "session_id": format!("sess_{pending_id}"),
        "task_kind": "brainstorm",
        "raw_digest": digest,
        "created_at": "2026-01-01T00:00:01Z",
    });
    if let Some(binding_id) = binding_id {
        request["binding_id"] = json!(binding_id);
    }
    request
}

async fn capture(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: Option<&str>,
    pending_id: &str,
) -> CoreResult<()> {
    core.capture_character_pending_review(
        principal,
        character_id.to_string(),
        dto::<CaptureCharacterPendingReviewRequest>(capture_request(
            binding_id,
            pending_id,
            PROMOTE_DIGEST,
        )),
    )
    .await
    .map(|_| ())
}

/// The standard refusal every memory call under the pinned binding carries.
fn provider_refusal(error: &CoreError) -> (&str, &str, MemoryProviderRefusal, Option<&str>) {
    match error {
        CoreError::MemoryProviderRefused {
            code,
            retry_class,
            family,
            wire_code,
            ..
        } => (code, retry_class, *family, wire_code.as_deref()),
        other => panic!("expected a classified provider refusal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Subjects, capture and candidate reads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn subject_capture_and_candidate_reads_run_on_the_remote_provider() {
    let env = Env::boot("R2 Subject Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");

    // A real owned World, Character and a second binding: the actor surface
    // itself is Nexus-side and unrelated to the pinned memory provider.
    let world_a = create_world(&core, &principal, "R2 World A").await;
    let world_b = create_world(&core, &principal, "R2 World B").await;
    let (character_id, binding_a, _) =
        create_character(&core, &principal, "R2 Ada", &world_a).await;
    let binding_b = add_binding(&core, &principal, &character_id, &world_b).await;
    let world_c = create_world(&core, &principal, "R2 World C").await;
    let binding_c = add_binding(&core, &principal, &character_id, &world_c).await;

    // Two captures: one shared Character scope, one binding-local.
    capture(&core, &principal, &character_id, None, "pend_shared")
        .await
        .expect("shared capture");
    capture(
        &core,
        &principal,
        &character_id,
        Some(&binding_a),
        "pend_local",
    )
    .await
    .expect("binding-local capture");
    // A second active binding of the same Character owns its own scope.
    capture(
        &core,
        &principal,
        &character_id,
        Some(&binding_b),
        "pend_other_binding",
    )
    .await
    .expect("binding-local capture on the second binding");

    // The provider is the independent durable owner of the effects: both
    // candidates, their placements, the receipts and the ensured subject.
    let pool = provider.pool().await;
    let subject_id = legacy::character_subject_id(&character_id);
    assert_eq!(
        provider_subject(&pool, &subject_id).await,
        Some((
            Some(legacy::CHARACTER_KIND_NS.to_string()),
            Some(character_id.clone()),
            "active".to_string()
        )),
        "the adapter's subject ensure created the Character subject on the provider"
    );
    let mut candidates = provider_candidates(&pool, &subject_id).await;
    candidates.sort();
    let mut expected = vec![
        (
            "pending".to_string(),
            "context".to_string(),
            Some(binding_a.clone()),
            "story_summary".to_string(),
        ),
        (
            "pending".to_string(),
            "context".to_string(),
            Some(binding_b.clone()),
            "story_summary".to_string(),
        ),
        (
            "pending".to_string(),
            "shared".to_string(),
            None,
            "story_summary".to_string(),
        ),
    ];
    expected.sort();
    assert_eq!(
        candidates, expected,
        "the standard capture op stored every placement under the pinned subject kind"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.capture").await,
        3,
        "each capture committed its own provider receipt"
    );
    let shared_candidate = sqlx::query(
        "SELECT candidate_id FROM candidates WHERE subject_id = ? AND capture_visibility = 'shared'",
    )
    .bind(&subject_id)
    .fetch_one(&pool)
    .await
    .expect("shared candidate")
    .get::<String, _>("candidate_id");
    pool.close().await;

    // Candidate list: the retained scope split is served from the provider.
    let shared = core
        .list_character_pending_reviews(&principal, character_id.clone(), None, 50, 0)
        .await
        .expect("shared queue");
    assert_eq!(shared.items.len(), 1, "the shared scope sees one row");
    assert_eq!(shared.items[0].pending_id.as_str(), "pend_shared");
    assert_eq!(shared.items[0].raw_digest.as_str(), PROMOTE_DIGEST);
    assert_eq!(shared.items[0].session_id.as_str(), "sess_pend_shared");
    assert!(
        shared.items[0].binding_id.is_none(),
        "a shared row carries no binding provenance"
    );
    let local = core
        .list_character_pending_reviews(
            &principal,
            character_id.clone(),
            Some(binding_a.clone()),
            50,
            0,
        )
        .await
        .expect("binding-local queue");
    assert_eq!(local.items.len(), 1, "the binding scope sees only its own row");
    assert_eq!(local.items[0].pending_id.as_str(), "pend_local");
    assert_eq!(
        local.items[0].binding_id.as_deref().map(|id| id.as_str().to_string()),
        Some(binding_a.clone())
    );
    assert_eq!(
        core.count_character_pending_reviews(&principal, character_id.clone(), Some(binding_a.clone()))
            .await
            .expect("binding-local count")
            .count,
        1
    );

    // Candidate get + dismissal: the retained miss is not-found and the
    // provider's row is consumed exactly once.
    core.delete_character_pending_review(
        &principal,
        character_id.clone(),
        "pend_shared".to_string(),
    )
    .await
    .expect("dismiss the shared pending row");
    let pool = provider.pool().await;
    assert_eq!(
        provider_candidate(&pool, &shared_candidate).await,
        Some(("consumed".to_string(), None, 2)),
        "the standard dismissal consumed the provider candidate"
    );
    pool.close().await;
    let repeated = core
        .delete_character_pending_review(
            &principal,
            character_id.clone(),
            "pend_shared".to_string(),
        )
        .await
        .expect_err("a consumed row is no longer queued");
    assert!(matches!(repeated, CoreError::NotFound { .. }), "got {repeated:?}");
    assert_eq!(
        core.count_character_pending_reviews(&principal, character_id.clone(), None)
            .await
            .expect("count after dismissal")
            .count,
        0,
        "a dismissed row leaves the shared queue"
    );

    // Two callers dismissing the same row at once: the provider's dedup key
    // makes the second a replay of the first, so both may answer the retained
    // success — but there is exactly ONE durable dismissal receipt and ONE
    // consumption, never a second effect.
    let baseline = provider.pool().await;
    let dismiss_before = provider_receipts(&baseline, "memory.candidates.dismiss").await;
    baseline.close().await;
    capture(&core, &principal, &character_id, None, "pend_race")
        .await
        .expect("a candidate to race");
    let first = core.delete_character_pending_review(
        &principal,
        character_id.clone(),
        "pend_race".to_string(),
    );
    let second = core.delete_character_pending_review(
        &principal,
        character_id.clone(),
        "pend_race".to_string(),
    );
    let (first, second) = tokio::join!(first, second);
    assert!(
        first.is_ok() || second.is_ok(),
        "at least one caller observes the committed dismissal: {first:?} / {second:?}"
    );
    for answer in [&first, &second] {
        if let Err(error) = answer {
            assert!(
                matches!(error, CoreError::NotFound { .. }),
                "the only other answer is the retained absence: {error:?}"
            );
        }
    }
    let pool = provider.pool().await;
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.dismiss").await,
        dismiss_before + 1,
        "the keyed dismissal committed exactly one durable effect"
    );
    assert_eq!(
        sqlx::query(
            "SELECT COUNT(*) FROM candidates WHERE subject_id = ? \
             AND capture_visibility = 'shared' AND status = 'pending'",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .expect("pending shared candidates")
        .get::<i64, _>(0),
        0,
        "the raced candidate is consumed and nothing else in its scope is"
    );
    pool.close().await;

    // Review: the drain consumes the binding-local candidate and writes its
    // `fact` fragment on the provider, in the binding's own placement.
    let reviewed = core
        .review_character_memory(
            &principal,
            character_id.clone(),
            dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_a })),
        )
        .await
        .expect("review drains the binding scope");
    assert_eq!(reviewed.fragmented, 1, "the binding-local promote is coerced");
    assert_eq!(reviewed.promoted, 0);
    assert_eq!(reviewed.dropped, 0);
    let pool = provider.pool().await;
    let after_review = provider_candidates(&pool, &subject_id).await;
    assert!(
        after_review.iter().all(|(status, _, context, _)| {
            status == "consumed" || context.as_deref() == Some(binding_b.as_str())
        }),
        "the dismissal and the review consumed their own candidates: {after_review:?}"
    );
    assert!(
        after_review
            .iter()
            .any(|(status, _, context, _)| status == "pending"
                && context.as_deref() == Some(binding_b.as_str())),
        "the other binding's queue is not consumed by this scope's review"
    );
    let fragment = sqlx::query(
        "SELECT record_id, kind, scope_context_id FROM records WHERE subject_id = ?",
    )
    .bind(&subject_id)
    .fetch_one(&pool)
    .await
    .expect("the reviewed fragment record");
    assert_eq!(fragment.get::<String, _>("kind"), "fact");
    assert_eq!(
        fragment.get::<Option<String>, _>("scope_context_id"),
        Some(binding_a.clone()),
        "a binding-local review writes the record on the provider under the binding placement"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.review").await,
        1,
        "the review reached the standard review op"
    );
    pool.close().await;

    // Two DIFFERENT standard mutations racing on one opaque candidate: the
    // provider's revision check admits exactly one consumption, so one of the
    // two callers observes the other's committed effect instead of a second
    // write. Which caller wins is a race; the invariant is one receipt.
    let baseline = provider.pool().await;
    let dismiss_before = provider_receipts(&baseline, "memory.candidates.dismiss").await;
    let review_before = provider_receipts(&baseline, "memory.candidates.review").await;
    baseline.close().await;
    capture(
        &core,
        &principal,
        &character_id,
        Some(&binding_c),
        "pend_cas",
    )
    .await
    .expect("a candidate for the CAS race");
    let dismiss = core.delete_character_pending_review(
        &principal,
        character_id.clone(),
        "pend_cas".to_string(),
    );
    let review = core.review_character_memory(
        &principal,
        character_id.clone(),
        dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_c })),
    );
    let (dismiss, review) = tokio::join!(dismiss, review);
    let reviewed = review.expect("the review reports its own bounded batch");
    assert!(
        dismiss.is_ok() || reviewed.processed == Some(1),
        "whichever caller lost reports the winner's effect, never a second one: \
         {dismiss:?} / {reviewed:?}"
    );
    if let Err(error) = &dismiss {
        assert!(
            matches!(error, CoreError::NotFound { .. }),
            "the losing dismissal is the retained absence: {error:?}"
        );
    }
    let pool = provider.pool().await;
    let committed = provider_receipts(&pool, "memory.candidates.dismiss").await - dismiss_before
        + provider_receipts(&pool, "memory.candidates.review").await - review_before;
    assert_eq!(
        committed, 1,
        "exactly one of the two raced mutations committed a durable effect"
    );
    assert_eq!(
        sqlx::query("SELECT COUNT(*) FROM candidates WHERE subject_id = ? AND status = 'pending'")
            .bind(&subject_id)
            .fetch_one(&pool)
            .await
            .expect("pending candidates")
            .get::<i64, _>(0),
        1,
        "the raced candidate is consumed once; the untouched binding row remains"
    );
    pool.close().await;

    // The Creator arm shares the same pinned provider and its own subject.
    core.create_pending_review(
        &principal,
        dto::<CreatePendingReviewRequest>(json!({
            "creator_id": env.creator_id,
            "pending_id": "pend_creator",
            "session_id": "sess_pend_creator",
            "task_kind": "brainstorm",
            "raw_digest": PROMOTE_DIGEST,
            "created_at": "2026-01-01T00:00:02Z",
        })),
    )
    .await
    .expect("creator capture");
    let listed = core
        .list_pending_reviews(&principal, None, 10)
        .await
        .expect("creator queue");
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].pending_id.as_str(), "pend_creator");
    assert_eq!(
        core.count_pending_reviews(&principal).await.expect("count").count,
        1
    );
    let pool = provider.pool().await;
    let creator_subject = legacy::creator_subject_id(&env.creator_id);
    assert_eq!(
        provider_subject(&pool, &creator_subject)
            .await
            .map(|(_, _, status)| status),
        Some("active".to_string()),
        "the Creator capture ensured its own provider subject"
    );
    pool.close().await;

    // No local provider and no local legacy content were touched: the remote
    // binding never constructs the local provider and never reads or writes the
    // legacy queue.
    let legacy = env.legacy_bearer();
    fs::create_dir_all(legacy.parent().expect("legacy dir")).expect("legacy dir");
    fs::write(&legacy, b"# local legacy memory\n").expect("seed the legacy bearer");
    let nexus = core_db(&env).await;
    // A remote Creator miss must come from the provider alone: the local legacy
    // queue and its cross-Creator ownership re-read (the retained 403) are
    // local-only paths. Removing the legacy table from this disposable state DB
    // makes any consultation fail loudly, so the retained not-found answer here
    // proves the read never happens; the table is restored afterwards.
    sqlx::query("ALTER TABLE memory_pending_review RENAME TO memory_pending_review_r2_probe")
        .execute(&nexus)
        .await
        .expect("withhold the legacy queue from the remote path");
    let miss = core
        .delete_pending_review(&principal, "pend_legacy_foreign".to_string())
        .await
        .expect_err("the remote miss is not a dismissal");
    assert!(
        matches!(&miss, CoreError::NotFound { .. }),
        "a remote miss never becomes the legacy ownership 403 or a local storage error: {miss:?}"
    );
    sqlx::query("ALTER TABLE memory_pending_review_r2_probe RENAME TO memory_pending_review")
        .execute(&nexus)
        .await
        .expect("restore the legacy queue");
    assert_eq!(
        scalar(&nexus, "SELECT COUNT(*) FROM memory_pending_review").await,
        0,
        "the legacy Creator queue was never written"
    );
    assert_eq!(
        scalar(&nexus, "SELECT COUNT(*) FROM character_memory_pending_review").await,
        0,
        "the legacy Character queue stays write-idle"
    );
    assert_eq!(
        scalar(&nexus, "SELECT COUNT(*) FROM memory_candidates").await,
        0,
        "the local provider's own candidate table stays empty"
    );
    assert_eq!(
        scalar(&nexus, "SELECT COUNT(*) FROM memory_receipts").await,
        0,
        "the local provider's own receipt table stays empty"
    );
    nexus.close().await;
    assert!(
        !env.local_provider_store().exists(),
        "a remote binding never constructs the local provider"
    );
    assert_eq!(
        fs::read(&legacy).expect("legacy bearer"),
        b"# local legacy memory\n",
        "the remote path left the legacy bearer untouched"
    );

    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str()),
        "every effect above travelled the signed session of the loaded credential"
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
}

// ---------------------------------------------------------------------------
// The D18 removal gate and its settlements
// ---------------------------------------------------------------------------

#[tokio::test]
async fn binding_removal_follows_known_remote_dependencies() {
    let env = Env::boot("R2 Gate Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("principal");

    let mut worlds = Vec::new();
    for index in 0..6 {
        worlds.push(create_world(&core, &principal, &format!("R2 Gate {index}")).await);
    }
    let (character_id, keeper, _) =
        create_character(&core, &principal, "R2 Gate Ada", &worlds[0]).await;
    let dismissed = add_binding(&core, &principal, &character_id, &worlds[1]).await;
    let reviewed = add_binding(&core, &principal, &character_id, &worlds[2]).await;
    let no_effect = add_binding(&core, &principal, &character_id, &worlds[3]).await;
    let replayed = add_binding(&core, &principal, &character_id, &worlds[4]).await;

    // (1) A committed binding-local capture is a known live dependency: the
    // authoritative removal gate refuses until it is actually disposed.
    capture(
        &core,
        &principal,
        &character_id,
        Some(&dismissed),
        "pend_gate_dismiss",
    )
    .await
    .expect("binding-local capture");
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &dismissed).await,
        0,
        "a committed capture retires exactly its own admission"
    );
    let published = object_facts(&nexus, &dismissed).await;
    assert_eq!(published.len(), 1, "the receipt published one known candidate");
    assert_eq!(published[0].0, "candidate");
    assert_eq!(published[0].2, "live");
    let candidate_id = published[0].1.clone();
    nexus.close().await;
    assert_remote_memory_holds(&core, &principal, &character_id, &dismissed).await;

    // Disposing it (the standard dismissal acknowledgement) closes the known
    // dependency, and the same removal then succeeds.
    core.delete_character_pending_review(
        &principal,
        character_id.clone(),
        "pend_gate_dismiss".to_string(),
    )
    .await
    .expect("dismiss the binding-local capture");
    let nexus = core_db(&env).await;
    assert_eq!(
        object_facts(&nexus, &dismissed).await,
        vec![(
            "candidate".to_string(),
            candidate_id.clone(),
            "released".to_string()
        )],
        "the dismissal acknowledgement is the candidate's released fact"
    );
    nexus.close().await;
    core.remove_binding(&principal, character_id.clone(), dismissed.clone())
        .await
        .expect("a disposed candidate no longer holds the binding");

    // (2) A review binds the record it created: the record reference is a live
    // dependency until the record leaves this binding's placement.
    capture(
        &core,
        &principal,
        &character_id,
        Some(&reviewed),
        "pend_gate_review",
    )
    .await
    .expect("binding-local capture for review");
    core.review_character_memory(
        &principal,
        character_id.clone(),
        dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": reviewed })),
    )
    .await
    .expect("review the binding-local candidate");
    let nexus = core_db(&env).await;
    let facts = object_facts(&nexus, &reviewed).await;
    assert!(
        facts
            .iter()
            .any(|(kind, _, state)| kind == "record" && state == "live"),
        "the committed review published its record reference: {facts:?}"
    );
    nexus.close().await;
    assert_remote_memory_holds(&core, &principal, &character_id, &reviewed).await;

    let fragment = core
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(reviewed.clone()),
            50,
            0,
        )
        .await
        .expect("binding fragments")
        .fragments
        .first()
        .cloned()
        .expect("the reviewed fragment");
    core.promote_character_fragment(
        &principal,
        character_id.clone(),
        fragment.fragment_id.as_str().to_string(),
        i64::try_from(fragment.revision).expect("fragment revision"),
    )
    .await
    .expect("promote the fragment out of the binding");
    let nexus = core_db(&env).await;
    let facts = object_facts(&nexus, &reviewed).await;
    assert!(
        facts.iter().all(|(_, _, state)| state != "live"),
        "the promotion released the record while the review consumed the candidate: {facts:?}"
    );
    assert!(
        facts.iter().any(|(kind, _, state)| kind == "record" && state == "released"),
        "the promotion's own record fact is released: {facts:?}"
    );
    nexus.close().await;
    core.remove_binding(&principal, character_id.clone(), reviewed.clone())
        .await
        .expect("a promoted record no longer holds the binding");

    // (3) A refusal that proves this attempt had no effect settles its
    // admission: the definite answer keeps the binding removable.
    let over_limit = "x".repeat(OVER_LIMIT_DIGEST_BYTES);
    let refused = core
        .capture_character_pending_review(
            &principal,
            character_id.clone(),
            dto::<CaptureCharacterPendingReviewRequest>(capture_request(
                Some(&no_effect),
                "pend_gate_no_effect",
                &over_limit,
            )),
        )
        .await
        .expect_err("the provider refuses a payload over its own bound");
    let (code, retry_class, family, wire_code) = provider_refusal(&refused);
    assert_eq!(code, "INVALID_INPUT", "the provider's invalid input code");
    assert_eq!(retry_class, "terminal", "a proven pre-effect refusal");
    assert_eq!(family, MemoryProviderRefusal::InvalidInput);
    assert_eq!(wire_code, None, "a known memory code needs no wire code");
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &no_effect).await,
        0,
        "a definite no-effect refusal retires its own admission"
    );
    assert!(
        object_facts(&nexus, &no_effect).await.is_empty(),
        "a refused capture publishes no candidate fact"
    );
    nexus.close().await;
    core.remove_binding(&principal, character_id.clone(), no_effect.clone())
        .await
        .expect("a settled no-effect attempt leaves the binding removable");

    // (4) A replayed receipt cannot revive a terminal fact: the same-key
    // capture answers the original receipt, whose publication is a stale
    // `live` snapshot of the already-released candidate.
    let (addr, task) = provider.serve().await;
    let second = CoreService::open(env.options(provider.selection(addr)))
        .await
        .expect("a second signed session to the same provider");
    let second_principal = second.active_principal().await.expect("principal");
    capture(
        &second,
        &second_principal,
        &character_id,
        Some(&replayed),
        "pend_gate_replay",
    )
    .await
    .expect("capture");
    let candidate = object_facts(&core_db(&env).await, &replayed)
        .await
        .first()
        .expect("the published candidate")
        .1
        .clone();
    second
        .delete_character_pending_review(
            &second_principal,
            character_id.clone(),
            "pend_gate_replay".to_string(),
        )
        .await
        .expect("dismiss it");
    capture(
        &second,
        &second_principal,
        &character_id,
        Some(&replayed),
        "pend_gate_replay",
    )
    .await
    .expect("the same pending_id replays the committed receipt");
    let nexus = core_db(&env).await;
    assert_eq!(
        object_facts(&nexus, &replayed).await,
        vec![("candidate".to_string(), candidate.clone(), "released".to_string())],
        "the replayed receipt published the same candidate and could not revive it"
    );
    nexus.close().await;
    second
        .remove_binding(&second_principal, character_id.clone(), replayed.clone())
        .await
        .expect("a terminal fact never blocks removal");
    assert!(
        second.close().await.expect("close").cleanup_confirmed,
        "the second open releases its own admission"
    );
    responder_of(task).await;

    // The keeper binding is the last one standing; nothing above touched it.
    core.remove_binding(&principal, character_id.clone(), keeper.clone())
        .await
        .expect_err("the last active binding still refuses removal");
    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str())
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the root open releases its own admission"
    );
}

// ---------------------------------------------------------------------------
// Provenance refusals
// ---------------------------------------------------------------------------

#[tokio::test]
async fn provenance_refusals_write_no_remote_effect() {
    let env = Env::boot("R2 Provenance Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("principal");

    let world_a = create_world(&core, &principal, "R2 Prov A").await;
    let world_b = create_world(&core, &principal, "R2 Prov B").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "R2 Prov Ada", &world_a).await;
    let (other_character, other_binding, other_revision) =
        create_character(&core, &principal, "R2 Prov Bea", &world_b).await;
    // The gate below needs a second active binding: the last one is its own
    // retained 409, which would hide the remote-memory answer.
    let world_c = create_world(&core, &principal, "R2 Prov C").await;
    add_binding(&core, &principal, &character_id, &world_c).await;

    let pool = provider.pool().await;

    // A foreign subject's own queue row: the other Character captures through
    // its own real binding, so the provider holds a pending candidate under a
    // subject this bearer does not own.
    capture(
        &core,
        &principal,
        &other_character,
        Some(&other_binding),
        "pend_other_subject",
    )
    .await
    .expect("the other Character's own capture");
    let other_subject = legacy::character_subject_id(&other_character);
    let other_candidate = sqlx::query("SELECT candidate_id FROM candidates WHERE subject_id = ?")
        .bind(&other_subject)
        .fetch_one(&pool)
        .await
        .expect("the other subject's candidate")
        .get::<String, _>("candidate_id");
    let before = scalar(&pool, "SELECT COUNT(*) FROM candidates").await;

    // Dismissing another subject's retained pending identity is the retained
    // not-found and mutates nothing: the provider authorizes the candidate op by
    // space-wide grant coverage, so the subject/identity match is the only
    // thing keeping one bearer off another's row.
    let stolen = core
        .delete_character_pending_review(
            &principal,
            character_id.clone(),
            "pend_other_subject".to_string(),
        )
        .await
        .expect_err("another subject's pending row is not this bearer's");
    assert!(
        matches!(&stolen, CoreError::NotFound { .. }),
        "the retained foreign-subject answer: {stolen:?}"
    );
    assert_eq!(
        provider_candidate(&pool, &other_candidate).await,
        Some(("pending".to_string(), Some(other_binding.clone()), 1)),
        "the foreign subject's candidate stays pending and unmutated"
    );

    // A missing binding: the proven provenance refusal precedes the provider.
    let missing = core
        .capture_character_pending_review(
            &principal,
            character_id.clone(),
            dto::<CaptureCharacterPendingReviewRequest>(capture_request(
                Some("awb_00000000000000000000000000000000"),
                "pend_missing",
                PROMOTE_DIGEST,
            )),
        )
        .await
        .expect_err("a missing binding cannot own a binding-local capture");
    assert!(
        matches!(&missing, CoreError::ForbiddenReason { resource, .. } if resource == "character_memory"),
        "the retained provenance answer: {missing:?}"
    );

    // A binding of another Character: foreign provenance is the same refusal.
    let foreign = core
        .capture_character_pending_review(
            &principal,
            character_id.clone(),
            dto::<CaptureCharacterPendingReviewRequest>(capture_request(
                Some(&other_binding),
                "pend_foreign",
                PROMOTE_DIGEST,
            )),
        )
        .await
        .expect_err("another Character's binding cannot own this capture");
    assert!(
        matches!(&foreign, CoreError::ForbiddenReason { resource, .. } if resource == "character_memory"),
        "the retained provenance answer: {foreign:?}"
    );

    // An inactive bearer: the archived Character is refused at its activity
    // lease, before any provenance check or provider write.
    core.transition_character(
        &principal,
        dto::<CoreCharacterTransitionRequest>(json!({
            "character_id": other_character,
            "expected_revision": other_revision,
            "target_status": "archived",
        })),
    )
    .await
    .expect("archive the other Character");
    let inactive = core
        .capture_character_pending_review(
            &principal,
            other_character.clone(),
            dto::<CaptureCharacterPendingReviewRequest>(capture_request(
                Some(&other_binding),
                "pend_inactive",
                PROMOTE_DIGEST,
            )),
        )
        .await
        .expect_err("an archived Character cannot capture");
    assert!(
        matches!(&inactive, CoreError::ActorConflict { code, .. } if code == "character_inactive"),
        "the retained inactive answer: {inactive:?}"
    );

    // None of the three refusals wrote anything: no provider candidate, no
    // admission, no known-object fact, and the live binding is untouched.
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM candidates").await,
        before,
        "provenance refusals wrote no provider candidate"
    );
    pool.close().await;
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &binding_id).await,
        0,
        "a refused provenance took no wire admission"
    );
    assert!(
        object_facts(&nexus, &binding_id).await.is_empty(),
        "a refused provenance published no object fact"
    );
    assert_eq!(
        object_facts(&nexus, &other_binding).await,
        vec![(
            "candidate".to_string(),
            other_candidate.clone(),
            "live".to_string()
        )],
        "only the other Character's own committed capture published a fact"
    );
    nexus.close().await;

    // The refused binding is still a valid, active, removable-only-by-policy
    // binding: capture succeeds once its provenance is real.
    capture(
        &core,
        &principal,
        &character_id,
        Some(&binding_id),
        "pend_after_refusal",
    )
    .await
    .expect("the same binding serves a real capture");
    assert_remote_memory_holds(&core, &principal, &character_id, &binding_id).await;
    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str())
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
}

// ---------------------------------------------------------------------------
// Uncertain and abandoned admissions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn uncertain_and_abandoned_capture_admissions_survive_reopen() {
    let env = Env::boot("R2 Uncertainty Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("principal");

    let mut worlds = Vec::new();
    for index in 0..3 {
        worlds.push(create_world(&core, &principal, &format!("R2 Unknown {index}")).await);
    }
    let (character_id, keeper, _) =
        create_character(&core, &principal, "R2 Unknown Ada", &worlds[0]).await;
    let uncertain = add_binding(&core, &principal, &character_id, &worlds[1]).await;
    let abandoned = add_binding(&core, &principal, &character_id, &worlds[2]).await;
    let subject_id = legacy::character_subject_id(&character_id);

    // A provider-committed capture whose response is lost: the caller learns
    // only that the outcome is unknown, and the attempt's admission stays.
    provider
        .faults()
        .fail_next(nexus_memory_conformance::FaultPoint::AfterCommitBeforeResponse);
    let lost = capture(
        &core,
        &principal,
        &character_id,
        Some(&uncertain),
        "pend_uncertain",
    )
    .await
    .expect_err("a lost post-commit response is not a success");
    let (code, retry_class, family, wire_code) = provider_refusal(&lost);
    assert_eq!(code, "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class, "uncertain");
    assert_eq!(family, MemoryProviderRefusal::Unavailable);
    assert_eq!(wire_code, None);
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &uncertain).await,
        1,
        "the unknown attempt keeps exactly its own admission"
    );
    assert!(
        object_facts(&nexus, &uncertain).await.is_empty(),
        "no result was published for a lost response"
    );
    nexus.close().await;
    assert_remote_memory_holds(&core, &principal, &character_id, &uncertain).await;

    // The admission is durable across a real close and reopen, and the later
    // same-key success — which does publish the original receipt — never
    // clears it.
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the first open releases its own admissions"
    );
    responder_of(responder).await;
    let (reopened, reopened_responder) = provider.open_core(&env).await;
    let reopened_principal = reopened.active_principal().await.expect("principal");
    assert_remote_memory_holds(&reopened, &reopened_principal, &character_id, &uncertain).await;
    // The acknowledgement lands BEFORE any result publication: the lost capture
    // committed at the provider but never published locally, so this dismissal
    // is the FIRST fact ever recorded for that candidate — and it is terminal.
    let pending = reopened
        .list_character_pending_reviews(
            &reopened_principal,
            character_id.clone(),
            Some(uncertain.clone()),
            50,
            0,
        )
        .await
        .expect("the provider really holds the committed candidate");
    assert_eq!(
        pending.items.len(),
        1,
        "the lost response's candidate is queued on the provider"
    );
    let candidate = only_candidate_id(&provider.pool().await, &subject_id).await;
    reopened
        .delete_character_pending_review(
            &reopened_principal,
            character_id.clone(),
            "pend_uncertain".to_string(),
        )
        .await
        .expect("acknowledge the candidate from the provider queue");
    let nexus = core_db(&env).await;
    assert_eq!(
        object_facts(&nexus, &uncertain).await,
        vec![("candidate".to_string(), candidate.clone(), "released".to_string())],
        "the acknowledgement is the first fact recorded for this candidate"
    );
    assert_eq!(
        admission_count(&nexus, &uncertain).await,
        1,
        "a disposal acknowledgement never retires an unknown admission"
    );
    nexus.close().await;

    // The later same-key success publishes the original receipt for the SAME
    // candidate: that stale `live` snapshot can neither revive nor erase the
    // terminal acknowledgement, and the older unknown admission still blocks.
    capture(
        &reopened,
        &reopened_principal,
        &character_id,
        Some(&uncertain),
        "pend_uncertain",
    )
    .await
    .expect("the same idempotency key replays the committed receipt");
    let nexus = core_db(&env).await;
    assert_eq!(
        object_facts(&nexus, &uncertain).await,
        vec![("candidate".to_string(), candidate, "released".to_string())],
        "the stale publication cannot revive a released candidate"
    );
    assert_eq!(
        admission_count(&nexus, &uncertain).await,
        1,
        "the replay retired only its own attempt; the older unknown stays"
    );
    nexus.close().await;
    assert_remote_memory_holds(&reopened, &reopened_principal, &character_id, &uncertain).await;

    // Caller loss mid-flight: the committed effect's response is withheld, the
    // calling task is abandoned, and the admission survives that loss.
    let hold = provider
        .faults()
        .hold_next(nexus_memory_conformance::FaultPoint::ResponseDrop);
    let mut hold = hold;
    let caller = reopened.clone();
    let caller_principal = reopened_principal.clone();
    let character = character_id.clone();
    let binding = abandoned.clone();
    let flying = tokio::spawn(async move {
        capture(&caller, &caller_principal, &character, Some(&binding), "pend_abandoned").await
    });
    hold.wait_reached()
        .await
        .expect("the provider committed and withheld the response");
    flying.abort();
    assert!(
        flying.await.expect_err("the caller task was abandoned").is_cancelled(),
        "the caller itself was lost, not answered"
    );
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &abandoned).await,
        1,
        "an abandoned attempt keeps its admission"
    );
    nexus.close().await;

    assert!(
        reopened.close().await.expect("close").cleanup_confirmed,
        "the reopened session closes"
    );
    responder_of(reopened_responder).await;
    let (final_open, final_responder) = provider.open_core(&env).await;
    let final_principal = final_open.active_principal().await.expect("principal");
    assert_remote_memory_holds(&final_open, &final_principal, &character_id, &abandoned).await;
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &abandoned).await,
        1,
        "the abandoned admission is durable across the real reopen"
    );
    nexus.close().await;
    assert_remote_memory_holds(&final_open, &final_principal, &character_id, &uncertain).await;

    // Protection is per binding, not a shared counter: this binding has no
    // remote dependency and removes even while another holds an unknown
    // admission, and that removal clears nothing of the other binding's facts.
    final_open
        .remove_binding(&final_principal, character_id.clone(), keeper.clone())
        .await
        .expect("a binding with no remote dependency still removes");
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &abandoned).await,
        1,
        "removing another binding never retires this attempt's admission"
    );
    assert_eq!(
        admission_count(&nexus, &uncertain).await,
        1,
        "removing another binding never retires the older unknown admission"
    );
    nexus.close().await;
    assert_remote_memory_holds(&final_open, &final_principal, &character_id, &abandoned).await;
    responder_of(final_responder).await;
    assert!(
        final_open.close().await.expect("close").cleanup_confirmed,
        "the final open releases its own admissions"
    );
}

// ---------------------------------------------------------------------------
// Static, content-free error contracts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remote_refusals_stay_static_without_peer_text() {
    let env = Env::boot("R2 Refusal Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("principal");

    let world = create_world(&core, &principal, "R2 Refusal World").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "R2 Refusal Ada", &world).await;

    // A peer-supplied secret planted in the request must never come back: the
    // provider's own message text (its bound, its wording) and the request's
    // content are both absent from every rendered form of the refusal.
    let secret = "r2-secret-sentinel-0f1e2d3c";
    // Core's own field bound is exactly 64 KiB, so the digest stays legal while
    // the serialized `content` the provider measures no longer is.
    let over_limit = format!(
        "{secret}{}",
        "x".repeat(OVER_LIMIT_DIGEST_BYTES - secret.len())
    );
    let refused = core
        .capture_character_pending_review(
            &principal,
            character_id.clone(),
            dto::<CaptureCharacterPendingReviewRequest>(capture_request(
                Some(&binding_id),
                "pend_secret",
                &over_limit,
            )),
        )
        .await
        .expect_err("the oversized capture is refused");
    let (code, retry_class, family, wire_code) = provider_refusal(&refused);
    assert_eq!(code, "INVALID_INPUT");
    assert_eq!(retry_class, "terminal");
    assert_eq!(family, MemoryProviderRefusal::InvalidInput);
    assert_eq!(wire_code, None);
    for rendered in [
        format!("{refused}"),
        format!("{refused:?}"),
    ] {
        assert!(
            !rendered.contains(secret),
            "the request content never travels: {rendered}"
        );
        assert!(
            !rendered.contains("max_content_bytes"),
            "the provider's own message never travels: {rendered}"
        );
        assert!(
            !rendered.contains("xxxx"),
            "no payload echo in the refusal: {rendered}"
        );
    }

    // A local-only extension keeps its own retained contract on a remote
    // binding: the structured `not_supported` code, not a peer-echoing error.
    let migration = core
        .migrate_legacy_content(&principal)
        .await
        .expect_err("legacy migration is local-only");
    assert!(
        matches!(&migration, CoreError::Coded { code, .. } if code == "not_supported"),
        "got {migration:?}"
    );

    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str())
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
}
