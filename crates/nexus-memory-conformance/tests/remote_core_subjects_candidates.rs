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
//!   file root stay untouched. The subject *read* is exercised as a read: the
//!   SDK's own signed session calls `memory.subjects.get` against the subject
//!   public Core ensured (active identity, unknown subject, and the archived
//!   lifecycle the Core reader branches on).
//! - The legacy Creator queue is withheld from this disposable state DB for the
//!   whole lifecycle, so every operation above is proof that it never reads it
//!   (a read would fail loudly), not merely that it wrote nothing.
//! - The D18 boundary is observed where it is authoritative: the Nexus-side
//!   `remote_binding_memory_admissions`/`_objects` rows (content-free
//!   bookkeeping) and the real public `remove_binding` gate. An unknown wire
//!   attempt keeps the binding unremovable across a real close/reopen, while the
//!   capture is still held in flight, and is never cleared by a later same-key
//!   success, by a published receipt, by a disposal acknowledgement, or by a
//!   post-retention capture that commits a genuinely new receipt; a proven
//!   no-effect refusal and a complete known-object disposal do permit the
//!   authoritative removal.
//! - Refusals reach the caller as the static classified `MemoryProviderRefused`
//!   carrier: the provider's own message text and any request content never
//!   travel with it.
//!
//! `MemoryProviderBinding` is crate-private, so this crate-external test cannot
//! read the pinned `(space, authority, client identity)` tuple back. The
//! namespace columns the admission/object rows carry are the observable
//! substitute: they are written from that pinned tuple by the product code.
//!
//! - The candidate identity guard is exercised on a candidate that really
//!   exists at the deterministic bridge id the client derives from a retained
//!   pending id: a foreign subject, and the right subject with a mismatched
//!   retained identity, are both refused before the provider CAS and mutate
//!   nothing, while the matching control goes through the same lookup branch and
//!   the real CAS consumes it.
//!
//! The adapter's `get_subject` read (`MemoryAdapter::subject_archived`) is
//! reached through its real consumer, the admitted Character Host composition:
//! `CoreService::attach_host` + `HostHandle::create_session`/`execute` build the
//! prompt through `CoreCharacterMind`, so the prompt handed to the deterministic
//! provider adapter is the observable that the projection served the
//! binding-local fact from the real signed provider — and that an archived
//! provider subject is refused (classified `SUBJECT_FENCED`) instead of being
//! served silently. The in-memory adapter exists only so that production path
//! needs no process, model or network; the memory port itself is never mocked.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
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
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    HostOperation, HostStartConfig, LaunchSpec, ManagedSessionHandle, OperationFinishedEvent,
    ProbeRequest, ProtocolKind, ProviderDescriptor, ProviderHealth, SessionOwner,
};
use nexus_agent_host::{
    HostError, HostFacade, HostManager, HostOperationId, HostResult, HostSessionId, LaunchStrategy,
    ProviderAdapter, ProviderId,
};
use nexus_contracts::generated::daemon_api::agent_host::create_session_request::CreateSessionRequest;
use nexus_contracts::generated::daemon_api::agent_host::execute_operation_request::ExecuteOperationRequest;
use nexus_contracts::{CoreRegisterCreatorRequest, ProviderCall, ProviderEventBatch, ProviderReply, SetActiveWorkspaceRequest};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::memory_provider::legacy;
use nexus_core::{
    CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreResult, CoreService,
    MemoryProviderRefusal, MemoryProviderSelection, Principal, RemoteMemoryProviderSelection,
};
use nexus_memory_conformance::{
    FixtureFaults, FixtureMemoryProvider, PeerDirectory, RetentionSweep,
    DEDUP_RETENTION_SECONDS, IDEMPOTENCY_RETENTION_SECONDS, RECEIPT_RETENTION_SECONDS,
};
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponder, ConnectResponderOptions,
    RemoteAdapter, RemoteAdapterOptions, RemoteIdentity, RemoteMemoryService, RemoteServePorts,
    RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, MemoryCandidatesGetRequest,
    MemorySubjectsFenceRequest, MemorySubjectsGetRequest, Principal as MemoryPrincipal, Relation,
    Rule, Scope, TimelineEvent,
};
use spoke_operations::{
    FindingPort, HostManifestPort, KnowledgeEntryPort, MemoryPort, RelationPort, RuleQueryPort,
    ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult,
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

    /// The operator principal the authenticated peer maps to, as the memory
    /// port's own request carrier: the fixture authorizes the request's
    /// principal id against the operator grant.
    fn memory_principal(&self) -> MemoryPrincipal {
        serde_json::from_value(json!({
            "principal_id": format!("prn_{}", peer_id(&CLIENT_SEED)),
            "extensions": {},
        }))
        .expect("valid memory principal")
    }

    /// Dial the SDK `RemoteAdapter` over its own signed loopback session.
    ///
    /// This is the SDK's own client (the pattern the conformance loopback uses),
    /// not Core: it addresses the provider operations Core's surface does not
    /// expose on this task, against the same durable state the Core calls wrote.
    async fn sdk_session(
        &self,
    ) -> (
        Arc<RemoteAdapter>,
        tokio::task::JoinHandle<Arc<ConnectResponder>>,
    ) {
        let (addr, task) = self.serve().await;
        let (ws, _response) = tokio::time::timeout(
            BOUND,
            tokio_tungstenite::connect_async(format!("ws://{addr}")),
        )
        .await
        .expect("the localhost websocket connect is bounded")
        .expect("connect the localhost websocket");
        let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
        let dialed = connect_remote_adapter(RemoteAdapterOptions {
            transport,
            local_identity: RemoteIdentity { seed: CLIENT_SEED },
            local_manifest: manifest("r2-subjects-client"),
            remote_pubkey: pubkey(&HOST_SEED),
            allowlist: vec![peer_id(&HOST_SEED)],
            invoke_timeout_ms: Some(5_000),
            capability_token: None,
        });
        let adapter = tokio::time::timeout(BOUND, dialed)
            .await
            .expect("the signed dial is bounded")
            .expect("the signed dial succeeds");
        (adapter, task)
    }

    /// Run the provider's own retention pass beyond BOTH windows: the keyed
    /// receipt body window and the dedup tombstone window that follows it.
    ///
    /// This is the existing public operator (`FixtureMemoryProvider::sweep_retained`),
    /// given one instant past `body_expires_at + dedup_retention_seconds`, so a
    /// later identical key is free again.
    async fn sweep_beyond_retention(&self) -> RetentionSweep {
        let beyond = u64::try_from(
            IDEMPOTENCY_RETENTION_SECONDS
                .saturating_add(DEDUP_RETENTION_SECONDS)
                .saturating_add(RECEIPT_RETENTION_SECONDS)
                .saturating_add(1),
        )
        .expect("bounded retention windows");
        let now = Utc::now() + ChronoDuration::seconds(i64::try_from(beyond).expect("bounded"));
        self.inner
            .sweep_retained(&self.space_id, now)
            .await
            .expect("the provider retention pass runs")
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

    // I215: arm the legacy-read trap BEFORE the first remote operation. The
    // local Creator queue is removed from this disposable state DB for the
    // whole lifecycle below, so any legacy queue read would fail loudly instead
    // of going unnoticed; the Nexus provenance/metadata tables the capture path
    // legitimately reads (actors, worlds, bindings) stay untouched. The table
    // is restored before the final local-state assertions.
    let trap = core_db(&env).await;
    sqlx::query("ALTER TABLE memory_pending_review RENAME TO memory_pending_review_r2_trap")
        .execute(&trap)
        .await
        .expect("withhold the legacy Creator queue from the whole lifecycle");

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
    // A remote Creator miss must come from the provider alone: the local legacy
    // queue and its cross-Creator ownership re-read (the retained 403) are
    // local-only paths. With the queue withheld for the whole lifecycle, an
    // actual legacy read anywhere above — or here — fails loudly, so the
    // retained not-found answer proves the read never happens.
    let miss = core
        .delete_pending_review(&principal, "pend_legacy_foreign".to_string())
        .await
        .expect_err("the remote miss is not a dismissal");
    assert!(
        matches!(&miss, CoreError::NotFound { .. }),
        "a remote miss never becomes the legacy ownership 403 or a local storage error: {miss:?}"
    );
    sqlx::query("ALTER TABLE memory_pending_review_r2_trap RENAME TO memory_pending_review")
        .execute(&trap)
        .await
        .expect("restore the legacy queue");
    trap.close().await;
    let nexus = core_db(&env).await;
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

    // I211: the subject *read* itself, over the SDK's own signed session. The
    // durable row asserted above is the effect of `ensure_subject`; this call is
    // the `memory.subjects.get` Core's adapter issues, observed at the protocol
    // boundary, and it can fail on a wrong response or a wrong subject.
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = provider.memory_principal();
    let got = match adapter
        .get_subject(
            &memory_principal,
            dto::<MemorySubjectsGetRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("subject body"),
        SpokeResult::Reject(reject) => panic!("the ensured subject is readable: {reject:?}"),
    };
    assert_eq!(got["subject"]["subject_id"], json!(subject_id));
    assert_eq!(got["subject"]["status"], json!("active"));
    assert_eq!(
        got["subject"]["kind"]["ns"],
        json!(legacy::CHARACTER_KIND_NS)
    );
    assert_eq!(got["subject"]["kind"]["id"], json!(character_id));
    let revision = got["subject"]["revision"]
        .as_u64()
        .expect("the subject revision");

    // A subject this provider never saw answers the protocol's definite absence.
    match adapter
        .get_subject(
            &memory_principal,
            dto::<MemorySubjectsGetRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": "sbj_never_created",
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => assert_eq!(reject.code, SpokeRejectCode::NotFound),
        SpokeResult::Ok(body) => panic!("an unknown subject is not served: {body:?}"),
    }

    // Archive the provider subject through the real admin operation, then read
    // the archived state back through the same get.
    match adapter
        .fence_subject(
            &memory_principal,
            dto::<MemorySubjectsFenceRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "target": { "type": "subject" },
                "action": "archive",
                "expected_revision": revision,
                "idempotency_key": "r2-archive:character",
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the admin archive is served: {reject:?}"),
    }
    let archived = match adapter
        .get_subject(
            &memory_principal,
            dto::<MemorySubjectsGetRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("archived subject"),
        SpokeResult::Reject(reject) => panic!("the archived subject is still readable: {reject:?}"),
    };
    assert_eq!(
        archived["subject"]["status"],
        json!("archived"),
        "the archived lifecycle is what Core's subject_archived read sees"
    );

    // Core meets the same archived subject: the next binding-local capture is
    // refused by the provider lifecycle gate with the definite class, and no
    // admission or candidate fact survives it.
    let refused = capture(
        &core,
        &principal,
        &character_id,
        Some(&binding_b),
        "pend_archived",
    )
    .await
    .expect_err("an archived subject refuses a fresh capture");
    let (code, retry_class, family, _) = provider_refusal(&refused);
    assert_eq!(code, "SUBJECT_FENCED");
    assert_eq!(retry_class, "terminal");
    assert_eq!(family, MemoryProviderRefusal::Conflict);
    let nexus = core_db(&env).await;
    assert_eq!(
        scalar(&nexus, "SELECT COUNT(*) FROM remote_binding_memory_admissions").await,
        0,
        "a definite lifecycle refusal settles this attempt's admission"
    );
    nexus.close().await;
    adapter.close();
    responder_of(sdk_responder).await;

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
        vec![(
            "candidate".to_string(),
            candidate.clone(),
            "released".to_string()
        )],
        "the stale publication cannot revive a released candidate"
    );
    assert_eq!(
        admission_count(&nexus, &uncertain).await,
        1,
        "the replay retired only its own attempt; the older unknown stays"
    );
    nexus.close().await;
    assert_remote_memory_holds(&reopened, &reopened_principal, &character_id, &uncertain).await;

    // I212: the same key is FREE again after BOTH retention windows. The
    // provider's own public retention operator runs past the receipt-body
    // window and the dedup tombstone window, so the identical Core capture can
    // no longer replay the old receipt: it commits a genuinely new effect.
    let pool = provider.pool().await;
    let old_receipt = sqlx::query(
        "SELECT receipt_id FROM receipts WHERE op = 'memory.capture' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("the committed-but-unknown capture receipt")
    .get::<String, _>("receipt_id");
    pool.close().await;
    let sweep = provider.sweep_beyond_retention().await;
    assert!(
        sweep.receipt_bodies_evicted >= 1 && sweep.dedup_tombstones_evicted >= 1,
        "the operator reclaimed both windows: {sweep:?}"
    );
    let pool = provider.pool().await;
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM receipts WHERE receipt_id = ?",
            &old_receipt
        )
        .await,
        0,
        "the original receipt body is past its retention window"
    );
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM idempotency_namespace").await,
        0,
        "the dedup tombstone is past its window too, so the key is free"
    );
    pool.close().await;

    capture(
        &reopened,
        &reopened_principal,
        &character_id,
        Some(&uncertain),
        "pend_uncertain",
    )
    .await
    .expect("the identical capture executes again once the key is free");
    let pool = provider.pool().await;
    let new_receipt = sqlx::query(
        "SELECT receipt_id FROM receipts WHERE op = 'memory.capture' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("the new committed capture receipt")
    .get::<String, _>("receipt_id");
    assert_ne!(
        new_receipt, old_receipt,
        "a post-retention capture commits a genuinely different receipt"
    );
    let new_candidate = sqlx::query(
        "SELECT candidate_id FROM candidates WHERE subject_id = ? AND status = 'pending'",
    )
    .bind(&subject_id)
    .fetch_one(&pool)
    .await
    .expect("the new pending candidate")
    .get::<String, _>("candidate_id");
    assert_ne!(
        new_candidate, candidate,
        "a post-retention capture produces a genuinely different candidate"
    );
    pool.close().await;
    let nexus = core_db(&env).await;
    let mut facts = object_facts(&nexus, &uncertain).await;
    facts.sort();
    let mut expected = vec![
        (
            "candidate".to_string(),
            candidate.clone(),
            "released".to_string(),
        ),
        (
            "candidate".to_string(),
            new_candidate.clone(),
            "live".to_string(),
        ),
    ];
    expected.sort();
    assert_eq!(
        facts, expected,
        "the new receipt publishes its own candidate and leaves the old fact terminal"
    );
    assert_eq!(
        admission_count(&nexus, &uncertain).await,
        1,
        "the post-retention receipt never retires the older unknown admission"
    );
    nexus.close().await;

    // Disposing the new known dependency does not free the older unknown one.
    reopened
        .delete_character_pending_review(
            &reopened_principal,
            character_id.clone(),
            "pend_uncertain".to_string(),
        )
        .await
        .expect("dispose the post-retention candidate");
    let nexus = core_db(&env).await;
    assert!(
        object_facts(&nexus, &uncertain)
            .await
            .iter()
            .all(|(_, _, state)| state != "live"),
        "the new candidate is disposed"
    );
    assert_eq!(
        admission_count(&nexus, &uncertain).await,
        1,
        "disposal never settles an unknown admission"
    );
    nexus.close().await;
    assert_remote_memory_holds(&reopened, &reopened_principal, &character_id, &uncertain).await;

    // Caller loss mid-flight: the committed effect's response is withheld, the
    // calling task is abandoned, and the admission survives that loss.
    // A generous hold bound: the in-flight assertions below run BEFORE the
    // caller is abandoned, so the provider must stay parked across them.
    let mut hold = provider
        .faults()
        .hold_next_bounded(nexus_memory_conformance::FaultPoint::ResponseDrop, Duration::from_secs(60));
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

    // I214: the caller has not been abandoned yet. The attempt's durable
    // admission was committed before the RPC and the provider has already
    // committed the effect, so the authoritative removal must refuse right now,
    // with the binding and its admission unchanged.
    assert_remote_memory_holds(&reopened, &reopened_principal, &character_id, &abandoned).await;
    let nexus = core_db(&env).await;
    assert_eq!(
        admission_count(&nexus, &abandoned).await,
        1,
        "the still-in-flight attempt keeps its admission"
    );
    assert_eq!(
        count(
            &nexus,
            "SELECT COUNT(*) FROM actor_world_bindings WHERE binding_id = ? AND status = 'active'",
            &abandoned
        )
        .await,
        1,
        "the refused removal left the binding in place"
    );
    nexus.close().await;

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

/// Prove the provider really serves one candidate at the exact addressed id,
/// through the SDK's own signed session: the fact the fetch under test reads.
async fn assert_served_at(
    adapter: &RemoteAdapter,
    principal: &MemoryPrincipal,
    space_id: &str,
    candidate_id: &str,
) {
    match adapter
        .get_candidate(
            principal,
            dto::<MemoryCandidatesGetRequest>(json!({
                "space_id": space_id,
                "candidate_id": candidate_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => {
            let body = serde_json::to_value(&body).expect("candidate body");
            assert_eq!(body["candidate"]["candidate_id"], json!(candidate_id));
            assert_eq!(body["candidate"]["status"], json!("pending"));
        }
        SpokeResult::Reject(reject) => {
            panic!("the staged candidate is served at its addressed id: {reject:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// The fetched shaped-id identity guards
// ---------------------------------------------------------------------------

/// A candidate that really exists at the deterministic bridge id Nexus derives
/// from a retained pending id, but whose subject or retained identity is not the
/// requested one: the real public dismissal fetches it and refuses before the
/// provider CAS. The matching control goes through the same branch and the real
/// CAS consumes it.
#[tokio::test]
async fn shaped_candidate_identity_guards_precede_the_provider_cas() {
    let env = Env::boot("R2 Identity Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("principal");

    let world_a = create_world(&core, &principal, "R2 Identity A").await;
    let world_b = create_world(&core, &principal, "R2 Identity B").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "R2 Identity Ada", &world_a).await;
    let (other_character, _, _) =
        create_character(&core, &principal, "R2 Identity Bea", &world_b).await;
    let subject_id = legacy::character_subject_id(&character_id);
    let other_subject = legacy::character_subject_id(&other_character);

    // The SDK session is only the address proof: it serves the staged row at
    // exactly the id the public dismissal derives and looks up.
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = provider.memory_principal();
    let baseline = provider.pool().await;
    let dismissals_before = provider_receipts(&baseline, "memory.candidates.dismiss").await;
    baseline.close().await;

    // (a) The addressed candidate exists, but under ANOTHER Character's subject.
    let foreign_pending = "pend_guard_foreign";
    let foreign_id = legacy::character_pending_review_candidate_id(foreign_pending);
    provider
        .inner
        .stage_pending_candidate(
            &foreign_id,
            &other_subject,
            None,
            foreign_pending,
            PROMOTE_DIGEST,
        )
        .await
        .expect("stage the foreign-subject candidate");
    assert_served_at(&adapter, &memory_principal, &provider.space_id, &foreign_id).await;
    let refused = core
        .delete_character_pending_review(
            &principal,
            character_id.clone(),
            foreign_pending.to_string(),
        )
        .await
        .expect_err("another subject's candidate is not this bearer's to dismiss");
    assert!(
        matches!(&refused, CoreError::NotFound { .. }),
        "the retained foreign-subject answer: {refused:?}"
    );
    let pool = provider.pool().await;
    assert_eq!(
        provider_candidate(&pool, &foreign_id).await,
        Some(("pending".to_string(), None, 1)),
        "the fetched foreign candidate is untouched, so the CAS never ran"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.dismiss").await,
        dismissals_before,
        "a guarded refusal commits no dismissal receipt"
    );
    pool.close().await;

    // (b) The addressed candidate belongs to THIS subject, but its retained
    // pending identity is a different row: still the same pre-CAS refusal.
    let wrong_pending = "pend_guard_wrong";
    let stored_pending = "pend_guard_other";
    let wrong_id = legacy::character_pending_review_candidate_id(wrong_pending);
    provider
        .inner
        .stage_pending_candidate(
            &wrong_id,
            &subject_id,
            Some(&binding_id),
            stored_pending,
            PROMOTE_DIGEST,
        )
        .await
        .expect("stage the mismatched-identity candidate");
    assert_served_at(&adapter, &memory_principal, &provider.space_id, &wrong_id).await;
    let listed = core
        .list_character_pending_reviews(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            50,
            0,
        )
        .await
        .expect("list the binding queue");
    assert_eq!(
        listed.items.len(),
        1,
        "the staged row is a real queued candidate of this subject"
    );
    assert_eq!(
        listed.items[0].pending_id.as_str(),
        stored_pending,
        "the served row carries the retained identity it was staged with"
    );
    let refused = core
        .delete_character_pending_review(
            &principal,
            character_id.clone(),
            wrong_pending.to_string(),
        )
        .await
        .expect_err("the retained pending identity does not address this row");
    assert!(
        matches!(&refused, CoreError::NotFound { .. }),
        "the retained mismatched-identity answer: {refused:?}"
    );
    let pool = provider.pool().await;
    assert_eq!(
        provider_candidate(&pool, &wrong_id).await,
        Some(("pending".to_string(), Some(binding_id.clone()), 1)),
        "the fetched mismatched candidate is untouched, so the CAS never ran"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.dismiss").await,
        dismissals_before,
        "a guarded refusal commits no dismissal receipt"
    );
    pool.close().await;

    // (c) Positive control: the subject and the retained identity both match,
    // so the SAME lookup branch reaches the real provider CAS and consumes it.
    let ok_pending = "pend_guard_ok";
    let ok_id = legacy::character_pending_review_candidate_id(ok_pending);
    provider
        .inner
        .stage_pending_candidate(&ok_id, &subject_id, Some(&binding_id), ok_pending, PROMOTE_DIGEST)
        .await
        .expect("stage the matching candidate");
    assert_served_at(&adapter, &memory_principal, &provider.space_id, &ok_id).await;
    core.delete_character_pending_review(
        &principal,
        character_id.clone(),
        ok_pending.to_string(),
    )
    .await
    .expect("the matching row is dismissed through the shaped-id branch");
    let pool = provider.pool().await;
    assert_eq!(
        provider_candidate(&pool, &ok_id).await,
        Some(("consumed".to_string(), Some(binding_id.clone()), 2)),
        "the real provider CAS consumed the matching candidate"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.dismiss").await,
        dismissals_before + 1,
        "exactly the matching dismissal committed"
    );
    pool.close().await;
    let nexus = core_db(&env).await;
    assert_eq!(
        object_facts(&nexus, &binding_id).await,
        vec![(
            "candidate".to_string(),
            ok_id.clone(),
            "released".to_string()
        )],
        "the positive dismissal records its D18 released fact"
    );
    nexus.close().await;

    adapter.close();
    responder_of(sdk_responder).await;
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
// The admitted Host composition: the real `subject_archived`/`family` consumer
// ---------------------------------------------------------------------------

const FIXTURE_PROVIDER: &str = "r2-mind-fixture";

/// Deterministic in-memory provider adapter: no process, no model, no network.
///
/// It exists only so the production admitted Host path is runnable here (the
/// memory port itself stays the real signed provider). Its `execute` records the
/// exact prompt text the Host assembled for the Character, which is where the
/// `CoreCharacterMind` projection's provider reads surface.
#[derive(Default)]
struct MindFixtureAdapter {
    prompts: std::sync::Mutex<Vec<String>>,
}

impl MindFixtureAdapter {
    fn prompts(&self) -> Vec<String> {
        self.prompts.lock().expect("prompt log").clone()
    }
}

#[async_trait]
impl ProviderAdapter for MindFixtureAdapter {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: ProviderId::new(FIXTURE_PROVIDER),
            display_name: "R2 mind fixture".to_string(),
            protocol_kind: ProtocolKind::NativeCli,
            capabilities: CapabilityDescriptor::acp_full(),
        }
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::acp_full()
    }

    async fn probe(&self, _request: ProbeRequest) -> HostResult<ProviderHealth> {
        Ok(ProviderHealth {
            provider_id: ProviderId::new(FIXTURE_PROVIDER),
            available: true,
            latency_ms: None,
            message: Some("r2 mind fixture".to_string()),
        })
    }

    async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
        Ok(ManagedSessionHandle {
            provider_id: ProviderId::new(FIXTURE_PROVIDER),
            session_id: HostSessionId::new(),
            capabilities: CapabilityDescriptor::acp_full(),
            process_identity: None,
        })
    }

    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        op: HostOperation,
    ) -> HostResult<HostEventStream> {
        let HostOperation::Prompt { op_id, content, .. } = op else {
            return Err(HostError::internal(
                "the mind fixture executes prompt operations only",
            ));
        };
        let prompt = content
            .into_iter()
            .filter_map(|block| match block {
                HostContentBlock::Text { text } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.prompts.lock().expect("prompt log").push(prompt);
        Ok(Box::pin(futures_util::stream::iter(vec![Ok(
            HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session.session_id.clone(),
                op_id,
                reason: FinishReason::EndTurn,
            }),
        )])))
    }

    async fn cancel(&self, _session: &ManagedSessionHandle, _op: HostOperationId) -> HostResult<()> {
        Ok(())
    }

    async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
        Ok(())
    }
}

/// The Host authority's composed provider port. The Character prompt path never
/// calls it (the adapter above is the effect boundary for that), so it answers
/// honestly and is never exercised.
struct QuietPort;

#[async_trait]
impl ProviderPort for QuietPort {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        Ok(ProviderReply {
            request_id: request.request_id,
            ok: true,
            session_id: None,
            operation_id: None,
            health: None,
            error: None,
        })
    }

    async fn next(
        &self,
        operation_id: String,
        _max_events: u32,
        _max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        Ok(ProviderEventBatch {
            events: vec![],
            gap: None,
            has_more: false,
            operation_id,
        })
    }
}

/// The production admitted Host path builds the Character prompt through
/// `CoreCharacterMind`, whose `family` read calls the provider's `get_subject`
/// and, only for an active subject, lists the records. Observe that path here:
/// the same prompt carries the binding-local fact while the provider subject is
/// active and loses it once the subject is archived — through a real signed
/// session to the independent durable provider, with no new Core API.
#[tokio::test]
async fn host_mind_projection_reads_the_remote_subject() {
    let env = Env::boot("R2 Host Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("principal");

    let world = create_world(&core, &principal, "R2 Host World").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "R2 Host Ada", &world).await;

    // One binding-local fact on the real provider: the memory the projection
    // must serve into the Host prompt.
    capture(
        &core,
        &principal,
        &character_id,
        Some(&binding_id),
        "pend_host_mind",
    )
    .await
    .expect("capture");
    core.review_character_memory(
        &principal,
        character_id.clone(),
        dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_id })),
    )
    .await
    .expect("review the binding-local fact");

    // Deterministic local agent host: no process, no model, no network.
    let manager = Arc::new(HostManager::new());
    let adapter = Arc::new(MindFixtureAdapter::default());
    manager
        .register_provider(
            adapter.clone(),
            LaunchStrategy::NativeCli {
                command: "r2-mind-fixture".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
            },
        )
        .await;
    // The Host's workspace boundary must contain both the Nexus home and the
    // admitted Character creative root this open pinned, so it is the user
    // home — the root the production native boot hands the manager.
    let workspace_root = env.user_home.clone();
    let mut host_config = nexus_agent_host::config::AgentHostConfig::default();
    host_config.max_ops_per_session = 4;
    manager
        .start(HostStartConfig {
            config_path: nexus_agent_host::config::agent_host_config_path(&env.user_home),
            workspace_root: workspace_root.clone(),
            max_sessions: host_config.max_sessions,
            max_ops_per_session: host_config.max_ops_per_session,
            timeouts: host_config.timeouts.clone(),
            host_config: Some(host_config),
            admitted_catalog: None,
            probe_owner: Some(SessionOwner {
                creator_id: env.creator_id.clone(),
                workspace_root,
                orchestration_run_id: None,
            }),
        })
        .await
        .expect("the deterministic host starts");
    let handle = core
        .attach_host(manager, Arc::new(QuietPort))
        .expect("the admitted Host authority attaches");

    let session = handle
        .create_session(
            &principal,
            dto::<CreateSessionRequest>(json!({
                "provider_id": FIXTURE_PROVIDER,
                "actor_ref": { "actor_kind": "character", "character_id": character_id },
                "viewpoint": { "world_id": world, "binding_id": binding_id },
            })),
        )
        .await
        .expect("the admitted Character host session is created");

    let prompt = || {
        dto::<ExecuteOperationRequest>(json!({
            "kind": "prompt",
            "content": "What do you remember about the alliance?",
        }))
    };
    handle
        .execute(&principal, session.session_id.clone(), prompt())
        .await
        .expect("the admitted Character prompt runs");
    let prompts = adapter.prompts();
    assert_eq!(
        prompts.len(),
        1,
        "the host handed the fixture adapter the assembled prompt"
    );
    assert!(
        prompts[0].contains(PROMOTE_DIGEST),
        "the mind projection served the binding-local fact into the prompt: {}",
        prompts[0]
    );

    // Archive the provider subject through the real admin op: the projection's
    // `subject_archived`/`family` read is what turns the fact off.
    let (sdk, sdk_responder) = provider.sdk_session().await;
    let memory_principal = provider.memory_principal();
    let subject_id = legacy::character_subject_id(&character_id);
    let current = match sdk
        .get_subject(
            &memory_principal,
            dto::<MemorySubjectsGetRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("subject body"),
        SpokeResult::Reject(reject) => panic!("the subject is readable: {reject:?}"),
    };
    match sdk
        .fence_subject(
            &memory_principal,
            dto::<MemorySubjectsFenceRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "target": { "type": "subject" },
                "action": "archive",
                "expected_revision": current["subject"]["revision"],
                "idempotency_key": "r2-archive:host-mind",
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the admin archive is served: {reject:?}"),
    }
    // Read the archived lifecycle back through the same SDK get.
    let archived = match sdk
        .get_subject(
            &memory_principal,
            dto::<MemorySubjectsGetRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("archived subject"),
        SpokeResult::Reject(reject) => panic!("the archived subject is readable: {reject:?}"),
    };
    assert_eq!(
        archived["subject"]["status"],
        json!("archived"),
        "the provider subject really is archived"
    );

    // The same admitted Character prompt now fails closed: the projection meets
    // the archived subject lifecycle on the provider and Core surfaces its
    // definite refusal instead of serving stale or silently empty memory. No
    // prompt was assembled from that refused projection.
    let failed = handle
        .execute(&principal, session.session_id.clone(), prompt())
        .await
        .expect_err("the archived subject's projection is refused");
    let (code, retry_class, family, _) = provider_refusal(&failed);
    assert_eq!(code, "SUBJECT_FENCED");
    assert_eq!(retry_class, "terminal");
    assert_eq!(family, MemoryProviderRefusal::Conflict);
    assert_eq!(
        adapter.prompts().len(),
        1,
        "a refused projection hands the provider no prompt"
    );

    sdk.close();
    responder_of(sdk_responder).await;
    assert!(
        handle.close().await.expect("host close").cleanup_confirmed,
        "the host authority closes"
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
    responder_of(responder).await;
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
