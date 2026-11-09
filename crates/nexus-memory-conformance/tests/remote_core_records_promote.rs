//! R3 (v1.229 N3 Task 3): remote record reads and writes, promotion and recall
//! — driven through the real public `CoreService` over the selected remote
//! provider.
//!
//! Every scenario opens Core with the selected remote provider, so the path
//! under test is the product one: the pinned `MemoryProviderBinding`, the SDK
//! `RemoteAdapter` over a signed Connect session, and the Nexus-side
//! admission/refusal policy. The provider on the other end is the independent,
//! durable `FixtureMemoryProvider` sharing nothing with Core's local provider,
//! served by the SDK's `connect_responder` over a real `127.0.0.1` socket with
//! the one product WS transport on both ends. Nothing is mocked: the credential
//! file, the signed hello, the operator-provisioned grant, the fixture's own
//! space/authority and its durable records/receipts are the real ones.
//!
//! What these tests prove, and what they intentionally cannot:
//!
//! - The Creator record family (`create_creator_memory`, `list_creator_memories`,
//!   `get_creator_memory`, `update_creator_memory`, `delete_creator_memory`) and
//!   the Character fragment family (`list_character_memory_fragments`,
//!   `promote_character_fragment`) run on the remote provider. `list_records` and
//!   `get_record` are the named remote record reads of this slice: the Creator
//!   `GET` is the by-id miss plus the authored-kind list, the fragment read is
//!   the by-id HIT `get_record`, and every effect is asserted in the fixture's
//!   own database, never in the local provider's tables or files.
//! - The provider-side space is the pinned selected one: every record read
//!   carries `space_id` = the provider's own space, every write lands in that
//!   space, and a recall over a space this provider does not serve is its
//!   definite `NOT_FOUND` — so no local space constant can stand in.
//! - Recall has no Core product caller on this slice (`soul::read` reads
//!   `memory.records.list` instead of ranking every kind through recall), so the
//!   recall operation is exercised over the SDK's own signed session against the
//!   same durable provider and the same pinned space. It is the SDK client, not
//!   a fake port.
//! - The legacy Creator queue, the legacy Character queue and the legacy
//!   Character fragment table are withheld from this disposable state DB for the
//!   whole lifecycle, and each applicable bearer's real long-term discovery
//!   DIRECTORY (`memory_io::list_memories` calls `fs::read_dir` on it) is a
//!   pre-existing regular file, so that discovery fails with a non-`NotFound`
//!   error instead of reading anything. The operations above are therefore proof
//!   that those reads never happen — not merely that nothing was written. The
//!   scope is exactly those tables and those two discovery paths: Nexus
//!   actor/world/binding metadata stays readable, and this is not a universal
//!   no-read claim about unrelated local surfaces.
//! - A read-only core keeps serving the record reads while every record write
//!   keeps its retained typed refusal, before any provider call.
//! - A caller that read a revision another writer has since changed keeps the
//!   retained `version_mismatch` and writes nothing: the promotion validates the
//!   record at exactly the provider revision its CAS targets, so a stale read is
//!   refused before any provider call and a concurrent replacement cannot get an
//!   unvalidated revision promoted.
//! - The served Character fragment family is the contract's, not every protocol
//!   `fact`: when an authenticated second writer replaces a *known* fragment's
//!   whole content (removing `nexus_legacy`), LIST and promotion report that
//!   damage honestly — its live D18 fact still proves the origin — while a
//!   provider-native fact (no Character-fragment producer marker and no fact) is
//!   not served by the fragment LIST and keeps the retained non-fragment refusal
//!   on promotion. Every refusal happens before any provider mutation
//!   (placement, revision, promote receipts and D18 facts unchanged). A valid
//!   fragment still promotes, and its response is already sound if a later
//!   writer rewrites the record, so no committed promotion becomes a projection
//!   failure.
//! - Refusals reach the caller as the static classified `MemoryProviderRefused`
//!   carrier: the provider's own message text and any request content never
//!   travel with it.
//!
//! `MemoryProviderBinding` is crate-private, so this crate-external test cannot
//! read the pinned `(space, authority, client identity)` tuple back. The
//! provider's own rows are the observable substitute: their `space_id`,
//! `provenance.captured_by` (the opaque peer-derived principal) and revisions are
//! written from that pinned tuple by the product code.

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use nexus_contracts::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::generated::core::CoreCharacterTransitionRequest;
use nexus_contracts::{CoreRegisterCreatorRequest, SetActiveWorkspaceRequest};
use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::memory_provider::legacy;
use nexus_core::{
    CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreResult, CoreService,
    MemoryProviderRefusal, MemoryProviderSelection, MemoryRevision, Principal,
    RemoteMemoryProviderSelection,
};
use nexus_memory_conformance::{FixtureMemoryProvider, PeerDirectory};
use serde_json::{json, Value};
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponder, ConnectResponderOptions,
    RemoteAdapter, RemoteAdapterOptions, RemoteIdentity, RemoteMemoryService, RemoteServePorts,
    RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, MemoryRecallRequest,
    MemoryRecordsCreateRequest, MemoryRecordsUpdateRequest, Principal as MemoryPrincipal, Relation,
    Rule, Scope, TimelineEvent,
};
use spoke_operations::{
    FindingPort, HostManifestPort, KnowledgeEntryPort, MemoryPort, RelationPort, RuleQueryPort,
    ScopeQueryPort, SpokeRejectCode, SpokeResult,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// The host's Connect identity (the provider side of the signed session).
const HOST_SEED: [u8; 32] = [0x42; 32];
/// The dialing client's Connect identity; the credential file holds this seed.
const CLIENT_SEED: [u8; 32] = [0x31; 32];
/// The competing client's *own* Connect identity: the independent signed session
/// that writes while Core's request is held. The fixture provisioned it as a
/// separate operator principal, so its writes are its own authenticated ones.
const WRITER_SEED: [u8; 32] = [0x5a; 32];
/// The one capability both ends negotiate: the fixture serves no other family.
const MEMORY_CAPABILITIES: [&str; 1] = ["spoke-memory"];
const WORKSPACE: &str = "default";
const CREDENTIAL_ID: &str = "cred-r3-records";
const BOUND: Duration = Duration::from_secs(5);
/// Bytes of the file that occupies a bearer's long-term discovery *directory*.
///
/// `memory_io::list_memories` calls `fs::read_dir` on that directory and only
/// treats `NotFound` as honest-empty, so a regular file there makes every real
/// discovery attempt fail loudly — the trap the lifecycle below must not trip.
const LEGACY_DIR_TRAP_BYTES: &[u8] = b"r3-legacy-long-term-dir-trap";
/// The legacy memory surfaces withheld from the disposable state DB.
const LEGACY_MEMORY_TABLES: [&str; 3] = [
    "memory_pending_review",
    "character_memory_pending_review",
    "character_memory_fragments",
];
const TRAP_SUFFIX: &str = "_r3_trap";
const SLUG: &str = "r3-pinned-notes";
const BODY: &str = "The northern alliance holds three passes and a shared granary.";
const EDITED_BODY: &str = "The northern alliance holds four passes after the winter treaty.";
/// A high-signal digest the deterministic classifier promotes for `brainstorm`.
const FRAGMENT_DIGEST: &str =
    "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";
/// The fixture's own `max_content_bytes` is 65536 and it measures the whole
/// serialized `content`; this body exceeds it while Core's own record call
/// performs no content-size validation, so the refusal is genuinely the
/// provider's.
const OVER_LIMIT_BODY_BYTES: usize = 70_000;

// ---------------------------------------------------------------------------
// A memory-only responder host
// ---------------------------------------------------------------------------

/// The baseline half of the responder's ports face: `spoke-baseline` is never
/// negotiated here, so the dispatch gate answers every `port.*` op before it
/// could reach this type. It exists because `RemoteServePorts` is a
/// baseline-port supertrait, and it refuses honestly.
struct MemoryOnlyHost;

fn no_baseline<T>() -> SpokeResult<T> {
    SpokeResult::Reject(spoke_operations::SpokeReject {
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
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").expect("a String write never fails");
    }
    out
}

/// The signed peers this fixture admits, as `(peer id, public key)` pairs.
struct Peers(Vec<(String, [u8; 32])>);

impl Peers {
    fn of(seeds: &[[u8; 32]]) -> Self {
        Self(
            seeds
                .iter()
                .map(|seed| (peer_id(seed), pubkey(seed)))
                .collect(),
        )
    }
}

/// Bind a real `127.0.0.1:0` listener whose first `connections` accepted
/// `WebSockets` each become one signed responder over the same memory service.
///
/// Two connections are what the causal-window histories need: Core's own signed
/// session and the independent writer's, each with its own socket, hello and
/// admitted identity.
async fn start_listener_connections(
    memory_service: Arc<dyn RemoteMemoryService>,
    host_manifest: HostCapabilityManifest,
    peers: Peers,
    connections: usize,
) -> (
    SocketAddr,
    tokio::task::JoinHandle<Vec<Arc<ConnectResponder>>>,
) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let task = tokio::spawn(async move {
        let allowlist: Vec<String> = peers.0.iter().map(|(peer, _)| peer.clone()).collect();
        let peer_keys: HashMap<String, [u8; 32]> = peers.0.iter().cloned().collect();
        let mut responders = Vec::with_capacity(connections);
        for _ in 0..connections {
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
                    .with_memory(Arc::clone(&memory_service)),
            );
            responders.push(
                connect_responder(ConnectResponderOptions {
                    transport,
                    identity: RemoteIdentity { seed: HOST_SEED },
                    manifest: host_manifest.clone(),
                    allowlist: allowlist.clone(),
                    peer_keys: peer_keys.clone(),
                    ports: Some(ports),
                    invoke_timeout_ms: Some(5_000),
                })
                .await,
            );
        }
        responders
    });
    (addr, task)
}

/// [`start_listener_connections`] for the one-client case every non-race
/// scenario uses.
async fn start_listener(
    memory_service: Arc<dyn RemoteMemoryService>,
    host_manifest: HostCapabilityManifest,
    client_peer: String,
    client_pubkey: [u8; 32],
) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
    let (addr, task) = start_listener_connections(
        memory_service,
        host_manifest,
        Peers(vec![(client_peer, client_pubkey)]),
        1,
    )
    .await;
    let joined = tokio::spawn(async move {
        task.await
            .expect("the listener task completed")
            .pop()
            .expect("the listener accepted its one connection")
    });
    (addr, joined)
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
    /// The competing client's peer id and the operator principal it was
    /// provisioned as.
    writer_peer: String,
    writer_principal: String,
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
        let writer_peer = peer_id(&WRITER_SEED);
        let writer_principal = format!("prn_{writer_peer}");
        for principal in [&principal_id, &writer_principal] {
            inner
                .provision_operator_admin(principal)
                .await
                .expect("operator provisions the admin grant");
        }
        let space_id = inner.identity().space_id.clone();
        Self {
            _home: home,
            db,
            inner,
            directory: Arc::new(
                PeerDirectory::new()
                    .with_peer(client_peer, principal_id)
                    .with_peer(writer_peer.clone(), writer_principal.clone()),
            ),
            space_id,
            writer_peer,
            writer_principal,
        }
    }

    /// Serve `connections` signed sessions over one listener, admitting both the
    /// Core client and the independent writer.
    async fn serve_connections(
        &self,
        service: Arc<dyn RemoteMemoryService>,
        connections: usize,
    ) -> (
        SocketAddr,
        tokio::task::JoinHandle<Vec<Arc<ConnectResponder>>>,
    ) {
        start_listener_connections(
            service,
            manifest(self.inner.identity().authority_host_id.as_str()),
            Peers::of(&[CLIENT_SEED, WRITER_SEED]),
            connections,
        )
        .await
    }

    /// Dial one real signed SDK session to this endpoint as `seed`'s identity.
    async fn dial(&self, addr: SocketAddr, seed: [u8; 32], host_id: &str) -> Arc<RemoteAdapter> {
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
            local_manifest: manifest(host_id),
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

    /// The competing client's own memory principal.
    fn writer_memory_principal(&self) -> MemoryPrincipal {
        serde_json::from_value(json!({
            "principal_id": self.writer_principal,
            "extensions": {},
        }))
        .expect("valid memory principal")
    }

    /// Serve one signed session over a fresh loopback listener.
    async fn serve(&self) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        self.serve_service(self.inner.server(self.directory.admission()))
            .await
    }

    /// Serve one signed session for an explicitly supplied memory service.
    ///
    /// Every ordinary scenario passes this provider's real assembled service
    /// here; the causal-window scenarios pass a test-only decorator that
    /// forwards every request unchanged and holds one response, while the
    /// independent signed writer performs its own `memory.records.update` on its
    /// own session. The decorator never writes or manufactures a reply.
    async fn serve_service(
        &self,
        service: Arc<dyn RemoteMemoryService>,
    ) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
        start_listener(
            service,
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
        self.open_core_service(env, self.inner.server(self.directory.admission()), access)
            .await
    }

    async fn open_core_service(
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

    /// Dial the SDK `RemoteAdapter` over its own signed loopback session.
    ///
    /// This is the SDK's own client (the pattern the conformance loopback uses),
    /// not Core: it addresses the provider operations Core's surface does not
    /// expose on this task — `memory.recall` — and acts as the second writer that
    /// changes a record between a caller's read and its revision-checked write.
    async fn sdk_session(
        &self,
    ) -> (
        Arc<RemoteAdapter>,
        tokio::task::JoinHandle<Arc<ConnectResponder>>,
    ) {
        let (addr, task) = self.serve().await;
        let adapter = self.dial(addr, CLIENT_SEED, "r3-records-client").await;
        (adapter, task)
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

// ---------------------------------------------------------------------------
// Legacy no-read traps
// ---------------------------------------------------------------------------

/// Withhold every legacy memory surface for the rest of the lifecycle and
/// occupy the real long-term discovery paths with a pre-existing regular file.
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
        .expect("withhold a legacy memory table from the whole lifecycle");
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
/// `list_memories` performs) still fails with a non-`NotFound` error. Any
/// accidental legacy file discovery above would have failed loudly instead.
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
            "the real list_memories discovery fails on this path, so it was never read: {discovery:?}"
        );
    }
}

/// The local provider and legacy memory surfaces stayed untouched: no legacy
/// row, no local provider row and no local provider file root.
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
        "a remote binding never constructs the local provider"
    );
}

// ---------------------------------------------------------------------------
// Provider-side observation helpers
// ---------------------------------------------------------------------------

/// One durable provider record row.
#[derive(Debug)]
struct ProviderRecord {
    record_id: String,
    kind: String,
    visibility: String,
    context_id: Option<String>,
    revision: i64,
    tombstoned: bool,
}

async fn provider_record(pool: &SqlitePool, record_id: &str) -> Option<ProviderRecord> {
    sqlx::query(
        "SELECT record_id, kind, scope_visibility, scope_context_id, revision, tombstone_json \
         FROM records WHERE record_id = ?",
    )
    .bind(record_id)
    .fetch_optional(pool)
    .await
    .expect("record row")
    .map(|row| ProviderRecord {
        record_id: row.get::<String, _>("record_id"),
        kind: row.get::<String, _>("kind"),
        visibility: row.get::<String, _>("scope_visibility"),
        context_id: row.get::<Option<String>, _>("scope_context_id"),
        revision: row.get::<i64, _>("revision"),
        tombstoned: row.get::<Option<String>, _>("tombstone_json").is_some(),
    })
}

/// The subject's durable record rows, ordered by id for a stable assertion.
async fn provider_records(pool: &SqlitePool, subject_id: &str) -> Vec<ProviderRecord> {
    sqlx::query(
        "SELECT record_id, kind, scope_visibility, scope_context_id, revision, tombstone_json \
         FROM records WHERE subject_id = ? ORDER BY record_id",
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await
    .expect("record rows")
    .into_iter()
    .map(|row| ProviderRecord {
        record_id: row.get::<String, _>("record_id"),
        kind: row.get::<String, _>("kind"),
        visibility: row.get::<String, _>("scope_visibility"),
        context_id: row.get::<Option<String>, _>("scope_context_id"),
        revision: row.get::<i64, _>("revision"),
        tombstoned: row.get::<Option<String>, _>("tombstone_json").is_some(),
    })
    .collect()
}

async fn provider_content(pool: &SqlitePool, record_id: &str) -> Value {
    let text = sqlx::query("SELECT content_json FROM records WHERE record_id = ?")
        .bind(record_id)
        .fetch_one(pool)
        .await
        .expect("record content")
        .get::<String, _>("content_json");
    serde_json::from_str(&text).expect("stored content is JSON")
}

async fn provider_provenance(pool: &SqlitePool, record_id: &str) -> Value {
    let text = sqlx::query("SELECT provenance_json FROM records WHERE record_id = ?")
        .bind(record_id)
        .fetch_one(pool)
        .await
        .expect("record provenance")
        .get::<String, _>("provenance_json");
    serde_json::from_str(&text).expect("stored provenance is JSON")
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
            "raw_digest": FRAGMENT_DIGEST,
            "created_at": "2026-01-01T00:00:01Z",
            "binding_id": binding_id,
        })),
    )
    .await
    .map(|_| ())
}

/// The classified refusal every remote record call carries, without the peer's
/// message: `(code, retry_class, family, wire_code, conflict_code)`.
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

// ---------------------------------------------------------------------------
// Creator record reads and writes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn creator_record_reads_and_writes_run_on_the_remote_provider() {
    let env = Env::boot("R3 Record Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");
    let subject_id = legacy::creator_subject_id(&env.creator_id);

    // The traps are armed BEFORE the first record operation and stay armed for
    // the whole lifecycle below.
    let (trap, dirs) = arm_legacy_traps(&env, None).await;

    let pool = provider.pool().await;
    let record_id =
        creator_create_commits_one_provider_record(&pool, &core, &principal, &subject_id).await;

    creator_list_and_slug_read_serve_the_pinned_space(&provider, &core, &principal, &record_id)
        .await;

    creator_update_and_stale_read_are_exact(&core, &principal, &pool, &record_id).await;

    let (create_receipts, update_receipts, delete_receipts) =
        creator_delete_tombstones_and_reports_receipts(&core, &principal, &pool, &record_id).await;
    pool.close().await;

    read_only_core_refuses_creator_record_writes(
        &env,
        &provider,
        &subject_id,
        create_receipts,
        update_receipts,
        delete_receipts,
    )
    .await;

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

/// The standard `memory.records.create` in the pinned space commits exactly one
/// provider record with the authored identity, opaque principal and no
/// credential material.
async fn creator_create_commits_one_provider_record(
    pool: &SqlitePool,
    core: &CoreService,
    principal: &Principal,
    subject_id: &str,
) -> String {
    assert!(core
        .create_creator_memory(principal, SLUG, "story_summary", BODY)
        .await
        .expect("create the Creator memory"));
    // A taken slug is the retained `Ok(false)` answer and writes nothing.
    assert!(
        !core
            .create_creator_memory(principal, SLUG, "story_summary", BODY)
            .await
            .expect("the duplicate create is refused"),
        "the slug is already taken"
    );

    let rows = provider_records(pool, subject_id).await;
    assert_eq!(rows.len(), 1, "one provider record, one create: {rows:?}");
    let created = &rows[0];
    assert_eq!(created.kind, "nexus.story_summary");
    assert_eq!(created.visibility, "shared");
    assert_eq!(created.context_id, None);
    assert_eq!(created.revision, 1);
    assert!(!created.tombstoned);
    assert_eq!(
        provider_receipts(pool, "memory.records.create").await,
        1,
        "the duplicate never committed a second receipt"
    );
    let content = provider_content(pool, &created.record_id).await;
    assert_eq!(content["text"], json!(BODY));
    assert_eq!(
        content["structured"]["nexus_legacy"]["slug"],
        json!(SLUG),
        "the authored identity rides in the record's own legacy block"
    );
    assert_eq!(
        content["structured"]["nexus_legacy"]["memory_kind"],
        json!("story_summary")
    );
    let provenance = provider_provenance(pool, &created.record_id).await;
    assert_eq!(
        provenance["captured_by"],
        json!(format!("prn_{}", peer_id(&CLIENT_SEED))),
        "the provider sees the authenticated peer's opaque principal, not a credential id"
    );
    // The loaded credential's own id and signing seed are the material the dial
    // used; neither is content the provider is asked to store.
    for column in ["content_json", "provenance_json"] {
        for secret in [CREDENTIAL_ID, hex(&CLIENT_SEED).as_str()] {
            assert_eq!(
                scalar(
                    pool,
                    &format!("SELECT COUNT(*) FROM records WHERE {column} LIKE '%{secret}%'"),
                )
                .await,
                0,
                "the credential material never reaches the provider's `{column}`"
            );
        }
    }
    created.record_id.clone()
}

/// LIST and the authored slug GET serve the pinned provider space.
async fn creator_list_and_slug_read_serve_the_pinned_space(
    provider: &Provider,
    core: &CoreService,
    principal: &Principal,
    record_id: &str,
) {
    let listed = core
        .list_creator_memories(principal)
        .await
        .expect("list the Creator memories");
    assert_eq!(listed.len(), 1);
    let listed = serde_json::to_value(&listed[0]).expect("record json");
    assert_eq!(listed["record_id"], json!(record_id));
    assert_eq!(
        listed["space_id"],
        json!(provider.space_id),
        "the read carries the pinned provider space, not a local constant"
    );
    assert_eq!(listed["revision"], json!(1));
    assert_eq!(listed["kind"], json!("nexus.story_summary"));
    assert_eq!(listed["scope"]["visibility"], json!("shared"));
    assert_eq!(listed["content"]["text"], json!(BODY));

    // GET by authored slug: the by-id probe misses on the provider and the
    // authored-kind list resolves the record, both in the pinned space.
    let got = core
        .get_creator_memory(principal, SLUG)
        .await
        .expect("get the Creator memory")
        .expect("the record exists on the provider");
    assert_eq!(
        serde_json::to_value(&got).expect("record json")["record_id"],
        json!(record_id)
    );
}

/// A revision-checked update bumps the provider once; a stale read is refused
/// before the write and the edited body is what the read now serves.
async fn creator_update_and_stale_read_are_exact(
    core: &CoreService,
    principal: &Principal,
    pool: &SqlitePool,
    record_id: &str,
) {
    let expected = MemoryRevision {
        record_id: record_id.to_owned(),
        revision: 1,
    };
    assert!(core
        .update_creator_memory(principal, SLUG, EDITED_BODY, Some(&expected))
        .await
        .expect("update the record"));
    let updated = provider_record(pool, record_id)
        .await
        .expect("the record row");
    assert_eq!(updated.revision, 2, "the provider bumped the revision once");
    assert_eq!(
        provider_content(pool, record_id).await["text"],
        json!(EDITED_BODY)
    );
    assert_eq!(provider_receipts(pool, "memory.records.update").await, 1);

    // A caller whose read is stale is refused before the write, in the retained
    // `version_mismatch` shape, and the provider is untouched. (The provider's
    // own stale-revision CAS is reached in the promotion target below.)
    let stale = core
        .update_creator_memory(principal, SLUG, BODY, Some(&expected))
        .await
        .expect_err("a stale read is refused");
    assert!(
        matches!(&stale, CoreError::ActorConflict { code, .. } if code == "version_mismatch"),
        "got {stale:?}"
    );
    let unchanged = provider_record(pool, record_id)
        .await
        .expect("the record row");
    assert_eq!(unchanged.revision, 2);
    assert_eq!(
        provider_receipts(pool, "memory.records.update").await,
        1,
        "a refused update commits nothing"
    );

    // The edited body is what the read now serves.
    let edited = core
        .get_creator_memory(principal, SLUG)
        .await
        .expect("get the edited record")
        .expect("the record exists");
    let edited = serde_json::to_value(&edited).expect("record json");
    assert_eq!(edited["revision"], json!(2));
    assert_eq!(edited["content"]["text"], json!(EDITED_BODY));
}

/// DELETE tombstones the provider record, and the authored reads agree; returns
/// the committed create/update/delete receipt counts.
async fn creator_delete_tombstones_and_reports_receipts(
    core: &CoreService,
    principal: &Principal,
    pool: &SqlitePool,
    record_id: &str,
) -> (i64, i64, i64) {
    assert!(core
        .delete_creator_memory(principal, SLUG)
        .await
        .expect("delete the record"));
    let tombstone = provider_record(pool, record_id)
        .await
        .expect("the record row");
    assert!(tombstone.tombstoned, "the provider tombstoned the record");
    assert_eq!(tombstone.revision, 3);
    assert_eq!(provider_receipts(pool, "memory.records.delete").await, 1);
    assert!(
        core.get_creator_memory(principal, SLUG)
            .await
            .expect("get the deleted record")
            .is_none(),
        "a tombstoned record is absent from the authored slug read"
    );
    assert!(
        core.list_creator_memories(principal)
            .await
            .expect("list after delete")
            .is_empty(),
        "a tombstoned record leaves the listing"
    );
    assert!(
        !core
            .delete_creator_memory(principal, SLUG)
            .await
            .expect("delete an absent record"),
        "the absent delete is the retained false"
    );
    (
        provider_receipts(pool, "memory.records.create").await,
        provider_receipts(pool, "memory.records.update").await,
        provider_receipts(pool, "memory.records.delete").await,
    )
}

/// A read-only core serves the reads and refuses every record write before it
/// can reach the provider; its own admission is released.
async fn read_only_core_refuses_creator_record_writes(
    env: &Env,
    provider: &Provider,
    subject_id: &str,
    create_receipts: i64,
    update_receipts: i64,
    delete_receipts: i64,
) {
    let (read_only, ro_responder) = provider.open_core_as(env, CoreAccess::ReadOnly).await;
    let ro_principal = read_only
        .active_principal()
        .await
        .expect("active principal");
    assert!(read_only
        .list_creator_memories(&ro_principal)
        .await
        .expect("a read-only core serves the record read")
        .is_empty());
    let refused_create = read_only
        .create_creator_memory(&ro_principal, "r3-read-only", "story_summary", BODY)
        .await
        .expect_err("a read-only core refuses the record create");
    assert!(
        matches!(&refused_create, CoreError::Forbidden { resource } if resource == "memory: read-only core access"),
        "got {refused_create:?}"
    );
    let refused_update = read_only
        .update_creator_memory(&ro_principal, SLUG, BODY, None)
        .await
        .expect_err("a read-only core refuses the record update");
    assert!(
        matches!(&refused_update, CoreError::Forbidden { resource } if resource == "memory: read-only core access"),
        "got {refused_update:?}"
    );
    let refused_delete = read_only
        .delete_creator_memory(&ro_principal, SLUG)
        .await
        .expect_err("a read-only core refuses the record delete");
    assert!(
        matches!(&refused_delete, CoreError::Forbidden { resource } if resource == "memory: read-only core access"),
        "got {refused_delete:?}"
    );
    let pool = provider.pool().await;
    assert_eq!(
        provider_records(&pool, subject_id).await.len(),
        1,
        "the read-only refusals wrote no record"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.create").await,
        create_receipts
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.update").await,
        update_receipts
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.delete").await,
        delete_receipts
    );
    pool.close().await;
    assert!(
        read_only.close().await.expect("close").cleanup_confirmed,
        "the read-only open releases its own admission"
    );
    responder_of(ro_responder).await;
}

// ---------------------------------------------------------------------------
// Character fragment reads and promotion
// ---------------------------------------------------------------------------

/// Rewrites the committed promotion receipt one provider returns, so the Core
/// is shown a schema-valid answer that belongs to another request. It changes
/// no durable state and every other answer passes through untouched.
struct MismatchedPromotionReceipt {
    inner: Arc<dyn RemoteMemoryService>,
    rewrite: Arc<AtomicBool>,
}

#[async_trait]
impl RemoteMemoryService for MismatchedPromotionReceipt {
    async fn invoke(&self, peer_id: &str, op: &str, payload: Value) -> SpokeResult<Value> {
        let answer = self.inner.invoke(peer_id, op, payload).await;
        if !self.rewrite.load(Ordering::SeqCst) || op != "memory.records.promote" {
            return answer;
        }
        match answer {
            SpokeResult::Ok(mut body) => {
                body["idempotency_key"] = json!("a-promote-receipt-for-another-request");
                SpokeResult::Ok(body)
            }
            SpokeResult::Reject(reject) => SpokeResult::Reject(reject),
        }
    }
}

/// D18/I-000283: a committed promotion receipt that does not prove THIS
/// promotion must not release the record's binding dependency — and it must
/// not claim the promotion. The unit negative covers
/// `promote_result_record_id` directly; this runs the same mismatch through
/// the real signed service path, where the receipt's echoed idempotency key is
/// rewritten in flight.
///
/// The promotion fails the static `memory_promote_receipt` check and the
/// record's live D18 fact stays live, so no other live record's protection is
/// cleared by a foreign answer.
#[tokio::test]
async fn a_mismatched_promotion_receipt_keeps_the_record_dependency_live() {
    let env = Env::boot("R3 Promotion Receipt Owner").await;
    let provider = Provider::open().await;
    let rewrite = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn RemoteMemoryService> = Arc::new(MismatchedPromotionReceipt {
        inner: provider.inner.server(provider.directory.admission()),
        rewrite: Arc::clone(&rewrite),
    });
    let (core, responder) = provider
        .open_core_service(&env, service, CoreAccess::EngineOwner)
        .await;
    let principal = core.active_principal().await.expect("active principal");

    let world = create_world(&core, &principal, "R3 Promotion Receipt World").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "R3 Receipt Ada", &world).await;
    let subject_id = legacy::character_subject_id(&character_id);

    let records = character_review_publishes_binding_local_fragments(
        &core,
        &principal,
        &provider,
        &character_id,
        &binding_id,
        &subject_id,
    )
    .await;
    let record_id = records[0].record_id.clone();

    let nexus = core_db(&env).await;
    let before = object_facts(&nexus, &binding_id).await;
    assert!(
        before.contains(&("record".to_string(), record_id.clone(), "live".to_string())),
        "the reviewed record starts as a live dependency: {before:?}"
    );
    nexus.close().await;

    // A receipt for another request: the mismatch fails closed, statically.
    rewrite.store(true, Ordering::SeqCst);
    let error = core
        .promote_character_fragment(&principal, character_id.clone(), record_id.clone(), 0)
        .await
        .expect_err("a mismatched promotion receipt refuses the promotion");
    match &error {
        CoreError::Internal { category } => assert!(
            category.starts_with("memory_promote_receipt"),
            "the static promotion-receipt refusal, got {category}"
        ),
        other => panic!("expected the static promote-receipt refusal, got {other:?}"),
    }

    let nexus = core_db(&env).await;
    let after = object_facts(&nexus, &binding_id).await;
    assert_eq!(after, before, "a mismatch releases no record dependency");
    nexus.close().await;

    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str()),
        "the refused promotion travelled the signed session of the loaded credential"
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
}

#[tokio::test]
async fn character_fragment_promotion_runs_on_the_remote_provider() {
    let env = Env::boot("R3 Promotion Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");

    let world = create_world(&core, &principal, "R3 Promotion World").await;
    let (character_id, binding_id, character_revision) =
        create_character(&core, &principal, "R3 Ada", &world).await;
    let subject_id = legacy::character_subject_id(&character_id);

    let (trap, dirs) = arm_legacy_traps(&env, Some(&character_id)).await;

    let records = character_review_publishes_binding_local_fragments(
        &core,
        &principal,
        &provider,
        &character_id,
        &binding_id,
        &subject_id,
    )
    .await;

    let (first, second) = binding_local_fragment_list_matches_records(
        &core,
        &principal,
        &env,
        &character_id,
        &binding_id,
        &records,
    )
    .await;

    let harness = FamilyHarness {
        env: &env,
        core: &core,
        principal: &principal,
        provider: &provider,
        character_id: &character_id,
    };
    promoting_the_first_fragment_shares_it_and_releases_its_fact(
        &harness,
        &binding_id,
        &first,
        &second,
    )
    .await;

    // A second writer changes the other fragment between the caller's read and
    // its revision-checked write: the caller's DTO revision 0 is now stale.
    let (adapter, sdk_responder) =
        second_writer_edit_makes_the_caller_revision_stale(&provider, &second).await;

    stale_and_archived_promotions_are_refused(
        &core,
        &principal,
        &provider,
        &character_id,
        &first,
        &second,
        character_revision,
    )
    .await;

    adapter.close();
    responder_of(sdk_responder).await;

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

/// Two binding-local captures drained by the real review pipeline become two
/// binding-local `fact` fragments on the provider.
async fn character_review_publishes_binding_local_fragments(
    core: &CoreService,
    principal: &Principal,
    provider: &Provider,
    character_id: &str,
    binding_id: &str,
    subject_id: &str,
) -> Vec<ProviderRecord> {
    for pending_id in ["pend_r3_a", "pend_r3_b"] {
        capture(core, principal, character_id, binding_id, pending_id)
            .await
            .expect("capture into the binding scope");
    }
    let reviewed = core
        .review_character_memory(
            principal,
            character_id.to_owned(),
            dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_id })),
        )
        .await
        .expect("review drains the binding scope");
    assert_eq!(reviewed.fragmented, 2, "both candidates became fragments");
    assert_eq!(reviewed.promoted, 0);
    assert_eq!(reviewed.dropped, 0);

    let pool = provider.pool().await;
    let records = provider_records(&pool, subject_id).await;
    assert_eq!(records.len(), 2, "two durable fragments: {records:?}");
    for record in &records {
        assert_eq!(record.kind, "fact");
        assert_eq!(record.visibility, "context");
        assert_eq!(
            record.context_id.as_deref(),
            Some(binding_id),
            "the fragment inherits the candidate's binding-local placement"
        );
        assert_eq!(record.revision, 1);
        assert!(!record.tombstoned);
    }
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.review").await,
        2,
        "each candidate was consumed by its own review"
    );
    pool.close().await;
    records
}

/// The binding-local fragment listing matches the durable record ids, and the
/// committed review published each record as a live D18 dependency.
async fn binding_local_fragment_list_matches_records(
    core: &CoreService,
    principal: &Principal,
    env: &Env,
    character_id: &str,
    binding_id: &str,
    records: &[ProviderRecord],
) -> (String, String) {
    let fragments = core
        .list_character_memory_fragments(
            principal,
            character_id.to_owned(),
            Some(binding_id.to_owned()),
            50,
            0,
        )
        .await
        .expect("list the binding-local fragments");
    assert_eq!(fragments.fragments.len(), 2);
    let mut fragment_ids: Vec<String> = fragments
        .fragments
        .iter()
        .map(|fragment| fragment.fragment_id.as_str().to_string())
        .collect();
    fragment_ids.sort();
    let mut expected_ids: Vec<String> = records
        .iter()
        .map(|record| record.record_id.clone())
        .collect();
    expected_ids.sort();
    assert_eq!(fragment_ids, expected_ids);
    for fragment in &fragments.fragments {
        assert_eq!(
            fragment
                .binding_id
                .as_deref()
                .map(std::string::String::as_str),
            Some(binding_id)
        );
        assert_eq!(fragment.revision, 0, "the retained DTO 0-based revision");
        assert_ne!(fragment.summary, "");
    }
    let first = fragment_ids[0].clone();
    let second = fragment_ids[1].clone();

    let nexus = core_db(env).await;
    let facts = object_facts(&nexus, binding_id).await;
    for record_id in [&first, &second] {
        assert!(
            facts.contains(&("record".to_string(), record_id.clone(), "live".to_string())),
            "the review published this record's dependency: {facts:?}"
        );
    }
    nexus.close().await;
    (first, second)
}

/// Shared references for the rewritten/native family phases.
#[derive(Clone, Copy)]
struct FamilyHarness<'a> {
    env: &'a Env,
    core: &'a CoreService,
    principal: &'a Principal,
    provider: &'a Provider,
    character_id: &'a str,
}

/// Promoting the first fragment shares it, leaves the other fragment's
/// placement alone, releases the record's D18 dependency and makes a second
/// promotion the retained stable conflict.
async fn promoting_the_first_fragment_shares_it_and_releases_its_fact(
    harness: &FamilyHarness<'_>,
    binding_id: &str,
    first: &str,
    second: &str,
) {
    let FamilyHarness {
        env,
        core,
        principal,
        provider,
        character_id,
    } = *harness;
    let promoted = core
        .promote_character_fragment(principal, character_id.to_owned(), first.to_owned(), 0)
        .await
        .expect("promote the fragment");
    assert!(
        promoted.fragment.binding_id.is_none(),
        "the promoted fragment carries no binding placement"
    );
    assert_eq!(promoted.fragment.revision, 1);

    let pool = provider.pool().await;
    let promoted_row = provider_record(&pool, first).await.expect("the row");
    assert_eq!(promoted_row.visibility, "shared");
    assert_eq!(promoted_row.context_id, None);
    assert_eq!(
        promoted_row.revision, 2,
        "the promotion bumped the revision"
    );
    assert_eq!(provider_receipts(&pool, "memory.records.promote").await, 1);
    let untouched = provider_record(&pool, second).await.expect("the row");
    assert_eq!(
        (untouched.visibility.as_str(), untouched.context_id.clone()),
        ("context", Some(binding_id.to_owned())),
        "the other fragment keeps its placement"
    );
    pool.close().await;

    let nexus = core_db(env).await;
    let facts = object_facts(&nexus, binding_id).await;
    assert!(
        facts.contains(&(
            "record".to_string(),
            first.to_owned(),
            "released".to_string()
        )),
        "the promotion receipt released the record's dependency: {facts:?}"
    );
    nexus.close().await;

    let already = core
        .promote_character_fragment(principal, character_id.to_owned(), first.to_owned(), 1)
        .await
        .expect_err("an already shared fragment is a stable conflict");
    assert!(
        matches!(&already, CoreError::ActorConflict { code, .. } if code == "character_fragment_already_shared"),
        "got {already:?}"
    );
}

/// A second signed writer edits the other fragment, bumping the provider
/// revision the caller's stale DTO read still names. Returns the live session.
async fn second_writer_edit_makes_the_caller_revision_stale(
    provider: &Provider,
    second: &str,
) -> (
    Arc<RemoteAdapter>,
    tokio::task::JoinHandle<Arc<ConnectResponder>>,
) {
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = Provider::memory_principal();
    let stored = match adapter
        .get_record(
            &memory_principal,
            dto(json!({
                "space_id": provider.space_id,
                "record_id": second,
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("record body"),
        SpokeResult::Reject(reject) => panic!("the fragment is readable: {reject:?}"),
    };
    let mut content = stored["record"]["content"].clone();
    content["text"] = json!("A second writer rewrote this fragment's body.");
    let edited = adapter
        .update_record(
            &memory_principal,
            dto::<MemoryRecordsUpdateRequest>(json!({
                "space_id": provider.space_id,
                "record_id": second,
                "expected_revision": stored["record"]["revision"],
                "content": content,
                "idempotency_key": "r3-stale-fragment-edit",
            })),
        )
        .await;
    match edited {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the second writer's update commits: {reject:?}"),
    }
    let pool = provider.pool().await;
    assert_eq!(
        provider_record(&pool, second)
            .await
            .expect("the row")
            .revision,
        2,
        "the second writer bumped the revision"
    );
    pool.close().await;
    (adapter, sdk_responder)
}

/// A stale DTO revision is refused before any provider call; the current
/// revision promotes; the archived Character's activity lease refuses the next
/// promotion without reaching the provider.
async fn stale_and_archived_promotions_are_refused(
    core: &CoreService,
    principal: &Principal,
    provider: &Provider,
    character_id: &str,
    first: &str,
    second: &str,
    character_revision: i64,
) {
    let conflict = core
        .promote_character_fragment(principal, character_id.to_owned(), second.to_owned(), 0)
        .await
        .expect_err("the stale revision is refused before the provider call");
    assert!(
        matches!(&conflict, CoreError::ActorConflict { code, .. } if code == "version_mismatch"),
        "got {conflict:?}"
    );
    let pool = provider.pool().await;
    let refused_row = provider_record(&pool, second).await.expect("the row");
    assert_eq!(
        (refused_row.visibility.as_str(), refused_row.revision),
        ("context", 2),
        "the refused promotion wrote nothing"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        1,
        "the refused promotion reached no provider operation"
    );
    pool.close().await;

    let retried = core
        .promote_character_fragment(principal, character_id.to_owned(), second.to_owned(), 1)
        .await
        .expect("promote the current revision");
    assert!(retried.fragment.binding_id.is_none());
    let pool = provider.pool().await;
    let retried_row = provider_record(&pool, second).await.expect("the row");
    assert_eq!(retried_row.visibility, "shared");
    assert_eq!(retried_row.context_id, None);
    assert_eq!(retried_row.revision, 3);
    assert_eq!(provider_receipts(&pool, "memory.records.promote").await, 2);
    pool.close().await;

    // The per-Character activity lease is this branch's outer admission: an
    // archived Character is refused there, before any provider read or write.
    core.transition_character(
        principal,
        dto::<CoreCharacterTransitionRequest>(json!({
            "character_id": character_id,
            "expected_revision": character_revision,
            "target_status": "archived",
        })),
    )
    .await
    .expect("archive the Character");
    let inactive = core
        .promote_character_fragment(principal, character_id.to_owned(), first.to_owned(), 1)
        .await
        .expect_err("an archived Character refuses the promotion at its activity lease");
    assert!(
        matches!(&inactive, CoreError::ActorConflict { code, .. } if code == "character_inactive"),
        "got {inactive:?}"
    );
    let pool = provider.pool().await;
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        2,
        "the lease refusal reached no provider operation"
    );
    assert_eq!(
        provider_record(&pool, first)
            .await
            .expect("the row")
            .visibility,
        "shared",
        "the lease refusal left the promoted record alone"
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------
// The Character fragment family under a real whole-body rewrite
// ---------------------------------------------------------------------------

/// N2's adoption maps a reviewed `FragmentOnly` product **to** a `fact` record;
/// it never makes every protocol `fact` a Nexus Character fragment, and it never
/// lets a user lose a fragment silently. This case drives the real signed public
/// Core against an authenticated second writer that replaces a *known reviewed*
/// fragment's whole content (removing `nexus_legacy`) and against a
/// provider-native `fact` created straight through the protocol:
///
/// - the known fragment is damaged, not reclassified: LIST and by-id promotion
///   both fail honestly, while its live D18 fact still proves its origin; the
///   genuinely unproven native fact stays a generic record that is never served
///   or promoted as a fragment;
/// - every refusal happens BEFORE any provider mutation — placement, revision,
///   promote receipts and D18 facts stay exactly as they were;
/// - a valid fragment still promotes, and its response is already sound when a
///   later writer rewrites the record, so a committed promotion never turns into
///   a projection failure.
#[tokio::test]
async fn rewritten_and_provider_native_facts_are_not_character_fragments() {
    let env = Env::boot("R3 Family Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");

    let world = create_world(&core, &principal, "R3 Family World").await;
    let (character_id, binding_id, _) = create_character(&core, &principal, "R3 Bea", &world).await;
    let subject_id = legacy::character_subject_id(&character_id);

    let (rewritten, control, control_session) =
        family_review_and_listing(&core, &principal, &character_id, &binding_id).await;

    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = Provider::memory_principal();

    // A generic provider-native `fact` under the same subject must never be
    // served as a fragment.
    let native_id = provider_native_fact_is_not_a_family_row(
        &core,
        &principal,
        &adapter,
        &provider,
        &character_id,
        &binding_id,
        &subject_id,
    )
    .await;

    // Capture the provider's own state before the refusals.
    let facts_before =
        family_refusal_baseline(&env, &provider, &rewritten, &native_id, &binding_id).await;

    let native_refused = core
        .promote_character_fragment(&principal, character_id.clone(), native_id.clone(), 0)
        .await
        .expect_err("a provider-native fact is not a Character fragment");
    assert!(
        matches!(&native_refused, CoreError::ActorInput(_)),
        "got {native_refused:?}"
    );

    let harness = FamilyHarness {
        env: &env,
        core: &core,
        principal: &principal,
        provider: &provider,
        character_id: &character_id,
    };
    damaged_known_fragment_is_reported_not_hidden(
        &harness,
        &adapter,
        &memory_principal,
        &rewritten,
        &native_id,
        &binding_id,
        &facts_before,
    )
    .await;

    valid_fragment_promotes_and_stays_representable(
        &harness,
        &adapter,
        &memory_principal,
        &control,
        &control_session,
    )
    .await;

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

/// Two reviewed fragments whose producer-recorded sessions identify the known
/// family; returns the rewritten row, the control row and its session.
async fn family_review_and_listing(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
) -> (String, String, String) {
    for pending_id in ["pend_family_a", "pend_family_b"] {
        capture(core, principal, character_id, binding_id, pending_id)
            .await
            .expect("capture into the binding scope");
    }
    let reviewed = core
        .review_character_memory(
            principal,
            character_id.to_owned(),
            dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_id })),
        )
        .await
        .expect("review drains the binding scope");
    assert_eq!(reviewed.fragmented, 2);

    let listed = core
        .list_character_memory_fragments(
            principal,
            character_id.to_owned(),
            Some(binding_id.to_owned()),
            50,
            0,
        )
        .await
        .expect("list the family");
    assert_eq!(listed.fragments.len(), 2);
    let mut sessions: Vec<String> = listed
        .fragments
        .iter()
        .map(|fragment| fragment.session_id.as_str().to_string())
        .collect();
    sessions.sort();
    assert_eq!(
        sessions,
        vec![
            "sess_pend_family_a".to_string(),
            "sess_pend_family_b".to_string()
        ],
        "both fragments carry the session identity their producer recorded"
    );
    let mut fragment_ids: Vec<String> = listed
        .fragments
        .iter()
        .map(|fragment| fragment.fragment_id.as_str().to_string())
        .collect();
    fragment_ids.sort();
    let rewritten = fragment_ids[0].clone();
    let control = fragment_ids[1].clone();
    let control_session = listed
        .fragments
        .iter()
        .find(|fragment| fragment.fragment_id.as_str() == control)
        .map(|fragment| fragment.session_id.as_str().to_string())
        .expect("the control fragment is listed");
    (rewritten, control, control_session)
}

/// A generic provider-native `fact` under the same subject is never served as a
/// fragment and never consumes a family slot; returns its provider id.
async fn provider_native_fact_is_not_a_family_row(
    core: &CoreService,
    principal: &Principal,
    adapter: &Arc<RemoteAdapter>,
    provider: &Provider,
    character_id: &str,
    binding_id: &str,
    subject_id: &str,
) -> String {
    let memory_principal = Provider::memory_principal();
    let native = match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "kind": "fact",
                "content": { "text": "A provider-native fact with no Nexus fragment origin." },
                "scope": { "visibility": "context", "context_id": binding_id },
                "idempotency_key": "r3-family-native",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("create body"),
        SpokeResult::Reject(reject) => panic!("the native fact is created: {reject:?}"),
    };
    let native_id = native["result"]["record_id"]
        .as_str()
        .expect("the provider-assigned record id")
        .to_string();

    let listed = core
        .list_character_memory_fragments(
            principal,
            character_id.to_owned(),
            Some(binding_id.to_owned()),
            50,
            0,
        )
        .await
        .expect("list the family with the native fact present");
    assert_eq!(
        listed.fragments.len(),
        2,
        "the native fact never consumes a family slot"
    );
    assert!(
        !listed
            .fragments
            .iter()
            .any(|fragment| fragment.fragment_id.as_str() == native_id),
        "the native fact is not served as a fragment"
    );
    native_id
}

/// The provider rows, promote receipt and D18 facts before the refusals.
async fn family_refusal_baseline(
    env: &Env,
    provider: &Provider,
    rewritten: &str,
    native_id: &str,
    binding_id: &str,
) -> Vec<(String, String, String)> {
    let pool = provider.pool().await;
    let rewritten_before = provider_record(&pool, rewritten)
        .await
        .expect("the rewritten row");
    let native_before = provider_record(&pool, native_id)
        .await
        .expect("the native row");
    assert_eq!(
        (
            rewritten_before.visibility.as_str(),
            rewritten_before.revision
        ),
        ("context", 1)
    );
    assert_eq!(
        (native_before.visibility.as_str(), native_before.revision),
        ("context", 1)
    );
    assert_eq!(provider_receipts(&pool, "memory.records.promote").await, 0);
    pool.close().await;
    let nexus = core_db(env).await;
    let facts_before = object_facts(&nexus, binding_id).await;
    nexus.close().await;
    assert!(
        facts_before.contains(&(
            "record".to_string(),
            rewritten.to_owned(),
            "live".to_string()
        )),
        "the review published the known fragment's dependency: {facts_before:?}"
    );
    facts_before
}

/// The authenticated writer strips the known fragment's identity: it is reported
/// as damage rather than hidden, and both refusals leave provider and D18 state
/// untouched.
async fn damaged_known_fragment_is_reported_not_hidden(
    harness: &FamilyHarness<'_>,
    adapter: &Arc<RemoteAdapter>,
    memory_principal: &MemoryPrincipal,
    rewritten: &str,
    native_id: &str,
    binding_id: &str,
    facts_before: &[(String, String, String)],
) {
    let FamilyHarness {
        env,
        core,
        principal,
        provider,
        character_id,
    } = *harness;
    let whole_rewrite = adapter
        .update_record(
            memory_principal,
            dto::<MemoryRecordsUpdateRequest>(json!({
                "space_id": provider.space_id,
                "record_id": rewritten,
                "expected_revision": 1,
                "content": { "text": "A whole-body rewrite that carries no Nexus metadata." },
                "idempotency_key": "r3-family-rewrite",
            })),
        )
        .await;
    match whole_rewrite {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the rewrite commits: {reject:?}"),
    }

    let listed = core
        .list_character_memory_fragments(
            principal,
            character_id.to_owned(),
            Some(binding_id.to_owned()),
            50,
            0,
        )
        .await
        .expect_err("a known fragment that lost its identity is reported, not hidden");
    assert!(
        matches!(&listed, CoreError::Internal { category } if category.contains("memory_fragment_metadata")),
        "got {listed:?}"
    );
    let rewritten_refused = core
        .promote_character_fragment(principal, character_id.to_owned(), rewritten.to_owned(), 1)
        .await
        .expect_err("the damaged known fragment refuses the promotion");
    assert!(
        matches!(&rewritten_refused, CoreError::Internal { category } if category.contains("memory_fragment_metadata")),
        "got {rewritten_refused:?}"
    );

    let pool = provider.pool().await;
    let rewritten_after = provider_record(&pool, rewritten)
        .await
        .expect("the rewritten row");
    assert_eq!(
        (
            rewritten_after.visibility.as_str(),
            rewritten_after.revision
        ),
        ("context", 2),
        "the refused promotion left the damaged record untouched"
    );
    let native_after = provider_record(&pool, native_id)
        .await
        .expect("the native row");
    assert_eq!(
        (
            native_after.visibility.as_str(),
            native_after.context_id.as_deref(),
            native_after.revision
        ),
        ("context", Some(binding_id), 1),
        "the refused promotion left the provider-native fact untouched"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        0,
        "neither refusal reached a provider operation"
    );
    pool.close().await;
    let nexus = core_db(env).await;
    assert_eq!(
        object_facts(&nexus, binding_id).await,
        facts_before,
        "neither refusal changed a D18 dependency fact"
    );
    nexus.close().await;
}

/// The untouched fragment promotes and is still served on the shared scope; a
/// later whole-body rewrite makes it unproven (`ActorInput`), never Internal, and
/// the committed promotion keeps its receipt and row.
async fn valid_fragment_promotes_and_stays_representable(
    harness: &FamilyHarness<'_>,
    adapter: &Arc<RemoteAdapter>,
    memory_principal: &MemoryPrincipal,
    control: &str,
    control_session: &str,
) {
    let FamilyHarness {
        core,
        principal,
        provider,
        character_id,
        ..
    } = *harness;
    let promoted = core
        .promote_character_fragment(principal, character_id.to_owned(), control.to_owned(), 0)
        .await
        .expect("the valid fragment promotes");
    assert!(promoted.fragment.binding_id.is_none());
    assert_eq!(promoted.fragment.revision, 1);
    assert_eq!(promoted.fragment.session_id.as_str(), control_session);
    // A valid shared fragment stays served: its session identity is the
    // representation, not a binding-scoped fact.
    let shared = core
        .list_character_memory_fragments(principal, character_id.to_owned(), None, 50, 0)
        .await
        .expect("the shared scope serves the promoted fragment");
    assert!(
        shared
            .fragments
            .iter()
            .any(|fragment| fragment.fragment_id.as_str() == control),
        "the promoted fragment is still a fragment on the shared scope"
    );
    let post_promotion_rewrite = adapter
        .update_record(
            memory_principal,
            dto::<MemoryRecordsUpdateRequest>(json!({
                "space_id": provider.space_id,
                "record_id": control,
                "expected_revision": 2,
                "content": { "text": "a post-promotion whole-body rewrite" },
                "idempotency_key": "r3-family-post-promotion",
            })),
        )
        .await;
    match post_promotion_rewrite {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the post-promotion rewrite commits: {reject:?}"),
    }
    // The promoted record is shared, so no binding scopes its origin facts: it is
    // unproven rather than falsely claimed as known, and still never an Internal.
    let retried = core
        .promote_character_fragment(principal, character_id.to_owned(), control.to_owned(), 1)
        .await
        .expect_err("the rewritten shared record is not a fragment");
    assert!(
        matches!(&retried, CoreError::ActorInput(_)),
        "got {retried:?}"
    );
    assert!(
        !matches!(&retried, CoreError::Internal { .. }),
        "a committed promotion never becomes a projection failure: {retried:?}"
    );
    let pool = provider.pool().await;
    let control_row = provider_record(&pool, control).await.expect("the row");
    assert_eq!(
        (control_row.visibility.as_str(), control_row.revision),
        ("shared", 3)
    );
    assert_eq!(provider_receipts(&pool, "memory.records.promote").await, 1);
    pool.close().await;
}

// ---------------------------------------------------------------------------
// The fragment page window at the real consumer boundary
// ---------------------------------------------------------------------------

/// The page window is counted in FAMILY rows. Interleaved generic records — a
/// provider-native `fact`, and a `fact` whose own metadata names the *unrelated*
/// `memory_long_term` family — never shift `offset`, `has_more` or a page, and
/// the walk stops once the page and its `has_more` proof are settled: an older
/// genuine Character-fragment damage a small first page never reaches cannot fail
/// it, while a listing wide enough to reach that row fails honestly. Both scopes
/// are real bindings of one owned Character.
#[tokio::test]
async fn fragment_page_windows_count_only_the_family() {
    let env = Env::boot("R3 Window Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");
    let world_a = create_world(&core, &principal, "R3 Window World A").await;
    let world_b = create_world(&core, &principal, "R3 Window World B").await;
    let (character_id, binding_a, _) =
        create_character(&core, &principal, "R3 Fay", &world_a).await;
    let binding_b = add_binding(&core, &principal, &character_id, &world_b).await;
    let subject_id = legacy::character_subject_id(&character_id);

    // Binding A: three reviewed fragments.
    review_family_binding(
        &core,
        &principal,
        &character_id,
        &binding_a,
        &["pend_window_a", "pend_window_b", "pend_window_c"],
        3,
    )
    .await;

    let (adapter, sdk_responder) = provider.sdk_session().await;
    let (native_id, unrelated_id) =
        create_generic_family_records(&adapter, &provider, &subject_id, &binding_a).await;

    // The page window counts family rows only: the interleaved native fact is
    // never served and never consumes a slot, so `offset`/`has_more` describe
    // the three fragments alone.
    family_pages_skip_generic_records(
        &core,
        &principal,
        &character_id,
        &binding_a,
        &native_id,
        &unrelated_id,
    )
    .await;

    // By id the unrelated-family record keeps the retained non-fragment answer.
    unrelated_family_promotion_is_refused(
        &core,
        &principal,
        &provider,
        &character_id,
        &unrelated_id,
    )
    .await;

    // Binding B: a DAMAGED fragment-origin record older than two valid
    // fragments, so a small first page never reaches it.
    let damaged_id =
        create_damaged_fragment_record(&adapter, &provider, &subject_id, &binding_b).await;
    review_family_binding(
        &core,
        &principal,
        &character_id,
        &binding_b,
        &["pend_window_d", "pend_window_e"],
        2,
    )
    .await;
    pages_and_promotion_report_the_damaged_fragment(
        &core,
        &principal,
        &provider,
        &character_id,
        &binding_b,
        &damaged_id,
    )
    .await;

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

/// Capture and review `pending_ids` into one binding, asserting the fragmented
/// count the real review pipeline reports.
async fn review_family_binding(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
    pending_ids: &[&str],
    expected_fragmented: i64,
) {
    for pending_id in pending_ids {
        capture(core, principal, character_id, binding_id, pending_id)
            .await
            .expect("capture into the binding");
    }
    let reviewed = core
        .review_character_memory(
            principal,
            character_id.to_owned(),
            dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_id })),
        )
        .await
        .expect("review the binding");
    assert_eq!(reviewed.fragmented, expected_fragmented);
}

/// A provider-native fact and an unrelated-family fact, both under the same
/// subject and binding; returns their provider ids.
async fn create_generic_family_records(
    adapter: &Arc<RemoteAdapter>,
    provider: &Provider,
    subject_id: &str,
    binding_id: &str,
) -> (String, String) {
    let memory_principal = Provider::memory_principal();
    let native = match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "kind": "fact",
                "content": { "text": "a provider-native fact with no Nexus fragment origin" },
                "scope": { "visibility": "context", "context_id": binding_id },
                "idempotency_key": "r3-window-native",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("create body"),
        SpokeResult::Reject(reject) => panic!("the native fact is created: {reject:?}"),
    };
    let native_id = native["result"]["record_id"]
        .as_str()
        .expect("the provider-assigned record id")
        .to_string();
    // A legal generic `fact` naming the UNRELATED `memory_long_term` family: a
    // family string that is not the Character one is not Character-fragment
    // origin, so this must stay a generic record.
    let unrelated = match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "kind": "fact",
                "content": {
                    "text": "a generic fact of the unrelated long-term family",
                    "structured": { "nexus_legacy": { "family": "memory_long_term" } },
                },
                "scope": { "visibility": "context", "context_id": binding_id },
                "idempotency_key": "r3-window-unrelated-family",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("create body"),
        SpokeResult::Reject(reject) => panic!("the unrelated-family fact is created: {reject:?}"),
    };
    let unrelated_id = unrelated["result"]["record_id"]
        .as_str()
        .expect("the provider-assigned record id")
        .to_string();
    (native_id, unrelated_id)
}

/// The page window counts family rows only: generic records never consume a slot
/// or get served as fragments.
async fn family_pages_skip_generic_records(
    core: &CoreService,
    principal: &Principal,
    character_id: &str,
    binding_id: &str,
    native_id: &str,
    unrelated_id: &str,
) {
    let mut served: Vec<(String, String)> = Vec::new();
    for (offset, expected_more) in [(0u32, true), (1, true), (2, false)] {
        let page = core
            .list_character_memory_fragments(
                principal,
                character_id.to_owned(),
                Some(binding_id.to_owned()),
                1,
                offset,
            )
            .await
            .expect("one family page");
        assert_eq!(
            page.fragments.len(),
            1,
            "offset {offset} serves one family row"
        );
        assert_eq!(
            page.pagination.has_more, expected_more,
            "offset {offset} reports the family's remainder"
        );
        served.push((
            page.fragments[0].fragment_id.as_str().to_string(),
            page.fragments[0].session_id.as_str().to_string(),
        ));
    }
    let mut sessions: Vec<String> = served.iter().map(|(_, session)| session.clone()).collect();
    sessions.sort();
    assert_eq!(
        sessions,
        vec![
            "sess_pend_window_a".to_string(),
            "sess_pend_window_b".to_string(),
            "sess_pend_window_c".to_string()
        ],
        "each page serves a real reviewed fragment"
    );
    assert!(
        !served.iter().any(|(record, _)| record == native_id),
        "the provider-native fact is never served as a fragment"
    );
    assert!(
        !served.iter().any(|(record, _)| record == unrelated_id),
        "an unrelated legacy family is never served as a fragment"
    );
}

/// By id an unrelated legacy family is the retained non-fragment refusal, and it
/// writes nothing.
async fn unrelated_family_promotion_is_refused(
    core: &CoreService,
    principal: &Principal,
    provider: &Provider,
    character_id: &str,
    unrelated_id: &str,
) {
    let pool = provider.pool().await;
    let unrelated_before = provider_record(&pool, unrelated_id)
        .await
        .expect("the unrelated-family row");
    assert_eq!(
        (
            unrelated_before.visibility.as_str(),
            unrelated_before.revision
        ),
        ("context", 1)
    );
    assert_eq!(provider_receipts(&pool, "memory.records.promote").await, 0);
    pool.close().await;
    let unrelated_refused = core
        .promote_character_fragment(
            principal,
            character_id.to_owned(),
            unrelated_id.to_owned(),
            0,
        )
        .await
        .expect_err("an unrelated legacy family is not a Character fragment");
    assert!(
        matches!(&unrelated_refused, CoreError::ActorInput(_)),
        "got {unrelated_refused:?}"
    );
    let pool = provider.pool().await;
    assert_eq!(
        provider_record(&pool, unrelated_id)
            .await
            .expect("the unrelated-family row")
            .revision,
        1,
        "the retained refusal wrote nothing"
    );
    assert_eq!(provider_receipts(&pool, "memory.records.promote").await, 0);
    pool.close().await;
}

/// Create an older fragment-origin record that lacks its session identity.
async fn create_damaged_fragment_record(
    adapter: &Arc<RemoteAdapter>,
    provider: &Provider,
    subject_id: &str,
    binding_id: &str,
) -> String {
    let memory_principal = Provider::memory_principal();
    let damaged = match adapter
        .create_record(
            &memory_principal,
            dto::<MemoryRecordsCreateRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "kind": "fact",
                "content": {
                    "text": "an older fragment-origin record without its session identity",
                    "structured": { "nexus_legacy": {
                        "family": "character_memory_fragments",
                        "fragment_id": "frag_window_damaged",
                    } },
                },
                "scope": { "visibility": "context", "context_id": binding_id },
                "idempotency_key": "r3-window-damaged",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("create body"),
        SpokeResult::Reject(reject) => panic!("the damaged record is created: {reject:?}"),
    };
    damaged["result"]["record_id"]
        .as_str()
        .expect("the provider-assigned record id")
        .to_string()
}

/// A small first page never reaches the older damaged row; a wide listing and
/// the by-id promotion report it honestly and write nothing.
async fn pages_and_promotion_report_the_damaged_fragment(
    core: &CoreService,
    principal: &Principal,
    provider: &Provider,
    character_id: &str,
    binding_id: &str,
    damaged_id: &str,
) {
    let first_page = core
        .list_character_memory_fragments(
            principal,
            character_id.to_owned(),
            Some(binding_id.to_owned()),
            1,
            0,
        )
        .await
        .expect("a first page that never reaches the older damaged row");
    assert_eq!(first_page.fragments.len(), 1);
    assert!(
        first_page.pagination.has_more,
        "the second fragment is past the first page"
    );

    let wide = core
        .list_character_memory_fragments(
            principal,
            character_id.to_owned(),
            Some(binding_id.to_owned()),
            50,
            0,
        )
        .await
        .expect_err("a listing that reaches the damaged row is honest");
    assert!(
        matches!(&wide, CoreError::Internal { category } if category.contains("memory_fragment_metadata")),
        "got {wide:?}"
    );
    let refused = core
        .promote_character_fragment(principal, character_id.to_owned(), damaged_id.to_owned(), 0)
        .await
        .expect_err("the damaged record cannot be promoted");
    assert!(
        matches!(&refused, CoreError::Internal { category } if category.contains("memory_fragment_metadata")),
        "got {refused:?}"
    );

    let pool = provider.pool().await;
    assert_eq!(
        provider_record(&pool, damaged_id)
            .await
            .expect("the damaged row")
            .revision,
        1,
        "the honest refusals wrote nothing"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        0,
        "no promotion receipt exists for the refusals"
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------
// Real provider contention and the during-call response history
// ---------------------------------------------------------------------------

/// One real causal window inside one Core call.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbeBoundary {
    /// The returned `memory.records.get`: between Core's validated read and the
    /// provider promotion CAS.
    AfterFragmentRead,
    /// The committed `memory.records.promote`: between the real commit and
    /// Core's response.
    AfterPromoteCommit,
}

impl ProbeBoundary {
    const fn op(self) -> &'static str {
        match self {
            Self::AfterFragmentRead => "memory.records.get",
            Self::AfterPromoteCommit => "memory.records.promote",
        }
    }

    const fn writer_key(self) -> &'static str {
        match self {
            Self::AfterFragmentRead => "r3-race-writer-read",
            Self::AfterPromoteCommit => "r3-race-writer-commit",
        }
    }
}

/// A test-only decorator over the provider's REAL assembled service that holds
/// exactly one armed response open.
///
/// Every request is forwarded unchanged to the inner service and the inner
/// response is always the one returned: this decorator never synthesizes a reply
/// and never writes anything itself. For the one armed request it captures that
/// response, signals the test and waits for the test's release, so an
/// independently established SDK session can compete at the actual boundary
/// while the original request is held.
struct HeldBoundaryProbe {
    inner: Arc<dyn RemoteMemoryService>,
    boundary: ProbeBoundary,
    target: Mutex<Option<String>>,
    armed: AtomicBool,
    held: AtomicBool,
    reached: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
}

impl HeldBoundaryProbe {
    fn over(inner: Arc<dyn RemoteMemoryService>, boundary: ProbeBoundary) -> Self {
        Self {
            inner,
            boundary,
            target: Mutex::new(None),
            armed: AtomicBool::new(false),
            held: AtomicBool::new(false),
            reached: Mutex::new(None),
            release: Mutex::new(None),
        }
    }

    /// Watch exactly this record's boundary request.
    fn watch(&self, record_id: &str) {
        *self.target.lock().expect("watched record") = Some(record_id.to_string());
    }

    /// Arm one hold with the channel ends the test observes.
    fn arm(&self, reached: oneshot::Sender<()>, release: oneshot::Receiver<()>) {
        *self.reached.lock().expect("reached slot") = Some(reached);
        *self.release.lock().expect("release slot") = Some(release);
        self.held.store(false, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Whether the armed request was actually held for the test.
    fn held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl RemoteMemoryService for HeldBoundaryProbe {
    async fn invoke(&self, peer_id: &str, op: &str, payload: Value) -> SpokeResult<Value> {
        let watched = self.target.lock().expect("watched record").clone();
        let record_id = payload
            .get("record_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let armed = op == self.boundary.op() && watched.as_deref() == record_id.as_deref();
        let response = self.inner.invoke(peer_id, op, payload).await;
        if armed && self.armed.swap(false, Ordering::SeqCst) {
            self.held.store(true, Ordering::SeqCst);
            let reached = self.reached.lock().expect("reached slot").take();
            if let Some(reached) = reached {
                let _ = reached.send(());
            }
            let release = self.release.lock().expect("release slot").take();
            if let Some(release) = release {
                let _ = release.await;
            }
        }
        response
    }
}

/// The `requester_principal_id` of the fixture's committed receipt for one
/// writer update, read straight out of the provider's own database.
async fn writer_receipt_principal(pool: &SqlitePool, idempotency_key: &str) -> String {
    sqlx::query(
        "SELECT requester_principal_id FROM receipts \
         WHERE op = 'memory.records.update' AND idempotency_key = ?",
    )
    .bind(idempotency_key)
    .fetch_one(pool)
    .await
    .expect("the writer's committed update receipt")
    .get(0)
}

/// Both required competing-client histories, each driven by a **separately
/// established real signed SDK session** while Core's own signed request is held
/// at the actual service boundary:
///
/// - after Core's validated read and before the provider promotion CAS: the
///   promotion must lose the real `REVISION_CONFLICT` (whitelisted carrier, no
///   promotion or D18 effect);
/// - after the provider commit and before the original response returns: the
///   original call must still answer truthfully, with its committed receipt and
///   D18 release.
///
/// The writer is a second `RemoteAdapter` on its own socket, handshake and
/// operator-provisioned admission under a distinct Ed25519 identity; the fixture
/// records the update under that principal. The decorator only forwards and
/// holds the real response — no synthetic reply, no reused Core context, no
/// direct inner write and no timing sleep.
#[tokio::test]
async fn an_independently_signed_writer_competes_at_both_causal_windows() {
    let env = Env::boot("R3 Race Owner").await;
    let provider = Provider::open().await;

    // The dedicated setup session creates two reviewed binding-local fragments.
    let race = race_setup_fragments(&env, &provider).await;
    let character_id = &race.character_id;
    let binding_id = &race.binding_id;
    let cas_record = &race.cas_record;
    let commit_record = &race.commit_record;

    // (a) Pre-CAS window: the writer advances the record after Core's validated
    // read and before the promotion, so the promotion meets the real provider
    // CAS. The provider/D18 observations run before any session cleanup.
    let (cas_refused, cas_handles) =
        run_writer_before_promotion_cas(&env, &provider, character_id, cas_record).await;
    assert_pre_cas_window_refusal(&env, &provider, cas_record, binding_id, &cas_refused).await;
    close_race_handles(&provider, cas_handles).await;

    // (b) Post-commit window: the writer rewrites the record after the promotion
    // committed and before the original response returns.
    let (promoted, commit_handles) =
        run_writer_after_promotion_commit(&env, &provider, character_id, commit_record).await;
    assert_committed_promotion_state(
        &env,
        &provider,
        commit_record,
        &race.commit_session,
        binding_id,
        &promoted,
    )
    .await;
    close_race_handles(&provider, commit_handles).await;
}

/// The reviewed fragments the race scenarios promote, with their sessions.
struct RaceSetup {
    character_id: String,
    binding_id: String,
    cas_record: String,
    commit_record: String,
    commit_session: String,
}

/// The dedicated setup session creates two reviewed binding-local fragments and
/// closes its own admission.
async fn race_setup_fragments(env: &Env, provider: &Provider) -> RaceSetup {
    let (setup, setup_responder) = provider.open_core(env).await;
    let principal = setup.active_principal().await.expect("active principal");
    let world = create_world(&setup, &principal, "R3 Race World").await;
    let (character_id, binding_id, _) =
        create_character(&setup, &principal, "R3 Dee", &world).await;
    for pending_id in ["pend_race_a", "pend_race_b"] {
        capture(&setup, &principal, &character_id, &binding_id, pending_id)
            .await
            .expect("capture into the binding scope");
    }
    setup
        .review_character_memory(
            &principal,
            character_id.clone(),
            dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_id })),
        )
        .await
        .expect("review drains the binding scope");
    let listed = setup
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            50,
            0,
        )
        .await
        .expect("list the family");
    assert_eq!(listed.fragments.len(), 2);
    let mut fragments: Vec<(String, String)> = listed
        .fragments
        .iter()
        .map(|fragment| {
            (
                fragment.fragment_id.as_str().to_string(),
                fragment.session_id.as_str().to_string(),
            )
        })
        .collect();
    fragments.sort();
    let (cas_record, _) = fragments[0].clone();
    let (commit_record, commit_session) = fragments[1].clone();
    let nexus = core_db(env).await;
    let facts = object_facts(&nexus, &binding_id).await;
    nexus.close().await;
    for record in [&cas_record, &commit_record] {
        assert!(
            facts.contains(&("record".to_string(), record.clone(), "live".to_string())),
            "the review published this record's dependency: {facts:?}"
        );
    }
    assert!(
        setup.close().await.expect("close").cleanup_confirmed,
        "the setup open releases its own admission"
    );
    responder_of(setup_responder).await;
    RaceSetup {
        character_id,
        binding_id,
        cas_record,
        commit_record,
        commit_session,
    }
}

/// The still-open owners of one raced window; closed only after the
/// provider/D18 observations, as the original inline ordering did.
struct RaceHandles {
    writer: Arc<RemoteAdapter>,
    core: CoreService,
    listeners: tokio::task::JoinHandle<Vec<Arc<ConnectResponder>>>,
}

/// Close the raced session owners after the observations. Every close/settle
/// check of the original inline flow is preserved.
async fn close_race_handles(provider: &Provider, handles: RaceHandles) {
    let RaceHandles {
        writer,
        core,
        listeners,
    } = handles;
    writer.close();
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the raced open releases its own admission"
    );
    let responders = tokio::time::timeout(BOUND, listeners)
        .await
        .expect("the two-connection listener is bounded")
        .expect("both signed sessions completed");
    let peers: Vec<String> = responders
        .iter()
        .filter_map(|responder| responder.remote_peer_id())
        .collect();
    assert!(
        peers.contains(&peer_id(&CLIENT_SEED)),
        "Core's own signed session served the call: {peers:?}"
    );
    assert!(
        peers.contains(&provider.writer_peer),
        "the independently signed writer session served the racing operation: {peers:?}"
    );
}

/// Run the pre-CAS window and return the refused promotion; the writer's update
/// commits between Core's validated read and the promotion.
async fn run_writer_before_promotion_cas(
    env: &Env,
    provider: &Provider,
    character_id: &str,
    cas_record: &str,
) -> (CoreError, RaceHandles) {
    let cas_probe = Arc::new(HeldBoundaryProbe::over(
        provider.inner.server(provider.directory.admission()),
        ProbeBoundary::AfterFragmentRead,
    ));
    cas_probe.watch(cas_record);
    let (cas_addr, cas_listeners) = provider
        .serve_connections(Arc::clone(&cas_probe) as Arc<dyn RemoteMemoryService>, 2)
        .await;
    let cas_core =
        CoreService::open(env.options(provider.selection(cas_addr), CoreAccess::EngineOwner))
            .await
            .expect("the selected remote provider opens");
    let cas_principal = cas_core.active_principal().await.expect("principal");
    let cas_writer = provider.dial(cas_addr, WRITER_SEED, "r3-race-writer").await;
    let writer_principal = provider.writer_memory_principal();
    let (cas_reached_tx, cas_reached_rx) = oneshot::channel();
    let (cas_release_tx, cas_release_rx) = oneshot::channel();
    cas_probe.arm(cas_reached_tx, cas_release_rx);
    let cas_call = cas_core.promote_character_fragment(
        &cas_principal,
        character_id.to_owned(),
        cas_record.to_owned(),
        0,
    );
    let (cas_refused, cas_written) = tokio::join!(cas_call, async {
        cas_reached_rx
            .await
            .expect("Core's validated read reached the armed boundary");
        let updated = cas_writer
            .update_record(
                &writer_principal,
                dto::<MemoryRecordsUpdateRequest>(json!({
                    "space_id": provider.space_id,
                    "record_id": cas_record,
                    "expected_revision": 1,
                    "content": { "text": "the independently signed writer advanced this record" },
                    "idempotency_key": ProbeBoundary::AfterFragmentRead.writer_key(),
                })),
            )
            .await;
        let committed = matches!(&updated, SpokeResult::Ok(_));
        let _ = cas_release_tx.send(());
        committed
    });
    assert!(
        cas_written,
        "the independently signed writer's update committed"
    );
    assert!(
        cas_probe.held(),
        "Core's validated read was held for the competing window"
    );
    let cas_refused = cas_refused.expect_err("the promotion loses the real provider CAS");
    (
        cas_refused,
        RaceHandles {
            writer: cas_writer,
            core: cas_core,
            listeners: cas_listeners,
        },
    )
}

/// The refused promotion is the real provider CAS, and neither provider nor D18
/// state moved.
async fn assert_pre_cas_window_refusal(
    env: &Env,
    provider: &Provider,
    cas_record: &str,
    binding_id: &str,
    cas_refused: &CoreError,
) {
    let (code, retry_class, family, wire_code, conflict_code) = classified_refusal(cas_refused);
    assert_eq!((code, retry_class), ("REVISION_CONFLICT", "terminal"));
    assert_eq!(family, MemoryProviderRefusal::Conflict);
    assert_eq!(wire_code, None);
    assert_eq!(conflict_code, Some("version_mismatch"));
    assert!(
        !format!("{cas_refused} {cas_refused:?}").contains("stored revision"),
        "the provider's own CAS message never travels"
    );
    let pool = provider.pool().await;
    let cas_row = provider_record(&pool, cas_record)
        .await
        .expect("the raced row");
    assert_eq!(
        (cas_row.visibility.as_str(), cas_row.revision),
        ("context", 2),
        "the provider CAS refused and the promotion wrote nothing"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        0,
        "the refused promotion committed no provider receipt"
    );
    assert_eq!(
        writer_receipt_principal(&pool, ProbeBoundary::AfterFragmentRead.writer_key()).await,
        provider.writer_principal,
        "the competing update was authenticated as the writer's own principal"
    );
    pool.close().await;
    let nexus = core_db(env).await;
    let after = object_facts(&nexus, binding_id).await;
    nexus.close().await;
    assert!(
        after.contains(&(
            "record".to_string(),
            cas_record.to_owned(),
            "live".to_string()
        )),
        "the refused promotion released nothing: {after:?}"
    );
    assert!(
        !after.contains(&(
            "record".to_string(),
            cas_record.to_owned(),
            "released".to_string()
        )),
        "no release fact was published for the refused promotion"
    );
}

/// Run the post-commit window and return the promotion's truthful response.
async fn run_writer_after_promotion_commit(
    env: &Env,
    provider: &Provider,
    character_id: &str,
    commit_record: &str,
) -> (
    nexus_contracts::generated::daemon_api::characters::memory::promote_character_fragment_response::PromoteCharacterFragmentResponse,
    RaceHandles,
){
    let commit_probe = Arc::new(HeldBoundaryProbe::over(
        provider.inner.server(provider.directory.admission()),
        ProbeBoundary::AfterPromoteCommit,
    ));
    commit_probe.watch(commit_record);
    let (commit_addr, commit_listeners) = provider
        .serve_connections(Arc::clone(&commit_probe) as Arc<dyn RemoteMemoryService>, 2)
        .await;
    let commit_core =
        CoreService::open(env.options(provider.selection(commit_addr), CoreAccess::EngineOwner))
            .await
            .expect("the selected remote provider opens");
    let commit_principal = commit_core.active_principal().await.expect("principal");
    let commit_writer = provider
        .dial(commit_addr, WRITER_SEED, "r3-race-writer")
        .await;
    let writer_principal = provider.writer_memory_principal();
    let (commit_reached_tx, commit_reached_rx) = oneshot::channel();
    let (commit_release_tx, commit_release_rx) = oneshot::channel();
    commit_probe.arm(commit_reached_tx, commit_release_rx);
    let commit_call = commit_core.promote_character_fragment(
        &commit_principal,
        character_id.to_owned(),
        commit_record.to_owned(),
        0,
    );
    let (commit_result, commit_written) = tokio::join!(commit_call, async {
        commit_reached_rx
            .await
            .expect("the promotion commit reached the armed boundary");
        let updated = commit_writer
            .update_record(
                &writer_principal,
                dto::<MemoryRecordsUpdateRequest>(json!({
                    "space_id": provider.space_id,
                    "record_id": commit_record,
                    "expected_revision": 2,
                    "content": { "text": "the independently signed writer rewrote this record" },
                    "idempotency_key": ProbeBoundary::AfterPromoteCommit.writer_key(),
                })),
            )
            .await;
        let committed = matches!(&updated, SpokeResult::Ok(_));
        let _ = commit_release_tx.send(());
        committed
    });
    assert!(
        commit_written,
        "the independently signed writer's post-commit rewrite committed"
    );
    assert!(
        commit_probe.held(),
        "the committed promotion response was held for the competing window"
    );
    let promoted = commit_result.expect("the committed promotion answers truthfully");
    (
        promoted,
        RaceHandles {
            writer: commit_writer,
            core: commit_core,
            listeners: commit_listeners,
        },
    )
}

/// The committed promotion answers truthfully: cleared placement, retained step,
/// its own receipt, the writer's authenticated post-commit rewrite and the D18
/// release.
async fn assert_committed_promotion_state(
    env: &Env,
    provider: &Provider,
    commit_record: &str,
    commit_session: &str,
    binding_id: &str,
    promoted: &nexus_contracts::generated::daemon_api::characters::memory::promote_character_fragment_response::PromoteCharacterFragmentResponse,
) {
    assert!(
        promoted.fragment.binding_id.is_none(),
        "the promotion cleared the binding placement"
    );
    assert_eq!(promoted.fragment.revision, 1, "the retained 0→1 step");
    assert_eq!(
        promoted.fragment.session_id.as_str(),
        commit_session,
        "the response states the identity the validated content carried"
    );
    let pool = provider.pool().await;
    let commit_row = provider_record(&pool, commit_record)
        .await
        .expect("the committed row");
    assert_eq!(
        (commit_row.visibility.as_str(), commit_row.revision),
        ("shared", 3),
        "the promotion committed and the writer's rewrite advanced the same record"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        1,
        "the committed promotion kept its single receipt"
    );
    assert_eq!(
        provider_content(&pool, commit_record).await["text"],
        json!("the independently signed writer rewrote this record"),
        "the writer's rewrite landed after the commit, before the response returned"
    );
    assert_eq!(
        writer_receipt_principal(&pool, ProbeBoundary::AfterPromoteCommit.writer_key()).await,
        provider.writer_principal,
        "the post-commit rewrite was authenticated as the writer's own principal"
    );
    pool.close().await;
    let nexus = core_db(env).await;
    let after = object_facts(&nexus, binding_id).await;
    nexus.close().await;
    assert!(
        after.contains(&(
            "record".to_string(),
            commit_record.to_owned(),
            "released".to_string()
        )),
        "the committed promotion released its dependency: {after:?}"
    );
}

// ---------------------------------------------------------------------------
// Recall and the pinned space
// ---------------------------------------------------------------------------

#[tokio::test]
async fn recall_and_record_reads_use_the_pinned_remote_space() {
    let env = Env::boot("R3 Recall Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");
    let subject_id = legacy::creator_subject_id(&env.creator_id);

    let (trap, dirs) = arm_legacy_traps(&env, None).await;

    assert!(core
        .create_creator_memory(&principal, SLUG, "story_summary", BODY)
        .await
        .expect("create the Creator memory"));
    let listed = core
        .list_creator_memories(&principal)
        .await
        .expect("list the Creator memories");
    assert_eq!(listed.len(), 1);
    let record_id = serde_json::to_value(&listed[0]).expect("record json")["record_id"]
        .as_str()
        .expect("the provider-assigned record id")
        .to_string();

    // Recall has no Core product caller on this slice, so it is driven over the
    // SDK's own signed session against the same durable provider.
    recall_over_the_signed_session(&provider, &subject_id, &record_id).await;

    // The durable write and its reads all carry the provider's own space.
    let pool = provider.pool().await;
    let space: String = sqlx::query("SELECT space_id FROM records WHERE record_id = ?")
        .bind(&record_id)
        .fetch_one(&pool)
        .await
        .expect("record space")
        .get::<String, _>("space_id");
    assert_eq!(space, provider.space_id);
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

/// The SDK's own signed session reads the created record and recalls the pinned
/// space; an unserved space is the definite absence. The session is closed.
async fn recall_over_the_signed_session(provider: &Provider, subject_id: &str, record_id: &str) {
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = Provider::memory_principal();

    let got = match adapter
        .get_record(
            &memory_principal,
            dto(json!({
                "space_id": provider.space_id,
                "record_id": record_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("record body"),
        SpokeResult::Reject(reject) => panic!("the created record is readable: {reject:?}"),
    };
    assert_eq!(got["record"]["record_id"], json!(record_id));
    assert_eq!(got["record"]["space_id"], json!(provider.space_id));
    assert_eq!(got["record"]["revision"], json!(1));

    let recalled = match adapter
        .recall(
            &memory_principal,
            dto::<MemoryRecallRequest>(json!({
                "space_id": provider.space_id,
                "subject_id": subject_id,
                "selection": { "mode": "subject_all" },
                "budget": { "max_items": 5 },
                "query": "alliance",
            })),
        )
        .await
    {
        SpokeResult::Ok(body) => serde_json::to_value(&body).expect("recall body"),
        SpokeResult::Reject(reject) => panic!("the pinned space is recalled: {reject:?}"),
    };
    assert!(
        recalled["records"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|record| record["record_id"].as_str() == Some(record_id)),
        "recall serves the record the product wrote in the pinned space: {recalled}"
    );
    assert!(
        recalled["records"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|record| record["space_id"] == json!(provider.space_id)),
        "every recalled record belongs to the pinned provider space"
    );
    assert!(
        recalled["total_authorized"].as_u64().unwrap_or(0) >= 1,
        "the authorized set is non-empty"
    );

    // A space this provider does not serve is its definite absence: the product's
    // pinned space is the provider's own, never a local default that would answer
    // here.
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
}

// ---------------------------------------------------------------------------
// Static, whitelisted refusals
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remote_record_refusals_stay_static_without_peer_text() {
    let env = Env::boot("R3 Refusal Owner").await;
    let provider = Provider::open().await;
    let (core, responder) = provider.open_core(&env).await;
    let principal = core.active_principal().await.expect("active principal");
    let subject_id = legacy::creator_subject_id(&env.creator_id);

    let (trap, dirs) = arm_legacy_traps(&env, None).await;

    // A body the record call accepts but the provider's own `max_content_bytes`
    // refuses: the refusal is genuinely the provider's, mapped by the safe
    // whitelist rather than copied through.
    let sentinel = "r3-peer-text-sentinel";
    let oversized = format!("{sentinel}{}", "x".repeat(OVER_LIMIT_BODY_BYTES));
    let refused = core
        .create_creator_memory(&principal, SLUG, "story_summary", &oversized)
        .await
        .expect_err("the provider's content bound refuses the body");
    let (refusal_code, retry_class, family, wire_code, conflict_code) =
        classified_refusal(&refused);
    assert_eq!((refusal_code, retry_class), ("INVALID_INPUT", "terminal"));
    assert_eq!(family, MemoryProviderRefusal::InvalidInput);
    assert_eq!(wire_code, None);
    assert_eq!(conflict_code, None);
    let shown = format!("{refused} | {refused:?}");
    assert!(
        !shown.contains(sentinel),
        "request content never travels with a refusal: {shown}"
    );
    assert!(
        !shown.contains("max_content_bytes"),
        "the provider's own message never travels with a refusal: {shown}"
    );

    // A record the provider has never seen is the proven definite absence, which
    // the retained slug read renders as `None` (the same whitelisted class drives
    // the shortcut), and a second miss writes nothing.
    assert!(core
        .get_creator_memory(&principal, "r3-never-written")
        .await
        .expect("the absent read is answered")
        .is_none());

    let pool = provider.pool().await;
    assert!(
        provider_records(&pool, &subject_id).await.is_empty(),
        "a refused create wrote no record"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.create").await,
        0,
        "a refused create committed no receipt"
    );
    pool.close().await;

    restore_legacy_tables(&trap).await;
    assert_no_local_memory(&env).await;
    assert_legacy_dir_traps(&dirs);

    assert_eq!(
        responder_of(responder).await.remote_peer_id().as_deref(),
        Some(peer_id(&CLIENT_SEED).as_str()),
        "the refusal travelled the signed session of the loaded credential"
    );
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the open releases its own admission"
    );
}
