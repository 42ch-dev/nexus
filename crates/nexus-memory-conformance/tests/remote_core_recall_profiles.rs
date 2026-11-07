//! R4 (v1.229 N3 Task 4): remote profile reads/writes, bounded `memory.recall`
//! and the admitted Character mind projection over the selected remote provider.
//!
//! Every scenario opens the real public `CoreService` with the selected remote
//! provider, so the path under test is the product one: the pinned
//! `MemoryProviderBinding`, the SDK `RemoteAdapter` over a signed Connect
//! session, and Nexus's own admission/refusal policy. The provider on the other
//! end is the independent, durable `FixtureMemoryProvider` sharing nothing with
//! Core's local provider, served by the SDK's `connect_responder` over a real
//! `127.0.0.1` socket with the one product WS transport on both ends. Nothing is
//! mocked: the credential file, the signed hello, the operator-provisioned
//! grant, the fixture's own space/authority and its durable profiles/records/
//! receipts are the real ones.
//!
//! What these tests prove, and what they intentionally cannot:
//!
//! - The Creator SOUL profile (`get_creator_soul`/`set_creator_soul`) runs on the
//!   remote provider through `memory.profile.get`/`memory.profile.update`. The
//!   create-only write, the revision CAS and the retained static
//!   `version_mismatch` conflict class are exercised, and a stale revision
//!   performs exactly one provider write attempt (no adoption retry: the
//!   projected-legacy adoption is local-only and cannot run here).
//! - A read-only core keeps serving the profile read while the write keeps its
//!   retained typed refusal, before any provider call.
//! - `memory.recall` has no Core product caller on this slice — the Character
//!   mind projection reads kind-filtered `memory.records.list` instead of
//!   ranking every kind through recall — so the recall operation is exercised
//!   over the SDK's own signed session against the same durable provider and the
//!   same pinned space. It is the SDK client, not a fake port. Selection modes,
//!   the item/byte budget, the overflow item and the query/snapshot-bound cursor
//!   are the provider's real behavior; a recall over a space this provider does
//!   not serve is its definite `NOT_FOUND`.
//! - The admitted Host composition (`CoreService::attach_host` +
//!   `HostHandle::create_session`/`execute`) assembles the Character prompt
//!   through the same `CoreCharacterMind` production reader, which performs
//!   `memory.profile.get` and kind-filtered `memory.records.list` on the real
//!   signed provider and folds the provider's SOUL document and promoted memory
//!   into the prompt. That is the actual Core/Host consumer of Task 3's read
//!   paths, not a private accessor or a fabricated getter.
//! - The legacy Creator queue, the legacy Character queue and the legacy
//!   Character fragment table are withheld from this disposable state DB for the
//!   exercised interval (armed after the initial Core open and restored before
//!   close), and each applicable bearer's real long-term discovery DIRECTORY
//!   (`memory_io::list_memories` calls `fs::read_dir` on it) is a pre-existing
//!   regular file, so a caller that propagates the non-`NotFound` discovery
//!   error cannot read anything there — a swallowed error or an unrelated read
//!   is not excluded. The local provider is never constructed (no store root, no
//!   journal, no provider tables), and no SOUL-narrative / soul-invalidation row
//!   is written. The scope is exactly those tables, those two discovery paths
//!   and the local provider surfaces: Nexus actor/world/binding metadata stays
//!   readable, and this is neither a whole open-to-close no-I/O proof nor a
//!   universal no-read claim about unrelated local surfaces.
//! - Refusals reach the caller as the static classified `MemoryProviderRefused`
//!   carrier: the provider's own message text and any request content never
//!   travel with it.
//!
//! `MemoryProviderBinding` is crate-private, so this crate-external test cannot
//! read the pinned `(space, authority, client identity)` tuple back. The
//! provider's own rows are the observable substitute: their `space_id`,
//! `subject_id`, revisions and receipts are written from that pinned tuple by
//! the product code.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    HostOperation, HostStartConfig, LaunchSpec, ManagedSessionHandle, OperationFinishedEvent,
    ProbeRequest, ProtocolKind, ProviderDescriptor, ProviderHealth, SessionOwner,
};
use nexus_agent_host::{
    HostError, HostFacade, HostManager, HostOperationId, HostResult, HostSessionId, LaunchStrategy,
    ProviderAdapter, ProviderId,
};
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::generated::daemon_api::agent_host::create_session_request::CreateSessionRequest;
use nexus_contracts::generated::daemon_api::agent_host::execute_operation_request::ExecuteOperationRequest;
use nexus_contracts::{
    CoreRegisterCreatorRequest, ProviderCall, ProviderEventBatch, ProviderReply,
    SetActiveWorkspaceRequest,
};
use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::memory_provider::legacy;
use nexus_core::{
    CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreResult, CoreService,
    MemoryProviderRefusal, MemoryProviderSelection, Principal, RemoteMemoryProviderSelection,
};
use nexus_memory_conformance::{FixtureMemoryProvider, PeerDirectory};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use serde_json::{json, Value};
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponder, ConnectResponderOptions,
    RemoteAdapter, RemoteAdapterOptions, RemoteIdentity, RemoteMemoryService, RemoteServePorts,
    RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, MemoryGrantsGrantRequest,
    MemoryProfileUpdateRequest, MemoryRecallRequest, MemoryRecordsCreateRequest,
    MemorySubjectsEnsureRequest, Principal as MemoryPrincipal, Relation, Rule, Scope,
    TimelineEvent,
};
use spoke_operations::{
    FindingPort, HostManifestPort, KnowledgeEntryPort, MemoryPort, RelationPort, RuleQueryPort,
    ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;
use tokio::net::TcpListener;

/// The host's Connect identity (the provider side of the signed session).
const HOST_SEED: [u8; 32] = [0x42; 32];
/// The dialing client's Connect identity; the credential file holds this seed.
const CLIENT_SEED: [u8; 32] = [0x31; 32];
/// The one capability both ends negotiate: the fixture serves no other family.
const MEMORY_CAPABILITIES: [&str; 1] = ["spoke-memory"];
const WORKSPACE: &str = "default";
const CREDENTIAL_ID: &str = "cred-r4-profiles";
const BOUND: Duration = Duration::from_secs(5);
/// Bytes of the file that occupies a bearer's long-term discovery *directory*.
///
/// `memory_io::list_memories` calls `fs::read_dir` on that directory and only
/// treats `NotFound` as honest-empty, so a regular file there makes every real
/// discovery attempt fail loudly — the trap the lifecycle below must not trip.
const LEGACY_DIR_TRAP_BYTES: &[u8] = b"r4-legacy-long-term-dir-trap";
/// The legacy memory surfaces withheld from the disposable state DB.
const LEGACY_MEMORY_TABLES: [&str; 3] = [
    "memory_pending_review",
    "character_memory_pending_review",
    "character_memory_fragments",
];
const TRAP_SUFFIX: &str = "_r4_trap";
/// The authored Character SOUL document the projection must serve.
const SOUL_BODY: &str = "## Identity\nR4 keeps the northern passes and the winter granary.";
/// The one authored edit the revision-checked profile write applies.
const EDITED_SOUL_BODY: &str = "## Identity\nR4 keeps four passes after the winter treaty.";
/// A distinctive request body that a refusal must never echo.
const REQUEST_SENTINEL_BODY: &str =
    "r4-request-secret-sentinel: the northern gate word must never travel back.";
/// The peer-controlled message/details sentinel a real refusal is rewritten with.
const PEER_SENTINEL: &str = "r4-peer-secret-sentinel";
/// A high-signal digest the deterministic classifier promotes for `brainstorm`.
const PROMOTE_DIGEST: &str =
    "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";
/// The recall corpus: every seeded record carries a known text and placement, so
/// each selection is compared against these independently captured facts.
const RECALL_ONE: &str = "The northern alliance holds three passes.";
const RECALL_TWO: &str = "The southern alliance holds a shared granary.";
const RECALL_CONTEXT: &str = "The binding-local alliance note about the eastern road.";
const RECALL_CONTEXT_ID: &str = "awb_r4_recall_context";
const RECALL_OTHER: &str = "The watchtower alliance note about the western road.";
const RECALL_OTHER_CONTEXT_ID: &str = "awb_r4_recall_other";
/// A record under a second subject: excluded from every subject-scoped recall.
const EXCLUDED_SUBJECT: &str = "sub_r4_recall_excluded";
const EXCLUDED_TEXT: &str = "The excluded alliance note under another subject.";
/// A second, differently scoped signed client (its own Ed25519 identity/hello).
const SCOPED_SEED: [u8; 32] = [0x5b; 32];
/// The fresh subject the UTF-8 byte-budget boundary reads.
const BYTE_SUBJECT: &str = "sub_r4_byte_budget";
/// The byte budget that fits the multibyte item whole and defers the long one.
const BYTE_BUDGET: u64 = 512;
/// A multibyte (UTF-8) body that fits inside `BYTE_BUDGET` whole.
const SMALL_UTF8: &str = "密码：cipher 北方的关隘在雪后仍然通行。";
/// A fresh subject whose second record discriminates serialized UTF-8 bytes
/// from Unicode scalar characters under the same budget.
const BYTE_CHAR_SUBJECT: &str = "sub_r4_byte_char_budget";
/// The fitting item of the discriminating case (its query hit ranks it first).
const FIT_UTF8: &str = "cipher 关隘。";
/// The discarding item: 200 CJK scalars (600 UTF-8 bytes), so its character
/// count is inside `BYTE_BUDGET` while its serialized bytes are far outside it.
const OVER_CHAR_UNDER_CHARS: usize = 200;
const FIXTURE_PROVIDER: &str = "r4-mind-fixture";

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
    use std::fmt::Write as _;

    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing into a String cannot fail");
    }
    text
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

    /// The Creator bearer's real long-term discovery directory: exactly what
    /// `MemoryBearerRef::Creator::long_term_memory_dir` resolves
    /// (`<nexus root>/creators/<creator>/memory/long-term`), which
    /// `memory_io::list_memories` reads with `fs::read_dir`.
    fn creator_long_term_dir(&self) -> PathBuf {
        nexus_home_layout::nexus_root_from_home(&self.user_home)
            .join("creators")
            .join(&self.creator_id)
            .join("memory")
            .join("long-term")
    }

    /// The Character bearer's real long-term discovery directory, exactly what
    /// `MemoryBearerRef::Character::long_term_memory_dir` resolves.
    fn character_long_term_dir(&self, character_id: &str) -> PathBuf {
        nexus_home_layout::character_long_term_memory_dir(
            &self.user_home,
            &self.creator_id,
            character_id,
        )
    }

    fn state_db(&self) -> PathBuf {
        nexus_home_layout::workspace_state_db_path(&self.user_home, &self.creator_id, WORKSPACE)
    }

    fn options(&self, selection: MemoryProviderSelection, access: CoreAccess) -> CoreOpenOptions {
        CoreOpenOptions {
            user_home: self.user_home.clone(),
            access,
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
    /// The second signed client's peer id and its own provisioned principal, so
    /// a scoped grant can be issued for it and its own signed session admitted.
    scoped_peer: String,
    scoped_principal: String,
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
        let scoped_peer = peer_id(&SCOPED_SEED);
        let scoped_principal = format!("prn_{scoped_peer}");
        let space_id = inner.identity().space_id.clone();
        Self {
            _home: home,
            db,
            inner,
            directory: Arc::new(
                PeerDirectory::new()
                    .with_peer(client_peer, principal_id)
                    .with_peer(scoped_peer.clone(), scoped_principal.clone()),
            ),
            space_id,
            scoped_peer,
            scoped_principal,
        }
    }

    /// Serve one signed session for an explicitly supplied memory service: the
    /// suite's test-only observation decorator, which forwards every request
    /// unchanged to this provider's real assembled service.
    async fn serve_service(
        &self,
        service: Arc<dyn RemoteMemoryService>,
    ) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        self.serve_service_for(service, peer_id(&CLIENT_SEED), pubkey(&CLIENT_SEED))
            .await
    }

    async fn serve_service_for(
        &self,
        service: Arc<dyn RemoteMemoryService>,
        client_peer: String,
        client_pubkey: [u8; 32],
    ) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        start_listener(
            service,
            manifest(self.inner.identity().authority_host_id.as_str()),
            client_peer,
            client_pubkey,
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
        self.open_core_as(env, CoreAccess::EngineOwner).await
    }

    /// [`Self::open_core`] at an explicit core access level: the read-only open
    /// serves the same pinned provider, so its writer refusal is the real
    /// `memory_writer_binding` gate rather than a missing provider.
    async fn open_core_as(
        &self,
        env: &Env,
        access: CoreAccess,
    ) -> (CoreService, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        self.open_core_serving(env, self.inner.server(self.directory.admission()), access)
            .await
    }

    /// [`Self::open_core`] over an explicitly supplied memory service (the
    /// test-only observation decorator).
    async fn open_core_serving(
        &self,
        env: &Env,
        service: Arc<dyn RemoteMemoryService>,
        access: CoreAccess,
    ) -> (CoreService, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        let (addr, task) = self.serve_service(service).await;
        let core = CoreService::open(env.options(self.selection(addr), access))
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
    fn memory_principal() -> MemoryPrincipal {
        serde_json::from_value(json!({
            "principal_id": format!("prn_{}", peer_id(&CLIENT_SEED)),
            "extensions": {},
        }))
        .expect("valid memory principal")
    }

    /// The second signed client's principal, as its own request carrier.
    fn scoped_memory_principal(&self) -> MemoryPrincipal {
        serde_json::from_value(json!({
            "principal_id": self.scoped_principal,
            "extensions": {},
        }))
        .expect("valid scoped memory principal")
    }

    /// Dial the SDK `RemoteAdapter` over its own signed loopback session.
    ///
    /// This is the SDK's own client (the pattern the conformance loopback uses),
    /// not Core: it addresses the provider operation Core's surface does not
    /// expose on this task — `memory.recall` — and provisions provider objects
    /// for the projection stage. It is the SDK client, never a fake port.
    async fn sdk_session(
        &self,
    ) -> (
        Arc<RemoteAdapter>,
        tokio::task::JoinHandle<Arc<ConnectResponder>>,
    ) {
        self.sdk_session_as(CLIENT_SEED, "r4-profiles-client").await
    }

    /// A signed SDK session for `seed`'s own identity, on a listener that admits
    /// exactly that peer: the second client's authority is its authenticated
    /// hello, never a request field.
    async fn sdk_session_as(
        &self,
        seed: [u8; 32],
        local_manifest_id: &str,
    ) -> (
        Arc<RemoteAdapter>,
        tokio::task::JoinHandle<Arc<ConnectResponder>>,
    ) {
        let (addr, task) = self
            .serve_service_for(
                self.inner.server(self.directory.admission()),
                peer_id(&seed),
                pubkey(&seed),
            )
            .await;
        let adapter = self.dial_as(addr, seed, local_manifest_id).await;
        (adapter, task)
    }

    /// Dial one real signed SDK session to `addr` as `seed`'s identity.
    async fn dial_as(
        &self,
        addr: SocketAddr,
        seed: [u8; 32],
        local_manifest_id: &str,
    ) -> Arc<RemoteAdapter> {
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
            local_identity: RemoteIdentity { seed },
            local_manifest: manifest(local_manifest_id),
            remote_pubkey: pubkey(&HOST_SEED),
            allowlist: vec![peer_id(&HOST_SEED)],
            invoke_timeout_ms: Some(5_000),
            capability_token: None,
        });
        tokio::time::timeout(BOUND, dialed)
            .await
            .expect("the signed dial is bounded")
            .expect("the signed dial succeeds")
    }
}

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid request DTO")
}

// ---------------------------------------------------------------------------
// The observation decorator over the real assembled service
// ---------------------------------------------------------------------------

/// Test-only decorator over the REAL fixture memory service.
///
/// It forwards every request unchanged to the assembled fixture service and
/// only counts the operations under test: `memory.profile.update`, and the
/// subject-scoped `memory.profile.get` / `memory.records.list` the Character
/// mind projection must perform. The counters are operative evidence that the
/// product code really made those calls (a bypass leaves them at zero); no
/// request is altered.
///
/// While a sentinel is armed it rewrites only the peer-controlled fields of a
/// REAL refusal — its message and its `details` — exactly as an untrusted peer
/// could. The refusal code, its classification and any provider effect stay the
/// provider's own; the sentinel exists to prove Nexus never copies peer text.
struct ServiceProbe {
    inner: Arc<dyn RemoteMemoryService>,
    watched_subject: Mutex<Option<String>>,
    refusal_sentinel: Mutex<Option<String>>,
    profile_updates: AtomicUsize,
    profile_gets: AtomicUsize,
    record_lists: AtomicUsize,
}

impl ServiceProbe {
    fn over(inner: Arc<dyn RemoteMemoryService>) -> Self {
        Self {
            inner,
            watched_subject: Mutex::new(None),
            refusal_sentinel: Mutex::new(None),
            profile_updates: AtomicUsize::new(0),
            profile_gets: AtomicUsize::new(0),
            record_lists: AtomicUsize::new(0),
        }
    }

    /// Rewrite the next real refusal's peer-controlled message/details with
    /// this sentinel; `None` disarms.
    fn arm_refusal_sentinel(&self, sentinel: Option<&str>) {
        *self.refusal_sentinel.lock().expect("refusal sentinel") = sentinel.map(str::to_string);
    }

    /// Count only reads addressed to this subject.
    fn watch(&self, subject_id: &str) {
        *self.watched_subject.lock().expect("watched subject") = Some(subject_id.to_string());
    }

    fn profile_updates(&self) -> usize {
        self.profile_updates.load(Ordering::SeqCst)
    }

    fn profile_gets(&self) -> usize {
        self.profile_gets.load(Ordering::SeqCst)
    }

    fn record_lists(&self) -> usize {
        self.record_lists.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl RemoteMemoryService for ServiceProbe {
    async fn invoke(&self, peer_id: &str, op: &str, payload: Value) -> SpokeResult<Value> {
        let watched = self
            .watched_subject
            .lock()
            .expect("watched subject")
            .clone();
        let addressed = watched.is_some()
            && watched.as_deref() == payload.get("subject_id").and_then(Value::as_str);
        match op {
            "memory.profile.update" => {
                self.profile_updates.fetch_add(1, Ordering::SeqCst);
            }
            "memory.profile.get" if addressed => {
                self.profile_gets.fetch_add(1, Ordering::SeqCst);
            }
            "memory.records.list" if addressed => {
                self.record_lists.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
        let result = self.inner.invoke(peer_id, op, payload).await;
        let result = match result {
            SpokeResult::Reject(mut reject) => {
                let sentinel = self
                    .refusal_sentinel
                    .lock()
                    .expect("refusal sentinel")
                    .clone();
                if let Some(sentinel) = sentinel {
                    reject.message = format!("{sentinel}: {}", reject.message);
                    reject
                        .details
                        .get_or_insert_with(serde_json::Map::new)
                        .insert("peer_note".to_owned(), json!(sentinel));
                }
                SpokeResult::Reject(reject)
            }
            ok @ SpokeResult::Ok(_) => ok,
        };
        result
    }
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

// ---------------------------------------------------------------------------
// Legacy no-read traps
// ---------------------------------------------------------------------------

/// Withhold every legacy memory surface for the exercised interval (armed
/// after the initial Core open and restored before close) and occupy the real
/// long-term discovery paths with a pre-existing regular file.
///
/// Returns the pool holding the renamed tables and the trapped paths; each test
/// restores the tables and re-checks the traps at the end.
async fn arm_legacy_traps(env: &Env, character_id: Option<&str>) -> (SqlitePool, Vec<PathBuf>) {
    let trap = core_db(env).await;
    for table in LEGACY_MEMORY_TABLES {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE {table} RENAME TO {table}{TRAP_SUFFIX}"
        )))
        .execute(&trap)
        .await
        .expect("withhold a legacy memory table for the exercised interval");
    }
    let mut dirs = vec![env.creator_long_term_dir()];
    if let Some(character_id) = character_id {
        dirs.push(env.character_long_term_dir(character_id));
    }
    for dir in &dirs {
        fs::create_dir_all(dir.parent().expect("bearer memory dir")).expect("bearer memory dir");
        fs::write(dir, LEGACY_DIR_TRAP_BYTES).expect("the long-term discovery dir becomes a file");
    }
    (trap, dirs)
}

async fn restore_legacy_tables(trap: &SqlitePool) {
    for table in LEGACY_MEMORY_TABLES {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE {table}{TRAP_SUFFIX} RENAME TO {table}"
        )))
        .execute(trap)
        .await
        .expect("restore a legacy memory table");
    }
    trap.close().await;
}

/// Read the traps back after the lifecycle: each path is still the regular file
/// it was, and the real discovery call (`fs::read_dir`, exactly what
/// `list_memories` performs) still fails with a non-`NotFound` error. A caller
/// that propagates that error cannot have read the directory; a swallowed error
/// or an unrelated file read is not excluded by this check.
fn assert_legacy_dir_traps(dirs: &[PathBuf]) {
    for dir in dirs {
        assert!(
            dir.is_file(),
            "the pre-existing long-term discovery path stays a file trap: {}",
            dir.display()
        );
        assert_eq!(
            fs::read(dir).expect("trap bytes"),
            LEGACY_DIR_TRAP_BYTES,
            "the file trap is untouched"
        );
        let discovery = fs::read_dir(dir);
        assert!(
            matches!(&discovery, Err(error) if error.kind() != std::io::ErrorKind::NotFound),
            "the real list_memories discovery still fails on this path: {discovery:?}"
        );
    }
}

/// The local provider, the local journals and the SOUL-narrative cache stayed
/// untouched: no legacy row, no local provider row, no local provider file
/// root (which is also where the local file journal lives) and no SOUL-narrative
/// / soul-invalidation row.
async fn assert_no_local_memory(env: &Env) {
    let nexus = core_db(env).await;
    for (table, why) in [
        ("memory_pending_review", "the legacy Creator queue"),
        (
            "character_memory_pending_review",
            "the legacy Character queue",
        ),
        (
            "character_memory_fragments",
            "the legacy Character fragments",
        ),
        ("memory_records", "the local provider's own record table"),
        ("memory_receipts", "the local provider's own receipt table"),
        ("memory_subjects", "the local provider's own subject table"),
        ("memory_soul_narratives", "the local SOUL narrative cache"),
        (
            "character_soul_narratives",
            "the local Character SOUL-narrative cache",
        ),
    ] {
        assert_eq!(
            scalar(&nexus, &format!("SELECT COUNT(*) FROM {table}")).await,
            0,
            "{why} stays empty"
        );
    }
    nexus.close().await;
    assert!(
        !env.local_provider_store().exists(),
        "a remote binding never constructs the local provider (or its file journal)"
    );
}

// ---------------------------------------------------------------------------
// Provider-side observation helpers
// ---------------------------------------------------------------------------

/// The provider's stored `personality` document for one subject:
/// `(space_id, body, revision)`.
async fn provider_personality(
    pool: &SqlitePool,
    subject_id: &str,
) -> Option<(String, String, i64)> {
    let row = sqlx::query(
        "SELECT space_id, document_json, revision FROM profiles \
         WHERE subject_id = ? AND profile_kind = 'personality'",
    )
    .bind(subject_id)
    .fetch_optional(pool)
    .await
    .expect("profile row")?;
    let document: Value = serde_json::from_str(&row.get::<String, _>("document_json"))
        .expect("stored profile document");
    Some((
        row.get::<String, _>("space_id"),
        document["body"].as_str().unwrap_or_default().to_string(),
        row.get::<i64, _>("revision"),
    ))
}

/// The provider's stored records for one subject as
/// `(kind, visibility, context_id)`.
async fn provider_records(
    pool: &SqlitePool,
    subject_id: &str,
) -> Vec<(String, String, Option<String>)> {
    sqlx::query(
        "SELECT kind, scope_visibility, scope_context_id FROM records \
         WHERE subject_id = ? ORDER BY record_id",
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await
    .expect("record rows")
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("kind"),
            row.get::<String, _>("scope_visibility"),
            row.get::<Option<String>, _>("scope_context_id"),
        )
    })
    .collect()
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
    core.create_world(principal, dto(json!({ "title": title })))
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

async fn capture(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
    pending_id: &str,
) -> CoreResult<()> {
    core.capture_character_pending_review(
        principal,
        character_id.to_string(),
        dto::<CaptureCharacterPendingReviewRequest>(json!({
            "pending_id": pending_id,
            "session_id": format!("sess_{pending_id}"),
            "task_kind": "brainstorm",
            "raw_digest": PROMOTE_DIGEST,
            "created_at": "2026-01-01T00:00:01Z",
            "binding_id": binding_id,
        })),
    )
    .await
    .map(|_| ())
}

/// The classified refusal every remote profile/recall call carries, without the
/// peer's message: `(code, retry_class, family, wire_code, conflict_code)`.
fn classified_refusal(
    error: &CoreError,
) -> (
    &str,
    &str,
    MemoryProviderRefusal,
    Option<&str>,
    Option<&str>,
) {
    match error {
        CoreError::MemoryProviderRefused {
            code,
            retry_class,
            family,
            wire_code,
            conflict_code,
        } => (
            code,
            retry_class,
            *family,
            wire_code.as_deref(),
            conflict_code.as_deref(),
        ),
        other => panic!("expected a classified provider refusal, got {other:?}"),
    }
}

/// One record's `(id, placement, exact text)` as the wire renders it.
fn record_triple(record: &Value) -> (String, Value, String) {
    (
        record["record_id"].as_str().expect("record id").to_string(),
        record["scope"].clone(),
        record["content"]["text"]
            .as_str()
            .expect("record text")
            .to_string(),
    )
}

/// The exact triples a recall response served, sorted by id.
fn response_triples(response: &Value) -> Vec<(String, Value, String)> {
    let mut triples: Vec<(String, Value, String)> = response["records"]
        .as_array()
        .expect("recall records")
        .iter()
        .map(record_triple)
        .collect();
    triples.sort_by(|left, right| left.0.cmp(&right.0));
    triples
}

/// Sort independently captured triples by id for set comparison.
fn sorted_triples(mut triples: Vec<(String, Value, String)>) -> Vec<(String, Value, String)> {
    triples.sort_by(|left, right| left.0.cmp(&right.0));
    triples
}

/// One served recall body as JSON.
async fn recall_ok(
    adapter: &RemoteAdapter,
    principal: &MemoryPrincipal,
    request: MemoryRecallRequest,
) -> Value {
    match adapter.recall(principal, request).await {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("recall body"),
        SpokeResult::Reject(reject) => panic!("the recall is served: {reject:?}"),
    }
}

/// One recall refusal.
async fn recall_reject(
    adapter: &RemoteAdapter,
    principal: &MemoryPrincipal,
    request: MemoryRecallRequest,
) -> SpokeReject {
    match adapter.recall(principal, request).await {
        SpokeResult::Ok(body) => panic!("the recall is refused: {body:?}"),
        SpokeResult::Reject(reject) => reject,
    }
}

// ---------------------------------------------------------------------------
// Profile get/update on the remote provider
// ---------------------------------------------------------------------------

#[tokio::test]
async fn creator_soul_profile_reads_and_writes_run_on_the_remote_provider() {
    let env = Env::boot("R4 Profile Owner").await;
    let provider = Provider::open().await;
    let probe = Arc::new(ServiceProbe::over(
        provider.inner.server(provider.directory.admission()),
    ));
    let (core, responder) = provider
        .open_core_serving(&env, probe.clone(), CoreAccess::EngineOwner)
        .await;
    let principal = core.active_principal().await.expect("active principal");
    let subject_id = legacy::creator_subject_id(&env.creator_id);

    // The traps are armed after the initial Core open and stay armed for the
    // exercised interval below.
    let (trap, dirs) = arm_legacy_traps(&env, None).await;

    // GET: an absent authored document is the honest-empty answer, served by the
    // provider's `memory.profile.get` in the pinned space.
    assert!(
        core.get_creator_soul(&principal)
            .await
            .expect("the absent profile read is answered")
            .is_none(),
        "no authored SOUL document yet"
    );

    // CREATE: the create-only profile write (`expected_revision: null`).
    core.set_creator_soul(&principal, SOUL_BODY, None)
        .await
        .expect("create the Creator SOUL document");
    let soul = core
        .get_creator_soul(&principal)
        .await
        .expect("read the Creator SOUL document")
        .expect("the created document is served");
    assert_eq!(soul.document.body, SOUL_BODY);
    assert_eq!(soul.revision.get(), 1);
    assert_eq!(
        probe.profile_updates(),
        1,
        "one profile write reached the provider"
    );

    // The durable effect is the provider's own row in the pinned space.
    let pool = provider.pool().await;
    let (space_id, body, revision) = provider_personality(&pool, &subject_id)
        .await
        .expect("the provider stores the personality document");
    assert_eq!(space_id, provider.space_id, "the pinned provider space");
    assert_eq!(body, SOUL_BODY);
    assert_eq!(revision, 1);
    pool.close().await;

    // A caller that read another revision keeps the retained conflict class and
    // writes nothing. The refusal is the provider's real one; only its
    // peer-controlled message/details are rewritten an untrusted peer could do
    // the same. Neither the distinctive request body nor the peer text may
    // travel back in Display OR Debug.
    probe.arm_refusal_sentinel(Some(PEER_SENTINEL));
    let updates_before_stale = probe.profile_updates();
    let stale = core
        .set_creator_soul(&principal, REQUEST_SENTINEL_BODY, Some(5))
        .await
        .expect_err("the stale revision is refused");
    probe.arm_refusal_sentinel(None);
    let (refusal_code, retry_class, family, wire_code, conflict_code) = classified_refusal(&stale);
    assert_eq!(
        (refusal_code, retry_class),
        ("REVISION_CONFLICT", "terminal")
    );
    assert_eq!(family, MemoryProviderRefusal::Conflict);
    assert_eq!(wire_code, None);
    assert_eq!(conflict_code, Some("version_mismatch"));
    let shown = format!("{stale} | {stale:?}");
    assert!(
        !shown.contains(REQUEST_SENTINEL_BODY),
        "the request content never travels with a refusal: {shown}"
    );
    assert!(
        !shown.contains(PEER_SENTINEL),
        "peer-controlled message/details text never travels with a refusal: {shown}"
    );
    assert_eq!(
        probe.profile_updates(),
        updates_before_stale + 1,
        "the refused CAS is the only additional provider write attempt"
    );
    let pool = provider.pool().await;
    let (_, unchanged, revision) = provider_personality(&pool, &subject_id)
        .await
        .expect("the provider still stores the document");
    assert_eq!(unchanged, SOUL_BODY, "a refused CAS writes nothing");
    assert_eq!(revision, 1);
    assert_eq!(
        provider_receipts(&pool, "memory.profile.update").await,
        1,
        "a refused CAS commits no receipt"
    );
    pool.close().await;

    // The revision-checked write on the served revision commits.
    core.set_creator_soul(&principal, EDITED_SOUL_BODY, Some(1))
        .await
        .expect("replace the Creator SOUL document at its revision");
    let soul = core
        .get_creator_soul(&principal)
        .await
        .expect("read the edited Creator SOUL document")
        .expect("the edited document is served");
    assert_eq!(soul.document.body, EDITED_SOUL_BODY);
    assert_eq!(soul.revision.get(), 2);
    assert_eq!(probe.profile_updates(), 3, "the edit reached the provider");
    let pool = provider.pool().await;
    let (_, body, revision) = provider_personality(&pool, &subject_id)
        .await
        .expect("the provider stores the edited document");
    assert_eq!(body, EDITED_SOUL_BODY);
    assert_eq!(revision, 2);
    pool.close().await;

    // A read-only core keeps serving the profile read and refuses the write
    // before any provider call. Its own signed connection is served through the
    // SAME observation decorator, so the counter around the refused call proves
    // that the actual read-only session sent nothing.
    let (read_only, ro_responder) = provider
        .open_core_serving(&env, probe.clone(), CoreAccess::ReadOnly)
        .await;
    let ro_principal = read_only
        .active_principal()
        .await
        .expect("active principal");
    let served = read_only
        .get_creator_soul(&ro_principal)
        .await
        .expect("a read-only core serves the profile read")
        .expect("the authored document is served");
    assert_eq!(served.document.body, EDITED_SOUL_BODY);
    let updates_before_readonly = probe.profile_updates();
    let refused = read_only
        .set_creator_soul(&ro_principal, SOUL_BODY, None)
        .await
        .expect_err("a read-only core refuses the profile write");
    assert!(
        matches!(&refused, CoreError::Forbidden { resource } if resource == "memory: read-only core access"),
        "got {refused:?}"
    );
    assert_eq!(
        probe.profile_updates(),
        updates_before_readonly,
        "the actual read-only session sent no profile write before its typed refusal"
    );
    let pool = provider.pool().await;
    let (_, unchanged, revision) = provider_personality(&pool, &subject_id)
        .await
        .expect("the provider still stores the edited document");
    assert_eq!(
        unchanged, EDITED_SOUL_BODY,
        "the read-only refusal wrote nothing"
    );
    assert_eq!(revision, 2);
    assert_eq!(
        provider_receipts(&pool, "memory.profile.update").await,
        2,
        "the read-only refusal committed no receipt"
    );
    pool.close().await;

    restore_legacy_tables(&trap).await;
    assert_no_local_memory(&env).await;
    assert_legacy_dir_traps(&dirs);

    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str()),
        "every effect above travelled the signed session of the loaded credential"
    );
    let ro_host = responder_of(ro_responder).await;
    assert_eq!(
        ro_host.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str()),
        "the read-only connection is the decorated signed session, not another writer"
    );
    assert!(
        read_only.close().await.expect("close").cleanup_confirmed,
        "the read-only open releases its own admission"
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
}

// ---------------------------------------------------------------------------
// Bounded recall selection, budget and cursor (SDK-only proof)
// ---------------------------------------------------------------------------

/// `memory.recall` has no Core product caller on this slice: the Character mind
/// projection reads kind-filtered `memory.records.list` instead of ranking every
/// kind through recall. The operation is therefore exercised over the SDK's own
/// signed session against the same durable provider and the same pinned space
/// the product's profile/record calls use — never through an invented Core
/// recall API and never through a fake port.
#[tokio::test]
async fn sdk_recall_selects_budgets_pages_and_cursors_on_the_durable_provider() {
    let env = Env::boot("R4 Recall Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");
    let subject_id = legacy::creator_subject_id(&env.creator_id);

    let (trap, dirs) = arm_legacy_traps(&env, None).await;

    // Two shared records through the real product path (which also ensures the
    // provider subject), then one binding-local record through the SDK session.
    assert!(core
        .create_creator_memory(&principal, "r4-recall-one", "story_summary", RECALL_ONE)
        .await
        .expect("create the first shared memory"));
    assert!(core
        .create_creator_memory(&principal, "r4-recall-two", "story_summary", RECALL_TWO)
        .await
        .expect("create the second shared memory"));
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = Provider::memory_principal();

    // Independently captured facts for every seeded record: id, placement and
    // exact text. Recall is compared against THESE, never against another
    // recall response.
    let shared_one = core
        .get_creator_memory(&principal, "r4-recall-one")
        .await
        .expect("read the first shared memory")
        .expect("the first shared memory exists");
    let shared_two = core
        .get_creator_memory(&principal, "r4-recall-two")
        .await
        .expect("read the second shared memory")
        .expect("the second shared memory exists");
    let shared_one = record_triple(&serde_json::to_value(shared_one).expect("record json"));
    let shared_two = record_triple(&serde_json::to_value(shared_two).expect("record json"));
    assert_eq!(shared_one.2, RECALL_ONE);
    assert_eq!(shared_two.2, RECALL_TWO);
    assert_eq!(shared_one.1, json!({ "visibility": "shared" }));
    assert_eq!(shared_two.1, json!({ "visibility": "shared" }));

    let context_record = match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "kind": "fact",
                "content": { "text": RECALL_CONTEXT },
                "scope": { "visibility": "context", "context_id": RECALL_CONTEXT_ID },
                "idempotency_key": "r4-recall-context",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("create body"),
        SpokeResult::Reject(reject) => panic!("the binding-local record is created: {reject:?}"),
    };
    let context_id = context_record["result"]["record_id"]
        .as_str()
        .expect("the provider-assigned context record id")
        .to_string();
    let context_triple = (
        context_id.clone(),
        json!({ "visibility": "context", "context_id": RECALL_CONTEXT_ID }),
        RECALL_CONTEXT.to_string(),
    );

    // A second, differently placed record under the same subject: admitted by
    // `subject_all`, excluded by the selected-context view.
    let other_record = match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "kind": "fact",
                "content": { "text": RECALL_OTHER },
                "scope": { "visibility": "context", "context_id": RECALL_OTHER_CONTEXT_ID },
                "idempotency_key": "r4-recall-other-context",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("other create body"),
        SpokeResult::Reject(reject) => panic!("the other-context record is created: {reject:?}"),
    };
    let other_id = other_record["result"]["record_id"]
        .as_str()
        .expect("the provider-assigned other-context record id")
        .to_string();
    let other_triple = (
        other_id,
        json!({ "visibility": "context", "context_id": RECALL_OTHER_CONTEXT_ID }),
        RECALL_OTHER.to_string(),
    );

    // A record under a DIFFERENT subject: excluded from every subject-scoped
    // selection, so a subject mix-up cannot pass the set comparison.
    match adapter
        .ensure_subject(
            &memory_principal,
            dto::<MemorySubjectsEnsureRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": EXCLUDED_SUBJECT,
                "kind": { "ns": "r4", "id": "excluded" },
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the excluded subject is ensured: {reject:?}"),
    }
    let excluded_record = match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": EXCLUDED_SUBJECT,
                "kind": "fact",
                "content": { "text": EXCLUDED_TEXT },
                "scope": { "visibility": "shared" },
                "idempotency_key": "r4-recall-excluded",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("excluded create body"),
        SpokeResult::Reject(reject) => panic!("the excluded-subject record is created: {reject:?}"),
    };
    let excluded_id = excluded_record["result"]["record_id"]
        .as_str()
        .expect("the provider-assigned excluded-subject record id")
        .to_string();
    let excluded_triple = (
        excluded_id,
        json!({ "visibility": "shared" }),
        EXCLUDED_TEXT.to_string(),
    );

    let shared_records = [shared_one.clone(), shared_two.clone()];
    let subject_records = [
        shared_one.clone(),
        shared_two.clone(),
        context_triple.clone(),
        other_triple.clone(),
    ];

    let recall = |selection: Value, budget: Value, query: Option<&str>, cursor: Option<&str>| {
        let mut request = json!({
            "space_id": provider.space_id,
            "subject_id": subject_id,
            "selection": selection,
            "budget": budget,
        });
        if let Some(query) = query {
            request["query"] = json!(query);
        }
        if let Some(cursor) = cursor {
            request["cursor"] = json!(cursor);
        }
        dto::<MemoryRecallRequest>(request)
    };
    let selected = |selection: Value| {
        recall(
            selection,
            json!({ "max_items": 10 }),
            Some("alliance"),
            None,
        )
    };

    // SELECTION: every selection's exact (id, placement, text) triples must equal
    // the independently captured facts; missing, substituted or extra records
    // fail even when the count matches.
    let all = recall_ok(
        &adapter,
        &memory_principal,
        selected(json!({ "mode": "subject_all" })),
    )
    .await;
    assert_eq!(all["total_authorized"], json!(4));
    assert_eq!(
        response_triples(&all),
        sorted_triples(subject_records.to_vec()),
        "subject_all serves exactly the independently captured subject records"
    );
    assert!(
        all["records"]
            .as_array()
            .expect("recall records")
            .iter()
            .all(|record| record["space_id"] == json!(provider.space_id)),
        "every recalled record belongs to the pinned provider space"
    );
    assert!(
        !response_triples(&all).contains(&excluded_triple),
        "the other subject's record is never served by a subject-scoped selection"
    );

    let shared_only = recall_ok(
        &adapter,
        &memory_principal,
        selected(json!({ "mode": "shared_only" })),
    )
    .await;
    assert_eq!(shared_only["total_authorized"], json!(2));
    assert_eq!(
        response_triples(&shared_only),
        sorted_triples(shared_records.to_vec()),
        "shared_only excludes the context-placed records"
    );

    let context_only = recall_ok(
        &adapter,
        &memory_principal,
        selected(json!({
            "mode": "selected_contexts",
            "context_ids": [RECALL_CONTEXT_ID],
            "include_shared": false,
        })),
    )
    .await;
    assert_eq!(context_only["total_authorized"], json!(1));
    assert_eq!(
        response_triples(&context_only),
        sorted_triples(vec![context_triple.clone()]),
        "the selected context admits exactly its own record, not the other context"
    );

    let context_and_shared = recall_ok(
        &adapter,
        &memory_principal,
        selected(json!({
            "mode": "selected_contexts",
            "context_ids": [RECALL_CONTEXT_ID],
            "include_shared": true,
        })),
    )
    .await;
    assert_eq!(context_and_shared["total_authorized"], json!(3));
    assert_eq!(
        response_triples(&context_and_shared),
        sorted_triples(vec![
            shared_one.clone(),
            shared_two.clone(),
            context_triple.clone(),
        ]),
        "the combined selection is exactly shared plus the selected context"
    );

    // BUDGET + CURSOR: one item per page over the four subject records until the
    // authorized set is exhausted; the concatenation must equal the independently
    // captured subject triples, not another recall response.
    let page_size = json!({ "max_items": 1 });
    let mut seen: Vec<(String, Value, String)> = Vec::new();
    let mut cursor: Option<String> = None;
    for page_index in 0..4 {
        let page = recall_ok(
            &adapter,
            &memory_principal,
            recall(
                json!({ "mode": "subject_all" }),
                page_size.clone(),
                Some("alliance"),
                cursor.as_deref(),
            ),
        )
        .await;
        assert_eq!(page["total_authorized"], json!(4));
        let records = page["records"].as_array().expect("page records");
        assert_eq!(records.len(), 1, "the item budget is a hard cap");
        seen.push(record_triple(&records[0]));
        let next = page["next_cursor"].as_str().map(str::to_string);
        if page_index == 3 {
            assert_eq!(
                page["truncated"],
                json!(false),
                "the last page ends the set"
            );
            assert_eq!(next, None, "the last page carries no cursor");
        } else {
            assert_eq!(page["truncated"], json!(true));
            cursor = Some(next.expect("an unfinished page carries a cursor"));
        }
    }
    assert_eq!(
        sorted_triples(seen),
        sorted_triples(subject_records.to_vec()),
        "the pages partition exactly the independently captured subject set"
    );

    // BYTE BUDGET: a fresh subject with exactly two records. The multibyte item
    // carries the query hit and fits whole; the long item is deferred, then
    // surfaced whole as the named overflow on the resumed page. Both bodies are
    // compared to their independently seeded originals, so a truncated or empty
    // content cannot pass.
    match adapter
        .ensure_subject(
            &memory_principal,
            dto::<MemorySubjectsEnsureRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": BYTE_SUBJECT,
                "kind": { "ns": "r4", "id": "bytes" },
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the byte-budget subject is ensured: {reject:?}"),
    }
    let create_byte_record = |subject: String, text: String, key: &str| {
        let adapter = Arc::clone(&adapter);
        let principal = memory_principal.clone();
        let space_id = provider.space_id.clone();
        let key = key.to_string();
        async move {
            match adapter
                .create_record(
                    &principal,
                    dto::<MemoryRecordsCreateRequest>(json!({
                        "space_id": space_id,
                        "subject_id": subject,
                        "kind": "fact",
                        "content": { "text": text },
                        "scope": { "visibility": "shared" },
                        "idempotency_key": key,
                    })),
                )
                .await
            {
                SpokeResult::Ok(body) => serde_json::to_value(&body).expect("byte create body"),
                SpokeResult::Reject(reject) => panic!("the byte record is created: {reject:?}"),
            }
        }
    };
    let small = create_byte_record(
        BYTE_SUBJECT.to_string(),
        SMALL_UTF8.to_string(),
        "r4-byte-small",
    )
    .await;
    let small_id = small["result"]["record_id"]
        .as_str()
        .expect("the small record id")
        .to_string();
    // A long multibyte body far beyond the byte budget.
    let large_text = format!("档案：{}", "北方的关隘在雪后仍然通行，".repeat(120));
    assert!(
        large_text.len() > usize::try_from(BYTE_BUDGET).expect("the budget fits usize"),
        "the long body exceeds the byte budget"
    );
    let large = create_byte_record(
        BYTE_SUBJECT.to_string(),
        large_text.clone(),
        "r4-byte-large",
    )
    .await;
    let large_id = large["result"]["record_id"]
        .as_str()
        .expect("the large record id")
        .to_string();

    let byte_recall = |cursor: Option<&str>| {
        let mut request = json!({
            "space_id": provider.space_id,
            "subject_id": BYTE_SUBJECT,
            "selection": { "mode": "shared_only" },
            "budget": { "max_items": 10, "max_bytes": BYTE_BUDGET },
            "query": "cipher",
        });
        if let Some(cursor) = cursor {
            request["cursor"] = json!(cursor);
        }
        dto::<MemoryRecallRequest>(request)
    };
    let first = recall_ok(&adapter, &memory_principal, byte_recall(None)).await;
    let first_records = first["records"].as_array().expect("first byte page");
    assert_eq!(first["total_authorized"], json!(2));
    assert_eq!(first_records.len(), 1, "the fitting multibyte item is kept");
    assert_eq!(first_records[0]["record_id"], json!(small_id));
    assert_eq!(
        first_records[0]["content"]["text"],
        json!(SMALL_UTF8),
        "the fitting multibyte item is served byte-for-byte"
    );
    assert_eq!(
        first["overflow_record_id"],
        Value::Null,
        "the deferred item is not named while the page still has content"
    );
    assert_eq!(first["truncated"], json!(true));
    let byte_cursor = first["next_cursor"]
        .as_str()
        .expect("the fitting page carries a cursor")
        .to_string();

    let second_page = recall_ok(&adapter, &memory_principal, byte_recall(Some(&byte_cursor))).await;
    let second_records = second_page["records"].as_array().expect("second byte page");
    assert_eq!(
        second_records.len(),
        1,
        "the deferred item is surfaced whole"
    );
    assert_eq!(second_records[0]["record_id"], json!(large_id));
    assert_eq!(
        second_records[0]["content"]["text"],
        json!(large_text),
        "the deferred item's full content is intact, never truncated"
    );
    assert_eq!(
        second_page["overflow_record_id"],
        json!(large_id),
        "the oversized first item on the resumed page is named"
    );
    assert_eq!(second_page["truncated"], json!(false));
    assert_eq!(second_page["next_cursor"], Value::Null);

    // BYTE vs CHARACTER BUDGET (discriminating): a fresh subject with a fitting
    // multibyte item and an item whose standard serialized UTF-8 form exceeds
    // the whole budget while its Unicode scalar count does not. A provider that
    // budgeted characters would serve both on the first page; the real byte
    // budget defers the second.
    match adapter
        .ensure_subject(
            &memory_principal,
            dto::<MemorySubjectsEnsureRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": BYTE_CHAR_SUBJECT,
                "kind": { "ns": "r4", "id": "byte-char" },
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the byte/char subject is ensured: {reject:?}"),
    }
    let fit = create_byte_record(
        BYTE_CHAR_SUBJECT.to_string(),
        FIT_UTF8.to_string(),
        "r4-byte-char-fit",
    )
    .await;
    let fit_id = fit["result"]["record_id"]
        .as_str()
        .expect("the fitting record id")
        .to_string();
    let over_char_under = "关".repeat(OVER_CHAR_UNDER_CHARS);
    let budget = usize::try_from(BYTE_BUDGET).expect("the budget fits usize");
    // Independent inequalities from the bounded known input: the body is inside
    // the WHOLE budget by Unicode scalar count, but its standard serialized
    // UTF-8 form (and even its bare text bytes) are outside it. The fitting item
    // consumes a positive amount, so no byte-accounting page can admit this body
    // under the same budget. This is the platform serialization, not the
    // provider's private budget helper.
    assert!(
        over_char_under.chars().count() <= budget,
        "the body's Unicode scalar count is inside the whole budget"
    );
    let serialized =
        serde_json::to_vec(&json!({ "text": &over_char_under })).expect("serialized content");
    assert!(
        serialized.len() > budget,
        "the body's serialized UTF-8 bytes exceed the whole budget"
    );
    assert!(
        over_char_under.len() > budget,
        "even the body's bare UTF-8 text exceeds the whole budget"
    );
    let over = create_byte_record(
        BYTE_CHAR_SUBJECT.to_string(),
        over_char_under.clone(),
        "r4-byte-char-over",
    )
    .await;
    let over_id = over["result"]["record_id"]
        .as_str()
        .expect("the over-budget record id")
        .to_string();

    let char_recall = |cursor: Option<&str>| {
        let mut request = json!({
            "space_id": provider.space_id,
            "subject_id": BYTE_CHAR_SUBJECT,
            "selection": { "mode": "shared_only" },
            "budget": { "max_items": 10, "max_bytes": BYTE_BUDGET },
            "query": "cipher",
        });
        if let Some(cursor) = cursor {
            request["cursor"] = json!(cursor);
        }
        dto::<MemoryRecallRequest>(request)
    };
    let chars_first = recall_ok(&adapter, &memory_principal, char_recall(None)).await;
    assert_eq!(chars_first["total_authorized"], json!(2));
    assert_eq!(
        response_triples(&chars_first),
        sorted_triples(vec![(
            fit_id.clone(),
            json!({ "visibility": "shared" }),
            FIT_UTF8.to_string(),
        )]),
        "a character budget would also serve the 200-scalar body; the byte budget keeps only the fitting item"
    );
    assert_eq!(
        chars_first["overflow_record_id"],
        Value::Null,
        "the deferred body is not named while the page still has content"
    );
    assert_eq!(chars_first["truncated"], json!(true));
    let chars_cursor = chars_first["next_cursor"]
        .as_str()
        .expect("the fitting page carries a cursor")
        .to_string();

    let chars_second = recall_ok(
        &adapter,
        &memory_principal,
        char_recall(Some(&chars_cursor)),
    )
    .await;
    assert_eq!(chars_second["total_authorized"], json!(2));
    assert_eq!(
        response_triples(&chars_second),
        sorted_triples(vec![(
            over_id.clone(),
            json!({ "visibility": "shared" }),
            over_char_under.clone(),
        )]),
        "the body whose chars fit but bytes do not is deferred and then served whole"
    );
    assert_eq!(
        chars_second["overflow_record_id"],
        json!(over_id),
        "the deferred body is the named overflow on the resumed page"
    );
    assert_eq!(chars_second["truncated"], json!(false));
    assert_eq!(chars_second["next_cursor"], Value::Null);

    // CURSOR BINDING: the cursor continues only the ranking it was minted under.
    let first_page = match adapter
        .recall(
            &memory_principal,
            recall(
                json!({ "mode": "subject_all" }),
                page_size.clone(),
                Some("alliance"),
                None,
            ),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("cursor page"),
        SpokeResult::Reject(reject) => panic!("the first page is recalled: {reject:?}"),
    };
    let minted = first_page["next_cursor"]
        .as_str()
        .expect("the first page carries a cursor")
        .to_string();
    match adapter
        .recall(
            &memory_principal,
            recall(
                json!({ "mode": "subject_all" }),
                page_size.clone(),
                Some("granary"),
                Some(&minted),
            ),
        )
        .await
    {
        SpokeResult::Reject(reject) => assert_eq!(reject.code, SpokeRejectCode::CursorInvalid),
        SpokeResult::Ok(body) => panic!("a cursor from another query is refused: {body:?}"),
    }
    // A committed provider write advances the snapshot the cursor is bound to.
    match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "kind": "fact",
                "content": { "text": "A later alliance note." },
                "scope": { "visibility": "shared" },
                "idempotency_key": "r4-recall-later",
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the later record is created: {reject:?}"),
    }
    match adapter
        .recall(
            &memory_principal,
            recall(
                json!({ "mode": "subject_all" }),
                page_size,
                Some("alliance"),
                Some(&minted),
            ),
        )
        .await
    {
        SpokeResult::Reject(reject) => assert_eq!(reject.code, SpokeRejectCode::CursorInvalid),
        SpokeResult::Ok(body) => {
            panic!("a cursor from before the write is refused: {body:?}")
        }
    }
    // AUTHORIZED IDENTITY PARTITION: a second, differently scoped signed client.
    // The admin issues it a subject-scoped, context-restricted `recall` grant;
    // its own signed session must see exactly the granted context and must be
    // denied every wider selection. The SDK ignores the client principal
    // argument, so a deliberately wrong claimed principal proves the server's
    // authenticated peer is the authority.
    match adapter
        .grant(
            &memory_principal,
            dto::<MemoryGrantsGrantRequest>(json!({
                "capabilities": ["recall"],
                "context_ids": [RECALL_CONTEXT_ID],
                "idempotency_key": "r4-scoped-recall-grant",
                "principal_id": provider.scoped_principal.clone(),
                "space_id": provider.space_id,
                "subject_id": subject_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the scoped recall grant is issued: {reject:?}"),
    }
    let (scoped, scoped_responder) = provider
        .sdk_session_as(SCOPED_SEED, "r4-scoped-client")
        .await;
    let scoped_principal = provider.scoped_memory_principal();

    let scoped_only = recall_ok(
        &scoped,
        &scoped_principal,
        recall(
            json!({
                "mode": "selected_contexts",
                "context_ids": [RECALL_CONTEXT_ID],
                "include_shared": false,
            }),
            json!({ "max_items": 10 }),
            Some("alliance"),
            None,
        ),
    )
    .await;
    assert_eq!(
        response_triples(&scoped_only),
        sorted_triples(vec![context_triple.clone()]),
        "the scoped identity sees exactly its granted context"
    );

    for (why, selection) in [
        (
            "shared visibility is not granted",
            json!({ "mode": "shared_only" }),
        ),
        (
            "subject-wide recall is not granted",
            json!({ "mode": "subject_all" }),
        ),
        (
            "another context is not granted",
            json!({
                "mode": "selected_contexts",
                "context_ids": [RECALL_OTHER_CONTEXT_ID],
                "include_shared": false,
            }),
        ),
        (
            "shared visibility is not granted to the selection",
            json!({
                "mode": "selected_contexts",
                "context_ids": [RECALL_CONTEXT_ID],
                "include_shared": true,
            }),
        ),
    ] {
        let reject = recall_reject(
            &scoped,
            &scoped_principal,
            recall(
                selection,
                json!({ "max_items": 10 }),
                Some("alliance"),
                None,
            ),
        )
        .await;
        assert_eq!(reject.code, SpokeRejectCode::NotAuthorized, "{why}");
    }
    // A different subject is outside the grant's subject scope.
    let mut other_subject = json!({
        "space_id": provider.space_id,
        "subject_id": EXCLUDED_SUBJECT,
        "selection": {
            "mode": "selected_contexts",
            "context_ids": [RECALL_CONTEXT_ID],
            "include_shared": false,
        },
        "budget": { "max_items": 10 },
    });
    other_subject["query"] = json!("alliance");
    let reject = recall_reject(
        &scoped,
        &scoped_principal,
        dto::<MemoryRecallRequest>(other_subject),
    )
    .await;
    assert_eq!(
        reject.code,
        SpokeRejectCode::NotAuthorized,
        "another subject is outside the grant's subject scope"
    );

    // The claimed client principal is not authority: the same scoped session
    // claiming the admin principal still sees only its granted context.
    let claimed_admin = recall_ok(
        &scoped,
        &memory_principal,
        recall(
            json!({
                "mode": "selected_contexts",
                "context_ids": [RECALL_CONTEXT_ID],
                "include_shared": false,
            }),
            json!({ "max_items": 10 }),
            Some("alliance"),
            None,
        ),
    )
    .await;
    assert_eq!(
        response_triples(&claimed_admin),
        sorted_triples(vec![context_triple.clone()]),
        "a payload-claimed principal does not widen the authenticated peer's authority"
    );
    scoped.close();
    let scoped_host = responder_of(scoped_responder).await;
    assert_eq!(
        scoped_host.remote_peer_id().as_deref(),
        Some(provider.scoped_peer.as_str()),
        "the scoped effects travelled the scoped peer's own signed session"
    );

    // A space this provider does not serve is its definite absence: the pinned
    // space is the provider's own, never a local default that would answer here.
    match adapter
        .recall(
            &memory_principal,
            dto::<MemoryRecallRequest>(json!({
                "space_id": "spc_nexus_local_default",
                "subject_id": subject_id,
                "selection": { "mode": "subject_all" },
                "budget": { "max_items": 5 },
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => assert_eq!(reject.code, SpokeRejectCode::NotFound),
        SpokeResult::Ok(body) => panic!("an unserved space is not recalled: {body:?}"),
    }

    adapter.close();
    responder_of(sdk_responder).await;

    // Every durable record carries the provider's own space, and the recall
    // itself wrote nothing.
    let pool = provider.pool().await;
    let spaces: Vec<String> = sqlx::query("SELECT DISTINCT space_id FROM records")
        .fetch_all(&pool)
        .await
        .expect("record spaces")
        .into_iter()
        .map(|row| row.get::<String, _>("space_id"))
        .collect();
    assert_eq!(spaces, vec![provider.space_id.clone()]);
    assert_eq!(
        provider_records(&pool, &subject_id).await.len(),
        5,
        "two shared, two context-placed and the later shared record; recall wrote nothing"
    );
    assert_eq!(
        provider_records(&pool, BYTE_SUBJECT).await.len(),
        2,
        "the byte-budget subject keeps exactly its two seeded records"
    );
    assert_eq!(
        provider_records(&pool, BYTE_CHAR_SUBJECT).await.len(),
        2,
        "the byte/char subject keeps exactly its two seeded records"
    );
    assert_eq!(
        provider_records(&pool, EXCLUDED_SUBJECT).await.len(),
        1,
        "the other subject keeps its own record, unserved by every selection above"
    );
    pool.close().await;

    restore_legacy_tables(&trap).await;
    assert_no_local_memory(&env).await;
    assert_legacy_dir_traps(&dirs);

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
// The admitted Host Character mind projection
// ---------------------------------------------------------------------------

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
            display_name: "R4 mind fixture".to_string(),
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
            message: Some("r4 mind fixture".to_string()),
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
                HostContentBlock::ResourceLink { .. } => None,
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

    async fn cancel(
        &self,
        _session: &ManagedSessionHandle,
        _op: HostOperationId,
    ) -> HostResult<()> {
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

/// The production admitted Host composition assembles the Character prompt
/// through `CoreCharacterMind`, which reads the Character subject's
/// `personality` profile with `memory.profile.get` and the two memory families
/// with kind-filtered `memory.records.list`, both on the real signed provider in
/// the pinned space. The decorator's counters are subject-scoped totals, so the
/// deltas measured around the `execute` call are that execution's own reads;
/// which selection/kinds those reads carried is evidenced by the prompt content
/// and by the provider's stored objects, not by the counters.
#[tokio::test]
async fn host_character_mind_projection_serves_soul_and_memory_from_the_remote_provider() {
    let env = Env::boot("R4 Host Owner").await;
    let provider = Provider::open().await;
    let probe = Arc::new(ServiceProbe::over(
        provider.inner.server(provider.directory.admission()),
    ));
    let (core, responder) = provider
        .open_core_serving(&env, probe.clone(), CoreAccess::EngineOwner)
        .await;
    let principal = core.active_principal().await.expect("principal");

    let world = create_world(&core, &principal, "R4 Host World").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "R4 Host Ada", &world).await;
    let subject_id = legacy::character_subject_id(&character_id);
    let (trap, dirs) = arm_legacy_traps(&env, Some(&character_id)).await;

    // One binding-local memory through the real public review path: the
    // provider's binding-local fragment `fact` record the projection must
    // serve. This write also ensures the Character provider subject.
    capture(
        &core,
        &principal,
        &character_id,
        &binding_id,
        "pend_r4_mind",
    )
    .await
    .expect("capture the binding-local digest");
    core.review_character_memory(
        &principal,
        character_id.clone(),
        dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_id })),
    )
    .await
    .expect("review the binding-local digest");

    // The Character's authored SOUL document through the SDK's own signed
    // session: a real provider profile object, never a local file.
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = Provider::memory_principal();
    match adapter
        .update_profile(
            &memory_principal,
            dto::<MemoryProfileUpdateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "profile_kind": "personality",
                "expected_revision": null,
                "document": { "format": "markdown", "body": SOUL_BODY },
                "idempotency_key": "r4-character-soul",
            })),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the Character SOUL is authored: {reject:?}"),
    }
    adapter.close();
    responder_of(sdk_responder).await;

    probe.watch(&subject_id);

    // Deterministic local agent host: no process, no model, no network.
    let manager = Arc::new(HostManager::new());
    let mind_adapter = Arc::new(MindFixtureAdapter::default());
    manager
        .register_provider(
            mind_adapter.clone(),
            LaunchStrategy::NativeCli {
                command: "r4-mind-fixture".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        )
        .await;
    // The Host's workspace boundary must contain both the Nexus home and the
    // admitted Character creative root this open pinned, so it is the user
    // home — the root the production native boot hands the manager.
    let workspace_root = env.user_home.clone();
    let host_config = nexus_agent_host::config::AgentHostConfig {
        max_ops_per_session: 4,
        ..Default::default()
    };
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

    // The counters are subject-scoped totals that already include session
    // creation; only the delta across this execution is attributed to it.
    let profile_gets_before = probe.profile_gets();
    let record_lists_before = probe.record_lists();
    handle
        .execute(
            &principal,
            session.session_id.clone(),
            dto::<ExecuteOperationRequest>(json!({
                "kind": "prompt",
                "content": "What do you remember about the alliance?",
            })),
        )
        .await
        .expect("the admitted Character prompt runs");

    assert!(
        probe.profile_gets() > profile_gets_before,
        "this execution performed memory.profile.get on the Character subject"
    );
    assert!(
        probe.record_lists() > record_lists_before,
        "this execution performed kind-filtered memory.records.list on the Character subject"
    );
    let prompts = mind_adapter.prompts();
    assert_eq!(prompts.len(), 1, "the host assembled one prompt");
    assert!(
        prompts[0].contains("R4 keeps the northern passes and the winter granary."),
        "the mind projection served the provider's SOUL document into the prompt: {}",
        prompts[0]
    );
    assert!(
        prompts[0].contains(PROMOTE_DIGEST),
        "the mind projection served the provider's promoted memory into the prompt: {}",
        prompts[0]
    );

    // The provider's own durable objects are the profile and the promoted
    // record; nothing of either came from a local file or the local provider.
    let pool = provider.pool().await;
    let (space_id, body, revision) = provider_personality(&pool, &subject_id)
        .await
        .expect("the provider stores the Character SOUL document");
    assert_eq!(space_id, provider.space_id);
    assert_eq!(body, SOUL_BODY);
    assert_eq!(revision, 1);
    let records = provider_records(&pool, &subject_id).await;
    let fact = records
        .iter()
        .find(|(kind, _, _)| kind == "fact")
        .expect("the provider stores the reviewed binding-local fragment");
    assert_eq!(
        fact.1, "context",
        "the fragment keeps its binding-local capture scope"
    );
    assert_eq!(
        fact.2.as_deref(),
        Some(binding_id.as_str()),
        "the fragment belongs to the selected binding"
    );
    assert!(
        records
            .iter()
            .all(|(_, _, context)| context.as_deref() == Some(binding_id.as_str())),
        "the projection's only provider records are the binding-local ones: {records:?}"
    );
    pool.close().await;

    restore_legacy_tables(&trap).await;
    assert_no_local_memory(&env).await;
    assert_legacy_dir_traps(&dirs);

    assert!(
        handle.close().await.expect("host close").cleanup_confirmed,
        "the host releases its own admission"
    );
    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str()),
        "every projection read travelled the signed session of the loaded credential"
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
}
