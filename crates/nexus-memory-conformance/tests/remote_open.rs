//! R1 (v1.229 N3 Task 1): the real remote open of the selected memory provider.
//!
//! The test drives the public `CoreService::open` surface against a durable
//! `FixtureMemoryProvider` served through the SDK's `connect_responder` over a
//! real `127.0.0.1` socket, with the one product WS transport
//! (`nexus_core::connect::ws_transport::WsTransport`) on both ends. Nothing is
//! mocked: the credential file, the signed hello, `memory.describe`,
//! `memory.spaces.list`, the provider's admission table and the pinned binding
//! are the real ones.
//!
//! What this proves, and what it cannot:
//!
//! - The signed session admits the LOADED Ed25519 public peer identity: the
//!   fixture's admission table maps exactly one peer id — derived from the seed
//!   the credential file holds — to the operator's admin principal, and the
//!   responder reports that same peer id as the authenticated dialer. A dial
//!   with any other identity is refused, so the successful open is an
//!   admission observable, not a no-throw.
//! - The configured space must be discovered exactly once: a selection naming
//!   a space the provider does not serve fails with `memory_remote_space` even
//!   though the provider did advertise its own space (no first/default space
//!   is ever taken), and the real read afterwards runs in the pinned space.
//! - The negotiated family is required: a host whose signed hello advertises
//!   no memory family fails with `memory_remote_capability`.
//! - The space authority must be the authenticated host: a responder whose
//!   signed manifest names another host fails with `memory_remote_authority`.
//! - No local provider is ever constructed or substituted: every failure and
//!   the successful open leave the local provider's file root absent, and the
//!   remote core refuses legacy migration with the structured `not_supported`
//!   code while a seeded legacy bearer file stays byte-identical.
//!
//! `MemoryProviderBinding` is crate-private, so this crate-external test cannot
//! read the pinned `(space_id, authority_host_id, authority_generation,
//! client_identity)` tuple back. The open's own checks are what the code and
//! the L2 review inspect; the behavioural facts above are what is observable
//! here, and the pinned generation's use lands in the R2/R4 namespace evidence.

use std::collections::HashMap;
use std::fs;
use std::net::{Shutdown, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::protocol::Message;

use nexus_contracts::generated::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::generated::daemon_api::memory::create_pending_review_request::CreatePendingReviewRequest;
use nexus_contracts::{CoreRegisterCreatorRequest, SetActiveWorkspaceRequest};
use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::memory_provider::legacy;
use nexus_core::{
    CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreResult, CoreService,
    MemoryCredential, MemoryProviderRefusal, MemoryProviderSelection, Principal as CorePrincipal,
    RemoteMemoryProviderSelection,
};
use nexus_memory_conformance::{FixtureMemoryProvider, PeerDirectory};
use spoke_connect::core::{
    derive_peer_id_from_ed25519_pubkey, issue_capability_token, verify_capability_token,
    CapabilityClaims, CapabilityTokenProof,
};
use spoke_connect::remote::transport::{loopback_transport_pair, TransportError};
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponder, ConnectResponderOptions,
    ConnectResponderState, RemoteAdapter, RemoteAdapterOptions, RemoteIdentity, RemoteMemoryService,
    RemoteServePorts, RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, MemoryCaptureRequest,
    MemoryExportsCreateRequest, MemoryGrantsGrantRequest, MemoryReceiptsGetRequest,
    MemoryReflectionsStateRequest, Principal, Relation, Rule, Scope, TimelineEvent,
};
use spoke_operations::{
    FindingPort, HostManifestPort, KnowledgeEntryPort, MemoryPort, RelationPort, RuleQueryPort,
    ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult,
};

/// The host's Connect identity (the provider side of the signed session).
const HOST_SEED: [u8; 32] = [0x42; 32];
/// The dialing client's Connect identity. Its credential file holds exactly
/// this seed, so the public peer id the signed hello presents is derivable.
const CLIENT_SEED: [u8; 32] = [0x31; 32];
/// The one capability both ends negotiate: the fixture serves no other family.
const MEMORY_CAPABILITIES: [&str; 1] = ["spoke-memory"];
const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORKSPACE: &str = "default";
const CREDENTIAL_ID: &str = "cred-remote-open";
/// Bound on transport setup and teardown observation. Generous because the
/// suite runs several signed loopback sessions in parallel on a loaded host;
/// every wait is still bounded.
const SETUP_BOUND: Duration = Duration::from_secs(30);

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
    manifest_with(host_id, &MEMORY_CAPABILITIES)
}

/// A host that serves no memory family: its signed hello declares nothing this
/// client can negotiate.
fn manifest_without_memory(host_id: &str) -> HostCapabilityManifest {
    manifest_with(host_id, &[])
}

fn manifest_with(host_id: &str, capabilities: &[&str]) -> HostCapabilityManifest {
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
/// the signed responder's transport. `memory_service` is the served memory face
/// (the assembled fixture, or that fixture behind a test admission hold). Dial
/// the returned address, then await the handle for the responder that served
/// the session.
async fn start_listener(
    memory_service: Arc<dyn RemoteMemoryService>,
    host_manifest: HostCapabilityManifest,
    client_peer: String,
    client_pubkey: [u8; 32],
    invoke_timeout_ms: u64,
) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
    start_listener_for(
        memory_service,
        host_manifest,
        HashMap::from([(client_peer, client_pubkey)]),
        invoke_timeout_ms,
    )
    .await
}

/// The same listener for the exact trusted dialer table the responder admits.
async fn start_listener_for(
    memory_service: Arc<dyn RemoteMemoryService>,
    host_manifest: HostCapabilityManifest,
    peers: HashMap<String, [u8; 32]>,
    invoke_timeout_ms: u64,
) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
    let (addr, task, _control) =
        start_listener_with_control(memory_service, host_manifest, peers, invoke_timeout_ms).await;
    (addr, task)
}

/// The same listener plus a control channel for the accepted TCP socket.
///
/// `shutdown` on the received clone tears the connection down abruptly, with no
/// WebSocket close frame, so a signed consumer observes a real broken
/// transport rather than the client's own graceful close. The receiver resolves
/// once the peer is accepted, so the caller dials first and awaits it after.
async fn start_listener_with_control(
    memory_service: Arc<dyn RemoteMemoryService>,
    host_manifest: HostCapabilityManifest,
    peers: HashMap<String, [u8; 32]>,
    invoke_timeout_ms: u64,
) -> (
    SocketAddr,
    tokio::task::JoinHandle<Arc<ConnectResponder>>,
    tokio::sync::oneshot::Receiver<std::net::TcpStream>,
) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let allowlist = peers.keys().cloned().collect();
    let (control_tx, control_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let (tcp, peer) = tokio::time::timeout(SETUP_BOUND, listener.accept())
            .await
            .expect("the client connects to the listener")
            .expect("accept the localhost peer");
        assert!(peer.ip().is_loopback(), "the fixture serves localhost only");
        let std_stream = tcp.into_std().expect("the accepted socket converts");
        let control = std_stream.try_clone().expect("clone the accepted socket");
        let tcp = TcpStream::from_std(std_stream).expect("restore the accepted socket");
        let _ = control_tx.send(control);
        let ws = tokio::time::timeout(
            SETUP_BOUND,
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
            allowlist,
            peer_keys: peers,
            ports: Some(ports),
            invoke_timeout_ms: Some(invoke_timeout_ms),
        })
        .await
    });
    (addr, task, control_rx)
}

// ---------------------------------------------------------------------------
// The disposable user home and the remote selection
// ---------------------------------------------------------------------------

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    creator_id: String,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().expect("temp home");
        let user_home = tmp.path().to_path_buf();
        fs::create_dir_all(user_home.join(".nexus42")).expect("nexus home");
        fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home, CREATOR, WORKSPACE,
        ))
        .expect("workspace dir");
        fs::write(
            user_home.join(".nexus42").join("config.toml"),
            format!(
                "active_creator_id = \"{CREATOR}\"\n\
                 [active_workspace_slug_by_creator]\n\
                 \"{CREATOR}\" = \"{WORKSPACE}\"\n"
            ),
        )
        .expect("seed config");
        Self {
            _tmp: tmp,
            user_home,
            creator_id: CREATOR.to_string(),
        }
    }

    /// The same disposable home bootstrapped through the real public home
    /// contract: a registered Creator and a selected workspace, so the actor
    /// surface (`create_world`/`create_character`/`add_binding`) is usable.
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
        Self {
            _tmp: tmp,
            user_home,
            creator_id,
        }
    }

    /// Write the operator-provisioned credential for `seed` at mode `0600`.
    fn write_credential(&self, seed: &[u8; 32]) {
        self.write_credential_named(CREDENTIAL_ID, seed, None);
    }

    /// Write a credential whose `capability_token` is the given proof object
    /// (or `null`), so the load boundary sees the exact published shape.
    fn write_credential_with_token(&self, seed: &[u8; 32], token: Option<&Value>) {
        self.write_credential_named(CREDENTIAL_ID, seed, token);
    }

    fn write_credential_named(&self, credential_id: &str, seed: &[u8; 32], token: Option<&Value>) {
        let dir = self.user_home.join(".nexus42").join("memory-credentials");
        fs::create_dir_all(&dir).expect("credential dir");
        let path = dir.join(format!("{credential_id}.json"));
        let token = token.map_or_else(|| "null".to_string(), Value::to_string);
        fs::write(
            &path,
            format!(
                "{{\"identity_seed_hex\":\"{}\",\"capability_token\":{token}}}",
                hex(seed)
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

    /// The workspace state database this open resolves.
    fn state_db(&self) -> PathBuf {
        nexus_home_layout::workspace_state_db_path(&self.user_home, &self.creator_id, WORKSPACE)
    }

    /// The local provider's private file root for this workspace. Its absence
    /// is the observable that no local provider was constructed.
    fn local_provider_store(&self) -> PathBuf {
        nexus_home_layout::operational_workspace_dir(&self.user_home, &self.creator_id, WORKSPACE)
            .join("memory-store")
    }

    /// The legacy bearer file the remote refusal must not touch.
    fn legacy_bearer(&self) -> PathBuf {
        self.user_home
            .join(".nexus42")
            .join("creators")
            .join(&self.creator_id)
            .join("memory")
            .join("long-term")
            .join("pinned.md")
    }

    fn options(&self, selection: MemoryProviderSelection) -> CoreOpenOptions {
        CoreOpenOptions {
            user_home: self.user_home.clone(),
            access: CoreAccess::EngineOwner,
            memory: Some(selection),
        }
    }
}

fn selection(
    space_id: &str,
    endpoint: SocketAddr,
    host_pubkey: &[u8; 32],
) -> MemoryProviderSelection {
    selection_for(
        space_id,
        endpoint,
        host_pubkey,
        CREDENTIAL_ID,
        vec![derive_peer_id_from_ed25519_pubkey(host_pubkey)],
    )
}

/// The same selection with an explicit credential file and client allowlist,
/// for the refusal boundaries (wrong pinned host key, unlisted host).
fn selection_for(
    space_id: &str,
    endpoint: SocketAddr,
    host_pubkey: &[u8; 32],
    credential_id: &str,
    allowlist: Vec<String>,
) -> MemoryProviderSelection {
    MemoryProviderSelection::Remote(RemoteMemoryProviderSelection {
        space_id: space_id.to_string(),
        credential_id: credential_id.to_string(),
        connect_endpoint: format!("ws://{endpoint}"),
        remote_pubkey_hex: hex(host_pubkey),
        allowlist,
    })
}

async fn open(env: &Env, selection: MemoryProviderSelection) -> CoreResult<CoreService> {
    CoreService::open(env.options(selection)).await
}

fn coded(error: &CoreError) -> &str {
    match error {
        CoreError::Coded { code, .. } => code,
        other => panic!("expected a coded refusal, got {other:?}"),
    }
}

/// Await one responder task without letting a fixture bug hang the suite.
async fn responder_of(
    task: tokio::task::JoinHandle<Arc<ConnectResponder>>,
) -> Arc<ConnectResponder> {
    tokio::time::timeout(SETUP_BOUND, task)
        .await
        .expect("the responder task is bounded")
        .expect("the responder task completed")
}

// ---------------------------------------------------------------------------
// The pinning test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remote_open_pins_space_and_authority() {
    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);

    // A credential failure fails the whole open before any socket is dialed:
    // the static category is returned and no local provider appears.
    let uncredentialed = Env::new();
    let dead_endpoint: SocketAddr = "127.0.0.1:9".parse().expect("dead endpoint");
    let error = open(
        &uncredentialed,
        selection("spc_unserved", dead_endpoint, &host_pubkey),
    )
    .await
    .err()
    .expect("a missing credential fails the open");
    assert_eq!(
        coded(&error),
        "memory_credential_invalid",
        "the credential refusal is static and precedes the dial"
    );
    assert!(
        !uncredentialed.local_provider_store().exists(),
        "a refused credential never constructs the local provider"
    );

    // The durable fixture: one space, one authority, one operator-provisioned
    // admin grant for the principal the dialing peer maps to.
    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&fixture_home.path().join("memory.sqlite"))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));

    let env = Env::new();
    env.write_credential(&CLIENT_SEED);
    let legacy = env.legacy_bearer();
    fs::create_dir_all(legacy.parent().expect("legacy dir")).expect("legacy bearer dir");
    fs::write(&legacy, b"# legacy long-term memory\n").expect("seed the legacy bearer");

    // A space the provider does not serve: the signed session opens, the exact
    // space is absent from discovery, and the open fails instead of taking the
    // provider's advertised space.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(
        &env,
        selection("spc_not_served_by_this_provider", addr, &host_pubkey),
    )
    .await
    .err()
    .expect("an unserved space fails the open");
    assert_eq!(
        coded(&error),
        "memory_remote_space",
        "the configured space must be discovered exactly once"
    );
    responder_of(task).await;
    assert!(
        !env.local_provider_store().exists(),
        "a space refusal never falls back to the local provider"
    );

    // The host does not negotiate the memory family at all.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest_without_memory(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .err()
        .expect("a host without the memory family fails the open");
    assert_eq!(
        coded(&error),
        "memory_remote_capability",
        "the negotiated family is required even when the space would match"
    );
    responder_of(task).await;
    assert!(
        !env.local_provider_store().exists(),
        "a capability refusal never falls back to the local provider"
    );

    // The space is discovered, but its authority is not the authenticated host.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest("fixture-not-the-space-authority"),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .err()
        .expect("an authority mismatch fails the open");
    assert_eq!(
        coded(&error),
        "memory_remote_authority",
        "the manifest host must own the discovered space authority"
    );
    responder_of(task).await;
    assert!(
        !env.local_provider_store().exists(),
        "an authority refusal never falls back to the local provider"
    );

    // The exact space on its authenticated authority opens, and the binding
    // serves the provider.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let core = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the exact selection opens against the served space");
    let responder = responder_of(task).await;
    assert_eq!(
        responder.remote_peer_id().as_deref(),
        Some(client_peer.as_str()),
        "the host authenticated the loaded credential's public peer identity"
    );
    assert!(
        responder.state().as_str() == "Established",
        "the host session really reached Established"
    );
    assert!(
        !env.local_provider_store().exists(),
        "the remote binding constructed no local provider"
    );

    // A real product read runs through the pinned binding: opening the Creator
    // memory adapter ensures the subject and lists candidates in the pinned
    // space over the signed session (the fixture refuses any other space).
    let principal = core.active_principal().await.expect("active principal");
    assert_eq!(
        core.count_pending_reviews(&principal)
            .await
            .expect("the pinned remote binding serves the memory family")
            .count,
        0,
        "the fixture holds no pending candidates"
    );

    // Legacy migration is a local-only operation: the nonlocal binding refuses
    // with the structured code and reads/writes no local legacy content.
    let refusal = core
        .migrate_legacy_content(&principal)
        .await
        .err()
        .expect("legacy migration is not supported on a remote binding");
    assert_eq!(
        coded(&refusal),
        "not_supported",
        "the retained local-only migration refusal"
    );
    assert_eq!(
        fs::read(&legacy).expect("the seeded legacy bearer"),
        b"# legacy long-term memory\n",
        "the remote refusal left the local legacy bearer untouched"
    );
    assert!(
        !env.local_provider_store().exists(),
        "the remote refusal created no local provider store"
    );

    // The owned close releases the binding; the closed open serves no memory
    // and a repeated close is idempotent.
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the close confirmed its own admission release"
    );
    assert!(
        matches!(
            core.count_pending_reviews(&principal).await,
            Err(CoreError::Closing)
        ),
        "a closed open serves no memory"
    );
    core.close().await.expect("idempotent close");
}

// ---------------------------------------------------------------------------
// Cancellation of an open at its established-session admission
// ---------------------------------------------------------------------------

/// Test-only admission hold in front of the real assembled fixture service.
///
/// It parks the first `memory.describe` invoke after the signed session is
/// already established — exactly the await the owning review names — and then
/// delegates to the real fixture. It serves no port, changes no protocol object
/// and fabricates no provider state.
struct AdmissionHold {
    inner: Arc<dyn RemoteMemoryService>,
    reached: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl RemoteMemoryService for AdmissionHold {
    async fn invoke(&self, peer_id: &str, op: &str, payload: Value) -> SpokeResult<Value> {
        if op == "memory.describe" {
            self.reached.notify_one();
            self.release.notified().await;
        }
        self.inner.invoke(peer_id, op, payload).await
    }
}

/// Open one remote selection from a home the caller keeps alive, so the future
/// can be spawned and then cancelled.
fn open_owned(
    user_home: PathBuf,
    selection: MemoryProviderSelection,
) -> impl std::future::Future<Output = CoreResult<CoreService>> {
    CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::EngineOwner,
        memory: Some(selection),
    })
}

/// Wait until the host reports the abandoned session closed.
async fn wait_for_closed(responder: &Arc<ConnectResponder>) {
    tokio::time::timeout(SETUP_BOUND, async {
        loop {
            if responder.state() == ConnectResponderState::Closed {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the host observed the abandoned established session closing");
}

/// Cancelling an open while its established-session admission is pending must
/// close exactly that session: the abandoned peer observes its session closing,
/// and a second, delivered open against the same provider keeps serving.
#[tokio::test]
async fn cancelled_open_closes_only_its_own_established_session() {
    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);

    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&fixture_home.path().join("memory.sqlite"))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));

    // The abandoned open is held inside its established-session admission; the
    // generous serve budget keeps the held call pending instead of timing out.
    let reached = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let held: Arc<dyn RemoteMemoryService> = Arc::new(AdmissionHold {
        inner: provider.server(directory.admission()),
        reached: Arc::clone(&reached),
        release: Arc::clone(&release),
    });
    // A separate open is delivered first, so it is already Established while
    // the measured open is pending.
    let delivered_home = Env::new();
    delivered_home.write_credential(&CLIENT_SEED);
    let (delivered_addr, delivered_host) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let delivered = open(
        &delivered_home,
        selection(&served_space, delivered_addr, &host_pubkey),
    )
    .await
    .expect("a separate open is delivered against the same provider");

    let (abandoned_addr, abandoned_host) = start_listener(
        held,
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        30_000,
    )
    .await;

    let abandoned_home = Env::new();
    abandoned_home.write_credential(&CLIENT_SEED);
    let abandoned = tokio::spawn(open_owned(
        abandoned_home.user_home.clone(),
        selection(&served_space, abandoned_addr, &host_pubkey),
    ));
    tokio::time::timeout(SETUP_BOUND, reached.notified())
        .await
        .expect("the abandoned open reached the established-session admission");

    // Cancel exactly at the held admission and observe the real peer closing
    // the established session it owns. The abort happens immediately after the
    // hold is reached: the SDK's own invoke budget is shorter than this test's
    // setup work, so the measured open must not wait behind another open.
    abandoned.abort();
    let cancelled = match abandoned.await {
        Ok(_) => panic!("the abandoned open completed instead of being cancelled"),
        Err(join) => join,
    };
    assert!(
        cancelled.is_cancelled(),
        "the abandoned open task was cancelled"
    );
    let abandoned_responder = responder_of(abandoned_host).await;
    wait_for_closed(&abandoned_responder).await;
    release.notify_waiters();

    // The delivered open is untouched: its own session is still established and
    // its pinned binding still serves the provider's space.
    let delivered_responder = responder_of(delivered_host).await;
    assert_eq!(
        delivered_responder.state().as_str(),
        "Established",
        "the other delivered session keeps running"
    );
    let principal = delivered.active_principal().await.expect("principal");
    assert_eq!(
        delivered
            .count_pending_reviews(&principal)
            .await
            .expect("the delivered binding still serves the memory family")
            .count,
        0
    );
    assert!(
        delivered.close().await.expect("close").cleanup_confirmed,
        "the delivered open closes normally"
    );
}

/// Dropping the last open without an explicit close still ends its session.
///
/// The SDK's receive task keeps its own `Arc` on the adapter, so only the
/// binding's own cleanup can release a *delivered* remote session. A core that
/// is dropped after an error — never closed — must therefore still let the
/// authenticated host observe the session closing, exactly as the explicit
/// close does.
#[tokio::test]
async fn dropping_the_open_closes_its_established_session() {
    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);

    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&fixture_home.path().join("memory.sqlite"))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));

    let home = Env::new();
    home.write_credential(&CLIENT_SEED);
    let (addr, host) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let core = open(&home, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the exact selection opens against the served space");

    // The delivered binding really serves the provider over the signed
    // session, so the drop below releases a live one.
    let principal = core.active_principal().await.expect("active principal");
    core.count_pending_reviews(&principal)
        .await
        .expect("the pinned remote binding serves the memory family");

    // No explicit close: dropping the last handle is the only thing that can
    // end this session.
    drop(core);
    let responder = responder_of(host).await;
    wait_for_closed(&responder).await;
}

// ---------------------------------------------------------------------------
// Authenticated admission: refusals, reconnect replay and credential redaction
// ---------------------------------------------------------------------------

/// A second Connect identity whose memory authority is exactly the one scoped
/// grant the admin issues it.
const SCOPED_SEED: [u8; 32] = [0x52; 32];
/// A third seed, used only as an untrusted peer id/key.
const OTHER_SEED: [u8; 32] = [0x63; 32];
const SCOPED_CREDENTIAL_ID: &str = "cred-remote-scoped";
const CAPTURE_PENDING: &str = "pend_remote_open";
const CAPTURE_SESSION: &str = "sess_remote_open";
const CAPTURE_WORLD: &str = "wld_remote_open";
const CAPTURE_DIGEST: &str =
    "Reconnect replays this capture's original receipt, never a second candidate.";

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid payload")
}

fn capture_body_as(pending_id: &str, creator_id: &str) -> CreatePendingReviewRequest {
    dto(json!({
        "pending_id": pending_id,
        "session_id": CAPTURE_SESSION,
        "creator_id": creator_id,
        "world_id": CAPTURE_WORLD,
        "task_kind": "chapter",
        "raw_digest": CAPTURE_DIGEST,
        "created_at": "2026-11-05T09:00:00Z",
    }))
}

fn capture_body(pending_id: &str) -> CreatePendingReviewRequest {
    capture_body_as(pending_id, CREATOR)
}

fn principal_label(principal_id: &str) -> Principal {
    dto(json!({ "principal_id": principal_id, "extensions": {} }))
}

/// The hello manifest the product's remote dial sends: the one family it is a
/// client of and no served op.
fn client_manifest(peer: &str) -> HostCapabilityManifest {
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": peer,
        "roles": ["memory-client"],
        "capabilities": MEMORY_CAPABILITIES,
        "namespaces": [],
        "extensions": {},
    }))
    .expect("valid client manifest")
}

/// A signed SDK session on the product WS transport, admitted by the same
/// responder a Core open dials. The product open itself only ever issues its
/// own admission ops; this is the SDK client for the boundaries it never
/// constructs (the optional families, the admin's grant).
async fn dial_adapter(addr: SocketAddr, seed: [u8; 32]) -> Arc<RemoteAdapter> {
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the localhost websocket connect is bounded")
    .expect("connect the localhost websocket");
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let dialed = connect_remote_adapter(RemoteAdapterOptions {
        transport,
        local_identity: RemoteIdentity { seed },
        local_manifest: client_manifest(&peer_id(&seed)),
        remote_pubkey: pubkey(&HOST_SEED),
        allowlist: vec![peer_id(&HOST_SEED)],
        invoke_timeout_ms: Some(5_000),
        capability_token: None,
    });
    tokio::time::timeout(SETUP_BOUND, dialed)
        .await
        .expect("the signed dial is bounded")
        .expect("the signed dial is admitted")
}

/// Issue one grant through the admin's own signed session. The fixture's
/// non-amplification rule decides whether the issuance is admissible.
async fn issue_grant(adapter: &RemoteAdapter, admin_principal: &str, body: Value) {
    match adapter
        .grant(
            &principal_label(admin_principal),
            dto::<MemoryGrantsGrantRequest>(body),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the admin issues the grant: {reject:?}"),
    }
}

fn provider_database(fixture_home: &TempDir) -> PathBuf {
    fixture_home.path().join("memory.sqlite")
}

async fn provider_pool(fixture_home: &TempDir) -> SqlitePool {
    SqlitePool::connect_with(SqliteConnectOptions::new().filename(provider_database(fixture_home)))
        .await
        .expect("open the fixture provider database")
}

/// The workspace state database the product Core writes: where the D18
/// namespace facts derived from the actual pinned binding live.
async fn core_db(env: &Env) -> SqlitePool {
    SqlitePool::connect_with(SqliteConnectOptions::new().filename(env.state_db()))
        .await
        .expect("open the workspace state database")
}

async fn provider_receipts(pool: &SqlitePool, op: &str) -> i64 {
    sqlx::query("SELECT COUNT(*) FROM receipts WHERE op = ?")
        .bind(op)
        .fetch_one(pool)
        .await
        .expect("provider receipt count")
        .get(0)
}

/// Every provider candidate as `(candidate_id, status)`.
async fn provider_candidates(pool: &SqlitePool) -> Vec<(String, String)> {
    sqlx::query("SELECT candidate_id, status FROM candidates ORDER BY candidate_id")
        .fetch_all(pool)
        .await
        .expect("provider candidates")
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect()
}

async fn provider_generation(pool: &SqlitePool) -> i64 {
    sqlx::query("SELECT authority_generation FROM spaces")
        .fetch_one(pool)
        .await
        .expect("provider space")
        .get(0)
}

/// The provider's durable receipt and candidate bodies as searchable text: the
/// real sink a credential sentinel must never reach.
async fn provider_body_text(pool: &SqlitePool) -> String {
    let rows: Vec<String> = sqlx::query(
        "SELECT COALESCE(result_json, '') || COALESCE(error_json, '') || extensions_json \
         FROM receipts \
         UNION ALL \
         SELECT COALESCE(content_json, '') || provenance_json || classification_json || \
         extensions_json FROM candidates",
    )
    .fetch_all(pool)
    .await
    .expect("provider body text")
    .into_iter()
    .map(|row| row.get(0))
    .collect();
    rows.join("\n")
}

/// A structurally valid capability proof whose claims already expired. The
/// credential contract (§3.1) declares structural validation only, so Nexus
/// accepts the shape and the host verifies the token.
fn expired_token(iss: &str, aud: &str) -> Value {
    json!({
        "v": 1,
        "claims": {
            "iss": iss,
            "sub": iss,
            "aud": aud,
            "capabilities": ["spoke-memory"],
            "exp": 1_u64,
        },
        "sig": "expired-proof-signature",
    })
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is after the Unix epoch")
        .as_secs()
}

/// Issue one real capability proof through the pinned SDK's own issuer.
fn capability_proof_at(
    issuer_seed: &[u8; 32],
    subject: &str,
    audience: &str,
    exp: u64,
    issued_at: u64,
) -> CapabilityTokenProof {
    issue_capability_token(
        issuer_seed,
        CapabilityClaims {
            iss: peer_id(issuer_seed),
            sub: subject.to_string(),
            aud: audience.to_string(),
            capabilities: vec!["spoke-memory".to_string()],
            exp,
            iat: None,
            jti: None,
        },
        issued_at,
    )
    .expect("the SDK issues the proof")
}

/// One committed provider receipt, read from the independent fixture's own row.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderReceipt {
    receipt_id: String,
    requester: String,
    key: Option<String>,
    digest: String,
    result: String,
    status: String,
}

async fn committed_receipts(pool: &SqlitePool, op: &str) -> Vec<ProviderReceipt> {
    sqlx::query(
        "SELECT receipt_id, requester_principal_id, idempotency_key, payload_digest, \
         COALESCE(result_json, ''), status FROM receipts WHERE op = ? AND status = 'committed' \
         ORDER BY receipt_id",
    )
    .bind(op)
    .fetch_all(pool)
    .await
    .expect("provider receipts")
    .into_iter()
    .map(|row| ProviderReceipt {
        receipt_id: row.get(0),
        requester: row.get(1),
        key: row.get::<Option<String>, _>(2),
        digest: row.get(3),
        result: row.get(4),
        status: row.get(5),
    })
    .collect()
}

fn receipt_candidate_ids(receipt: &ProviderReceipt) -> Vec<String> {
    let result: Value = serde_json::from_str(&receipt.result).expect("receipt result is JSON");
    result["candidate_ids"]
        .as_array()
        .expect("a capture receipt names its candidate ids")
        .iter()
        .map(|id| id.as_str().expect("candidate id string").to_string())
        .collect()
}

/// The provider's complete relevant effect set as stable text rows.
///
/// Not a count: an insertion, removal or content/revision change in receipts,
/// candidates or records changes the snapshot. Schema knowledge stays here in
/// the test, never in a helper the product could drift from.
async fn provider_effect_snapshot(pool: &SqlitePool) -> Vec<String> {
    let mut rows: Vec<String> = sqlx::query(
        "SELECT 'receipt:' || receipt_id || '|' || requester_principal_id || '|' || \
         COALESCE(idempotency_key, '') || '|' || payload_digest || '|' || status || '|' || \
         COALESCE(result_json, '') FROM receipts WHERE op <> 'memory.subjects.ensure' \
         ORDER BY receipt_id",
    )
    .fetch_all(pool)
    .await
    .expect("receipt rows")
    .into_iter()
    .map(|row| row.get(0))
    .collect();
    rows.extend(
        sqlx::query(
            "SELECT 'candidate:' || candidate_id || '|' || status || '|' || \
             COALESCE(content_digest, '') || '|' || provenance_json || '|' || \
             classification_json FROM candidates ORDER BY candidate_id",
        )
        .fetch_all(pool)
        .await
        .expect("candidate rows")
        .into_iter()
        .map(|row| row.get::<String, _>(0)),
    );
    rows.extend(
        sqlx::query(
            "SELECT 'record:' || record_id || '|' || kind || '|' || scope_visibility || '|' || \
             COALESCE(scope_context_id, '') || '|' || content_json FROM records ORDER BY record_id",
        )
        .fetch_all(pool)
        .await
        .expect("record rows")
        .into_iter()
        .map(|row| row.get::<String, _>(0)),
    );
    rows
}

/// The binding's still-open D18 wire admissions, one tuple of the identity keys
/// the actual pinned Core binding produced.
async fn binding_admissions(
    pool: &SqlitePool,
    binding_id: &str,
) -> Vec<(String, String, String, String, String, String, String)> {
    sqlx::query(
        "SELECT owner_creator_id, character_id, binding_id, authority_host_id, \
         authority_generation, space_id, client_identity \
         FROM remote_binding_memory_admissions WHERE binding_id = ? ORDER BY admission_id",
    )
    .bind(binding_id)
    .fetch_all(pool)
    .await
    .expect("binding admissions")
    .into_iter()
    .map(|row| {
        (
            row.get(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            row.get(5),
            row.get(6),
        )
    })
    .collect()
}

/// The binding's D18 object facts: the eight identity keys plus the monotone
/// state, written from the actual pinned binding.
async fn binding_object_facts(
    pool: &SqlitePool,
    binding_id: &str,
) -> Vec<(
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
)> {
    sqlx::query(
        "SELECT authority_host_id, authority_generation, space_id, client_identity, \
         character_id, binding_id, object_kind, object_id, state \
         FROM remote_binding_memory_objects WHERE binding_id = ? ORDER BY object_kind, object_id",
    )
    .bind(binding_id)
    .fetch_all(pool)
    .await
    .expect("binding object facts")
    .into_iter()
    .map(|row| {
        (
            row.get(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            row.get(5),
            row.get(6),
            row.get(7),
            row.get(8),
        )
    })
    .collect()
}

fn response_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("the SDK response serializes")
}

/// Test-only observation of the responder-side transport.
///
/// Every received byte sequence is passed through **unmodified** to the product
/// `WsTransport`; this wrapper only reads an invoke envelope's `op`,
/// `session_id`, `request_id` and optional `auth` for association. It performs
/// no handshake, signature, sequence or protocol validation — the pinned SDK
/// remains the sole validator of those.
///
/// SHORTCUT (documented test-only limit): the token scenario drives ONE
/// outstanding memory invoke at a time, so a single pending observation is
/// unambiguous. A second observation before the first is consumed marks the
/// slot ambiguous and every later admission is refused; this is not a
/// generalized concurrent-request policy.
/// The single-outstanding observation state.
///
/// Every transition happens under ONE lock, so observing and consuming are
/// atomic and an overlap irreversibly poisons the observer: an overwritten or
/// ambiguous observation is never kept, and no later observation can revive a
/// poisoned session.
enum ObserverState {
    Empty,
    Pending(ObservedInvoke),
    Poisoned,
}

struct EnvelopeObserver {
    inner: Arc<dyn Transport>,
    state: Mutex<ObserverState>,
}

struct ObservedInvoke {
    op: String,
    session_id: String,
    request_id: String,
    payload: Value,
    auth: Option<Value>,
}

impl EnvelopeObserver {
    fn new(inner: Arc<dyn Transport>) -> Self {
        Self {
            inner,
            state: Mutex::new(ObserverState::Empty),
        }
    }

    fn record(&self, envelope: &Value) {
        let Some(op) = envelope.get("op").and_then(Value::as_str) else {
            return;
        };
        let observed = ObservedInvoke {
            op: op.to_string(),
            session_id: envelope
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            request_id: envelope
                .get("request_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            payload: envelope.get("payload").cloned().unwrap_or(Value::Null),
            auth: envelope.get("auth").filter(|auth| !auth.is_null()).cloned(),
        };
        let mut state = self.state.lock().expect("observer state");
        match &*state {
            ObserverState::Empty => *state = ObserverState::Pending(observed),
            ObserverState::Pending(_) | ObserverState::Poisoned => {
                // Two invokes overlapped (or this session was already poisoned):
                // keep neither observation and stay poisoned forever.
                *state = ObserverState::Poisoned;
            }
        }
    }

    /// Atomically consume the single outstanding observation.
    ///
    /// An empty slot or a poisoned session poisons it permanently: the caller can
    /// never clear the poison by retrying, and a later observation cannot revive
    /// it.
    fn take(&self) -> Result<ObservedInvoke, &'static str> {
        let mut state = self.state.lock().expect("observer state");
        match std::mem::replace(&mut *state, ObserverState::Empty) {
            ObserverState::Pending(observed) => Ok(observed),
            ObserverState::Empty => {
                *state = ObserverState::Poisoned;
                Err("no received signed invocation to verify")
            }
            ObserverState::Poisoned => {
                *state = ObserverState::Poisoned;
                Err("the observed session is poisoned by an overlapping invocation")
            }
        }
    }
}

#[async_trait]
impl Transport for EnvelopeObserver {
    async fn send(&self, envelope: &[u8]) -> Result<(), TransportError> {
        self.inner.send(envelope).await
    }

    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        let envelope = self.inner.recv().await?;
        if let Ok(value) = serde_json::from_slice::<Value>(&envelope) {
            self.record(&value);
        }
        Ok(envelope)
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

/// The test-host token admission, over the **actually received** proof.
///
/// The proof is taken from the envelope the SDK just received (never from test
/// setup), the op is the dispatched operation, the peer is the
/// SDK-authenticated session peer, and the destination is this host's peer id.
/// Missing, mismatched, reused or ambiguous observations are refused before the
/// fixture sees anything.
struct TokenGate {
    inner: Arc<dyn RemoteMemoryService>,
    observer: Arc<EnvelopeObserver>,
    trusted: Vec<String>,
    host_peer: String,
}

impl TokenGate {
    fn deny(reason: &str) -> SpokeResult<Value> {
        let mut details = serde_json::Map::new();
        details.insert("retry_class".to_owned(), json!("terminal"));
        SpokeResult::Reject(SpokeReject {
            code: SpokeRejectCode::NotAuthorized,
            message: format!("capability token refused: {reason}"),
            details: Some(details),
        })
    }
}

#[async_trait]
impl RemoteMemoryService for TokenGate {
    async fn invoke(&self, peer_id: &str, op: &str, payload: Value) -> SpokeResult<Value> {
        let observed = match self.observer.take() {
            Ok(observed) => observed,
            Err(reason) => return Self::deny(reason),
        };
        if observed.session_id.is_empty() || observed.request_id.is_empty() {
            return Self::deny("the received invoke carried no authenticated session identity");
        }
        if observed.op != op {
            return Self::deny("the received op does not match the dispatched op");
        }
        if observed.payload != payload {
            return Self::deny("the received payload does not match the dispatched payload");
        }
        let Some(auth) = observed.auth else {
            return Self::deny("the received invoke carried no auth proof");
        };
        let proof: CapabilityTokenProof = match serde_json::from_value(auth) {
            Ok(proof) => proof,
            Err(_) => return Self::deny("the received auth is not the published proof shape"),
        };
        match verify_capability_token(
            &proof,
            &self.trusted,
            &self.host_peer,
            peer_id,
            now_seconds(),
        ) {
            Ok(_capabilities) => self.inner.invoke(peer_id, op, payload).await,
            Err(_) => Self::deny("the received proof did not verify"),
        }
    }
}

/// Serve the token-gated fixture over one loopback listener whose transport is
/// observed. Every admission decision uses the proof carried by the invocation
/// the SDK actually received.
async fn start_listener_token_gated(
    provider: &Arc<FixtureMemoryProvider>,
    directory: &Arc<PeerDirectory>,
    host_manifest: HostCapabilityManifest,
    peers: HashMap<String, [u8; 32]>,
    trusted: Vec<String>,
    host_peer: String,
) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let allowlist = peers.keys().cloned().collect();
    let fixture = provider.server(directory.admission());
    let task = tokio::spawn(async move {
        let (tcp, peer) = tokio::time::timeout(SETUP_BOUND, listener.accept())
            .await
            .expect("the client connects to the listener")
            .expect("accept the localhost peer");
        assert!(peer.ip().is_loopback(), "the fixture serves localhost only");
        let ws = tokio::time::timeout(
            SETUP_BOUND,
            tokio_tungstenite::accept_async_with_config(
                tcp,
                Some(ws_config(DEFAULT_MAX_ENVELOPE_BYTES)),
            ),
        )
        .await
        .expect("the websocket upgrade is bounded")
        .expect("websocket upgrade");
        let base: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
        let observer = Arc::new(EnvelopeObserver::new(base));
        let gate: Arc<dyn RemoteMemoryService> = Arc::new(TokenGate {
            inner: fixture,
            observer: Arc::clone(&observer),
            trusted,
            host_peer,
        });
        let transport: Arc<dyn Transport> = Arc::clone(&observer) as Arc<dyn Transport>;
        let ports: Arc<dyn RemoteServePorts + Send + Sync> = Arc::new(
            RemoteServePortsComposite::new(Arc::new(MemoryOnlyHost), None, None).with_memory(gate),
        );
        connect_responder(ConnectResponderOptions {
            transport,
            identity: RemoteIdentity { seed: HOST_SEED },
            manifest: host_manifest,
            allowlist,
            peer_keys: peers,
            ports: Some(ports),
            invoke_timeout_ms: Some(5_000),
        })
        .await
    });
    (addr, task)
}

// ---------------------------------------------------------------------------
// Real product actor fixture (owned World, Character and active binding)
// ---------------------------------------------------------------------------

async fn create_world(core: &CoreService, principal: &CorePrincipal, title: &str) -> String {
    core.create_world(principal, dto(json!({ "title": title })))
        .await
        .expect("create an owned World")
        .world_id
}

async fn create_character(
    core: &CoreService,
    principal: &CorePrincipal,
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

fn character_capture_request(binding_id: Option<&str>, pending_id: &str, digest: &str) -> Value {
    let mut request = json!({
        "pending_id": pending_id,
        "session_id": format!("sess_{pending_id}"),
        "task_kind": "chapter",
        "raw_digest": digest,
        "created_at": "2026-11-05T09:00:00Z",
    });
    if let Some(binding_id) = binding_id {
        request["binding_id"] = json!(binding_id);
    }
    request
}

/// A legitimate reopen on the same home after one failed-open stage.
///
/// The stage's failure must have released exactly the guards that open took:
/// this open has to acquire fresh, serve, and close with the host observing its
/// own session ending.
#[allow(clippy::too_many_arguments)]
async fn assert_home_reopens(
    provider: &Arc<FixtureMemoryProvider>,
    directory: &Arc<PeerDirectory>,
    env: &Env,
    space: &str,
    host_pubkey: &[u8; 32],
    authority_host_id: &str,
    client_peer: &str,
    client_pubkey: [u8; 32],
    credential_id: &str,
) {
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(authority_host_id),
        client_peer.to_string(),
        client_pubkey,
        5_000,
    )
    .await;
    let core = open(
        env,
        selection_for(
            space,
            addr,
            host_pubkey,
            credential_id,
            vec![peer_id(&HOST_SEED)],
        ),
    )
    .await
    .expect("the same home reopens legitimately after the failed stage");
    let responder = responder_of(task).await;
    assert!(
        core.close().await.expect("close").cleanup_confirmed,
        "the reopened home closes its own admission"
    );
    wait_for_closed(&responder).await;
}

/// Settle one listener the client refused.
///
/// The listener returns as soon as `connect_responder` spawns its own runner,
/// so a COMPLETED listener is not a closed session: its responder is closed and
/// its public `Closed` state observed. An expected cancellation is distinct from
/// a panic, and a task that is still running is aborted and then joined. The SDK
/// runner has no public join and is never claimed drained.
async fn settle_listener(mut task: tokio::task::JoinHandle<Arc<ConnectResponder>>) {
    match tokio::time::timeout(SETUP_BOUND, &mut task).await {
        Ok(Ok(responder)) => {
            responder.close();
            wait_for_closed(&responder).await;
        }
        Ok(Err(error)) if error.is_cancelled() => {}
        Ok(Err(error)) => panic!("the listener task failed: {error}"),
        Err(_) => {
            task.abort();
            match task.await {
                Ok(responder) => {
                    responder.close();
                    wait_for_closed(&responder).await;
                }
                Err(error) if error.is_cancelled() => {}
                Err(error) => panic!("the aborted listener task failed: {error}"),
            }
        }
    }
}

/// The client-side admission boundaries, stage by stage: a credential that is
/// not the published proof shape, an unreachable endpoint, an unpinned or
/// unlisted host, an untrusted dialer, a host without the umbrella family, an
/// unserved space and a non-authoritative space. Each fails the whole open with
/// its static class; after every stage the SAME home opens legitimately, the
/// listener settles, and the provider's effect set is unchanged.
#[tokio::test]
async fn connect_admission_refuses_untrusted_credentials_keys_and_families() {
    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);
    let host_peer = peer_id(&HOST_SEED);

    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));
    let pool = provider_pool(&fixture_home).await;
    let effects_before = provider_effect_snapshot(&pool).await;
    let dead_endpoint: SocketAddr = "127.0.0.1:9".parse().expect("dead endpoint");

    // Stage 1 — a capability token that is not the published `{v, claims, sig}`
    // shape: the loader refuses before any socket is dialed.
    let malformed = Env::new();
    malformed.write_credential_with_token(&CLIENT_SEED, Some(&json!({ "v": 1 })));
    let error = open(
        &malformed,
        selection(&served_space, dead_endpoint, &host_pubkey),
    )
    .await
    .err()
    .expect("a malformed token refuses the open");
    assert_eq!(
        coded(&error),
        "memory_credential_invalid",
        "the credential refusal is static and precedes the dial"
    );
    assert!(!malformed.local_provider_store().exists());
    malformed.write_credential(&CLIENT_SEED);
    assert_home_reopens(
        &provider,
        &directory,
        &malformed,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    // Stage 2 — the endpoint is unreachable. (§3.1 is structural validation, so
    // a well-formed proof — even an expired one — never produces a local
    // credential refusal; the real verifier lives in the token scenario below.)
    let unreachable = Env::new();
    unreachable.write_credential(&CLIENT_SEED);
    let error = open(
        &unreachable,
        selection(&served_space, dead_endpoint, &host_pubkey),
    )
    .await
    .err()
    .expect("a dead endpoint fails the open");
    assert_eq!(coded(&error), "memory_remote_unavailable");
    assert_home_reopens(
        &provider,
        &directory,
        &unreachable,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    let env = Env::new();
    env.write_credential(&CLIENT_SEED);

    // Stage 3 — the client's pinned host key is not the host's.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let wrong_host = pubkey(&OTHER_SEED);
    let error = open(
        &env,
        selection_for(
            &served_space,
            addr,
            &wrong_host,
            CREDENTIAL_ID,
            vec![host_peer.clone()],
        ),
    )
    .await
    .err()
    .expect("a mismatched host key fails the handshake");
    assert_eq!(coded(&error), "memory_remote_handshake");
    settle_listener(task).await;
    assert_eq!(provider_effect_snapshot(&pool).await, effects_before);
    assert!(!env.local_provider_store().exists());
    assert_home_reopens(
        &provider,
        &directory,
        &env,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    // Stage 4 — the host's key is pinned but it is not on the allowlist.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(
        &env,
        selection_for(
            &served_space,
            addr,
            &host_pubkey,
            CREDENTIAL_ID,
            vec![peer_id(&OTHER_SEED)],
        ),
    )
    .await
    .err()
    .expect("an unlisted host fails the handshake");
    assert_eq!(coded(&error), "memory_remote_handshake");
    settle_listener(task).await;
    assert_eq!(provider_effect_snapshot(&pool).await, effects_before);
    assert_home_reopens(
        &provider,
        &directory,
        &env,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    // Stage 5 — the dialer's own key is not trusted by the host.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        peer_id(&OTHER_SEED),
        pubkey(&OTHER_SEED),
        5_000,
    )
    .await;
    let error = open(
        &env,
        selection_for(
            &served_space,
            addr,
            &host_pubkey,
            CREDENTIAL_ID,
            vec![host_peer.clone()],
        ),
    )
    .await
    .err()
    .expect("an untrusted dialer fails the handshake");
    assert_eq!(coded(&error), "memory_remote_handshake");
    settle_listener(task).await;
    assert_eq!(provider_effect_snapshot(&pool).await, effects_before);
    assert_home_reopens(
        &provider,
        &directory,
        &env,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    // Stage 6 — `spoke-memory` is the required umbrella; a host declaring only
    // the sub-families negotiates nothing this client speaks.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest_with(
            &authority_host_id,
            &["spoke-memory-reflection", "spoke-memory-transfer"],
        ),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .err()
        .expect("sub-families without the umbrella fail the open");
    assert_eq!(coded(&error), "memory_remote_capability");
    let responder = responder_of(task).await;
    wait_for_closed(&responder).await;
    assert_eq!(provider_effect_snapshot(&pool).await, effects_before);
    assert_home_reopens(
        &provider,
        &directory,
        &env,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    // Stage 7 — space discovery: the configured space is not served.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(
        &env,
        selection("spc_not_served_by_this_provider", addr, &host_pubkey),
    )
    .await
    .err()
    .expect("an unserved space fails the open");
    assert_eq!(coded(&error), "memory_remote_space");
    let responder = responder_of(task).await;
    wait_for_closed(&responder).await;
    assert_eq!(provider_effect_snapshot(&pool).await, effects_before);
    assert_home_reopens(
        &provider,
        &directory,
        &env,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    // Stage 8 — the discovered space's authority is not the authenticated host.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest("fixture-not-the-space-authority"),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .err()
        .expect("an authority mismatch fails the open");
    assert_eq!(coded(&error), "memory_remote_authority");
    let responder = responder_of(task).await;
    wait_for_closed(&responder).await;
    assert_eq!(provider_effect_snapshot(&pool).await, effects_before);
    assert!(
        !env.local_provider_store().exists(),
        "no refusal falls back to the local provider"
    );
    assert_home_reopens(
        &provider,
        &directory,
        &env,
        &served_space,
        &host_pubkey,
        &authority_host_id,
        &client_peer,
        client_pubkey,
        CREDENTIAL_ID,
    )
    .await;

    assert_eq!(
        provider_effect_snapshot(&pool).await,
        effects_before,
        "every refused stage left the provider's complete effect set untouched"
    );
    pool.close().await;
}

/// The optional reflection and transfer families are neither negotiated nor
/// served: a real signed session gets the explicit, classified refusal, and the
/// provider commits nothing for it. N4 success remains deferred.
#[tokio::test]
async fn unavailable_reflection_and_transfer_ops_refuse_explicitly_on_the_signed_session() {
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);
    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory =
        Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id.clone()));
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer,
        client_pubkey,
        5_000,
    )
    .await;
    let adapter = dial_adapter(addr, CLIENT_SEED).await;
    let responder = responder_of(task).await;
    let remote = adapter
        .remote_manifest()
        .expect("the negotiated remote manifest");
    assert_eq!(
        remote.capabilities,
        vec!["spoke-memory".to_string()],
        "only the umbrella family is negotiated"
    );
    let principal = principal_label(&principal_id);
    let space = served_space.clone();

    match adapter
        .get_reflection_state(
            &principal,
            dto::<MemoryReflectionsStateRequest>(json!({
                "space_id": space,
                "subject_id": "sbj_remote_open",
                "selection": { "mode": "subject_all" },
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => {
            assert_eq!(reject.code, SpokeRejectCode::CapabilityPortMissing);
            assert_eq!(
                reject
                    .details
                    .as_ref()
                    .and_then(|details| details["retry_class"].as_str()),
                Some("terminal")
            );
        }
        SpokeResult::Ok(_) => panic!("reflection state is not served in N3"),
    }
    match adapter
        .create_export(
            &principal,
            dto::<MemoryExportsCreateRequest>(json!({
                "space_id": space,
                "idempotency_key": "remote-open-export",
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => {
            assert_eq!(reject.code, SpokeRejectCode::CapabilityPortMissing);
            assert_eq!(
                reject
                    .details
                    .as_ref()
                    .and_then(|details| details["retry_class"].as_str()),
                Some("terminal")
            );
        }
        SpokeResult::Ok(_) => panic!("the transfer family is not served in N3"),
    }

    let pool = provider_pool(&fixture_home).await;
    assert_eq!(
        provider_receipts(&pool, "memory.reflections.state").await,
        0
    );
    assert_eq!(provider_receipts(&pool, "memory.exports.create").await, 0);
    adapter.close();
    wait_for_closed(&responder).await;
    pool.close().await;
}

/// An UNEXPECTED transport loss (the provider's socket is torn down under a
/// live, open Core binding — the client never asked to close), then a reconnect
/// with the SAME persisted seed.
///
/// The signed consumer receives the ORIGINAL receipt tuple and the ORIGINAL
/// candidate both from the same-key replay response and from `memory.receipts.get`;
/// the product Core path is kept as complementary evidence, and the client pin
/// is read from the D18 namespace facts the actual `MemoryProviderBinding`
/// produced (provider rows alone are not the client pin). No C-089 cutover is
/// claimed.
#[tokio::test]
async fn unexpected_transport_loss_and_signed_replay_keep_the_original_receipt() {
    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);
    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory =
        Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id.clone()));
    let env = Env::boot("R5 Loss Owner").await;
    env.write_credential(&CLIENT_SEED);
    let pool = provider_pool(&fixture_home).await;
    let generator_before = provider_generation(&pool).await;

    // The product Core session, with a control clone of its accepted socket.
    let (addr, task, core_control) = start_listener_with_control(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        HashMap::from([(client_peer.clone(), client_pubkey)]),
        5_000,
    )
    .await;
    let core = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the exact selection opens");
    let core_responder = responder_of(task).await;
    let core_control = core_control.await.expect("the accepted socket control");
    assert_eq!(
        core_responder.remote_peer_id().as_deref(),
        Some(client_peer.as_str())
    );

    // A real owned World/Character/binding and a binding-scoped capture: its
    // D18 fact is written from the actual pinned binding.
    let principal = core.active_principal().await.expect("active principal");
    assert_eq!(principal.creator_id(), env.creator_id);
    let world = create_world(&core, &principal, "R5 Loss World").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "R5 Loss Ada", &world).await;
    let character_body = character_capture_request(
        Some(&binding_id),
        "pend_r5_loss",
        "The chapter pivots from betrayal to alliance, with causal consequences.",
    );
    core.capture_character_pending_review(
        &principal,
        character_id.clone(),
        dto::<CaptureCharacterPendingReviewRequest>(character_body.clone()),
    )
    .await
    .expect("the binding-scoped capture");
    let facts_db = core_db(&env).await;
    let facts_before = binding_object_facts(&facts_db, &binding_id).await;
    facts_db.close().await;
    assert_eq!(facts_before.len(), 1, "one candidate fact");
    assert_eq!(
        facts_before[0].0, authority_host_id,
        "pinned authority host"
    );
    assert_eq!(facts_before[0].1, "1", "pinned authority generation");
    assert_eq!(facts_before[0].2, served_space, "pinned space");
    assert_eq!(
        facts_before[0].3, client_peer,
        "pinned public client identity"
    );
    assert_eq!(facts_before[0].4, character_id, "pinned Character bearer");
    assert_eq!(facts_before[0].5, binding_id, "pinned binding");
    assert_eq!(facts_before[0].6, "candidate", "fact kind");
    assert_eq!(facts_before[0].8, "live", "the committed candidate is live");

    let character_subject = legacy::character_subject_id(&character_id);
    let consumer_request = dto::<MemoryCaptureRequest>(json!({
        "space_id": served_space,
        "subject_id": character_subject,
        "content": { "text": "A second, consumer-owned capture for the same subject." },
        "capture_scope": { "visibility": "shared" },
        "idempotency_key": "r5-consumer-capture",
    }));

    // The signed consumer's own session and its original capture.
    let (consumer_addr, consumer_task, consumer_control) = start_listener_with_control(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        HashMap::from([(client_peer.clone(), client_pubkey)]),
        5_000,
    )
    .await;
    let consumer = dial_adapter(consumer_addr, CLIENT_SEED).await;
    let consumer_responder = responder_of(consumer_task).await;
    let consumer_control = consumer_control.await.expect("the consumer socket control");
    let consumer_principal = principal_label(&principal_id);
    let original = match consumer
        .capture(&consumer_principal, consumer_request.clone())
        .await
    {
        SpokeResult::Ok(response) => response_json(&response),
        SpokeResult::Reject(reject) => panic!("the consumer capture commits: {reject:?}"),
    };
    let original_tuple = receipt_tuple(&original);
    let original_candidates = original["result"]["candidate_ids"].clone();
    assert_eq!(
        original_candidates.as_array().map(Vec::len),
        Some(1),
        "one original candidate"
    );

    // The provider's complete effect set and the client-pin namespace.
    let effects_before = provider_effect_snapshot(&pool).await;
    assert_eq!(
        committed_receipts(&pool, "memory.capture").await.len(),
        2,
        "one product receipt and one consumer receipt"
    );
    let admissions_db = core_db(&env).await;
    let admissions_before = binding_admissions(&admissions_db, &binding_id).await;
    admissions_db.close().await;
    assert!(
        admissions_before.is_empty(),
        "a definite committed capture retires exactly its own admission"
    );

    // UNEXPECTED LOSS: the provider's accepted sockets are torn down under the
    // still-open client bindings. The test never calls a client close here.
    assert!(!core.is_closing(), "the client did not begin closing");
    core_control
        .shutdown(Shutdown::Both)
        .expect("tear down the core socket");
    consumer_control
        .shutdown(Shutdown::Both)
        .expect("tear down the consumer socket");
    wait_for_closed(&core_responder).await;
    wait_for_closed(&consumer_responder).await;

    // The live bindings now observe a broken transport instead of a graceful
    // client shutdown.
    let after_loss = core
        .list_pending_reviews(&principal, None, 50)
        .await
        .err()
        .expect("the product binding observes the broken transport");
    assert!(
        matches!(after_loss, CoreError::MemoryProviderRefused { .. }),
        "the broken transport reaches the product as a classified refusal: {after_loss:?}"
    );
    assert!(!core.is_closing(), "the client still did not begin closing");
    match consumer
        .get_receipt(
            &consumer_principal,
            dto::<MemoryReceiptsGetRequest>(json!({
                "space_id": served_space,
                "receipt_id": original_tuple.0,
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => {
            assert_eq!(
                reject.code,
                SpokeRejectCode::InternalError,
                "the broken transport fails closed"
            );
            let kind = reject
                .details
                .as_ref()
                .and_then(|details| details.get("kind"))
                .and_then(Value::as_str);
            assert!(
                matches!(kind, Some("transport" | "session_closed")),
                "the SDK classifies the loss as a broken session: {kind:?}"
            );
        }
        SpokeResult::Ok(_) => panic!("the dead consumer session cannot answer"),
    }
    assert!(core.close().await.expect("close").cleanup_confirmed);
    consumer.close();

    // Reconnect with the SAME persisted seed: same peer, same generation.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let reopened = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the reconnect opens a fresh binding");
    let reopened_responder = responder_of(task).await;
    assert_eq!(
        reopened_responder.remote_peer_id().as_deref(),
        Some(client_peer.as_str()),
        "session ids are never identity: the same seed is"
    );
    assert_eq!(
        provider_generation(&pool).await,
        generator_before,
        "the same authority generation is served by the reconnect"
    );

    // Product path (complementary): the same binding-scoped capture replays.
    let reopened_principal = reopened.active_principal().await.expect("principal");
    reopened
        .capture_character_pending_review(
            &reopened_principal,
            character_id.clone(),
            dto::<CaptureCharacterPendingReviewRequest>(character_body),
        )
        .await
        .expect("the product replay is answered");

    // Signed consumer path: the same-key replay returns the ORIGINAL receipt
    // and the ORIGINAL candidate, and `receipts.get` serves the same tuple.
    let (consumer_addr, consumer_task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let consumer = dial_adapter(consumer_addr, CLIENT_SEED).await;
    let consumer_responder = responder_of(consumer_task).await;
    let replay = match consumer
        .capture(&consumer_principal, consumer_request.clone())
        .await
    {
        SpokeResult::Ok(response) => response_json(&response),
        SpokeResult::Reject(reject) => panic!("the same-key replay is answered: {reject:?}"),
    };
    assert_eq!(
        receipt_tuple(&replay),
        original_tuple,
        "the replay response carries the ORIGINAL receipt tuple"
    );
    assert_eq!(
        replay["result"]["candidate_ids"], original_candidates,
        "the replay names the ORIGINAL candidate"
    );
    let read_back = match consumer
        .get_receipt(
            &consumer_principal,
            dto::<MemoryReceiptsGetRequest>(json!({
                "space_id": served_space,
                "receipt_id": original_tuple.0,
            })),
        )
        .await
    {
        SpokeResult::Ok(response) => response_json(&response),
        SpokeResult::Reject(reject) => panic!("receipts.get answers: {reject:?}"),
    };
    assert_eq!(
        read_back["receipt"]["receipt_id"],
        json!(original_tuple.0),
        "receipts.get serves the original receipt id"
    );
    assert_eq!(
        read_back["receipt"]["result"]["candidate_ids"], original_candidates,
        "receipts.get names the original candidate"
    );

    // Neither path committed a second effect, and the client pin stayed the
    // same namespace across the reconnect.
    assert_eq!(
        provider_effect_snapshot(&pool).await,
        effects_before,
        "the replays added no receipt, candidate or record"
    );
    let state = core_db(&env).await;
    assert_eq!(
        binding_object_facts(&state, &binding_id).await,
        facts_before,
        "the D18 fact namespace (authority, generation, space, client identity) is unchanged"
    );
    let admissions_after = binding_admissions(&state, &binding_id).await;
    assert_eq!(
        admissions_after, admissions_before,
        "the lost attempt's admission is unchanged; no new pin namespace appeared"
    );
    state.close().await;
    assert!(
        !env.local_provider_store().exists(),
        "no reconnect fell back to the local provider"
    );

    assert!(reopened.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&reopened_responder).await;
    consumer.close();
    wait_for_closed(&consumer_responder).await;
    pool.close().await;
}

/// The stable identity of one receipt body: id, idempotency key, payload
/// digest, requester and the candidate ids it names.
fn receipt_tuple(receipt: &Value) -> (String, String, String, String) {
    (
        receipt["receipt_id"]
            .as_str()
            .expect("receipt id")
            .to_string(),
        receipt["idempotency_key"]
            .as_str()
            .expect("receipt key")
            .to_string(),
        receipt["payload_digest"]
            .as_str()
            .expect("receipt digest")
            .to_string(),
        receipt["requester_principal_id"]
            .as_str()
            .expect("receipt requester")
            .to_string(),
    )
}

/// A scoped grant cannot be amplified by the client, and neither the payload's
/// creator nor a session label is authority: every refusal leaves the provider's
/// receipts and candidates exactly as they were.
#[tokio::test]
async fn scoped_grants_and_payload_identity_cannot_be_amplified() {
    let host_pubkey = pubkey(&HOST_SEED);
    let admin_peer = peer_id(&CLIENT_SEED);
    let scoped_peer = peer_id(&SCOPED_SEED);
    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let admin_principal = format!("prn_{admin_peer}");
    let scoped_principal = format!("prn_{scoped_peer}");
    provider
        .provision_operator_admin(&admin_principal)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(
        PeerDirectory::new()
            .with_peer(admin_peer.clone(), admin_principal.clone())
            .with_peer(scoped_peer.clone(), scoped_principal.clone()),
    );
    let env = Env::new();
    env.write_credential(&CLIENT_SEED);
    env.write_credential_named(SCOPED_CREDENTIAL_ID, &SCOPED_SEED, None);
    let pool = provider_pool(&fixture_home).await;

    // The admin opens its own signed session first: a grant is what makes the
    // space visible at all, so the scoped identity cannot even complete
    // admission before it holds one.
    let (admin_addr, admin_task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        admin_peer.clone(),
        pubkey(&CLIENT_SEED),
        5_000,
    )
    .await;
    let admin = dial_adapter(admin_addr, CLIENT_SEED).await;
    let admin_responder = responder_of(admin_task).await;
    issue_grant(
        &admin,
        &admin_principal,
        json!({
            "space_id": space,
            "principal_id": scoped_principal,
            "capabilities": ["records_read"],
            "idempotency_key": "scoped-read-grant",
        }),
    )
    .await;

    // The scoped identity opens its own binding: admission is per authenticated
    // peer, and its grant state decides every operation.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        scoped_peer.clone(),
        pubkey(&SCOPED_SEED),
        5_000,
    )
    .await;
    let scoped = open(
        &env,
        selection_for(
            &space,
            addr,
            &host_pubkey,
            SCOPED_CREDENTIAL_ID,
            vec![peer_id(&HOST_SEED)],
        ),
    )
    .await
    .expect("the scoped identity opens its binding");
    let scoped_responder = responder_of(task).await;
    assert_eq!(
        scoped_responder.remote_peer_id().as_deref(),
        Some(scoped_peer.as_str())
    );
    let scoped_label = scoped.active_principal().await.expect("principal");
    assert_eq!(
        scoped_label.creator_id(),
        CREATOR,
        "the scoped open serves the selected creator; provider identity comes from the authenticated peer"
    );

    // Its read-only grant never becomes capture: the provider answers with its
    // own typed refusal and commits nothing.
    let effects_before_reading = provider_effect_snapshot(&pool).await;
    let ungranted = scoped
        .create_pending_review(&scoped_label, capture_body(CAPTURE_PENDING))
        .await
        .expect_err("a read-only principal cannot capture");
    match &ungranted {
        CoreError::MemoryProviderRefused {
            code,
            family,
            retry_class,
            ..
        } => {
            assert_eq!(code, "NOT_AUTHORIZED");
            assert_eq!(*family, MemoryProviderRefusal::Forbidden);
            assert_eq!(retry_class, "terminal");
        }
        other => panic!("expected the classified provider refusal, got {other:?}"),
    }
    assert_eq!(
        provider_effect_snapshot(&pool).await,
        effects_before_reading,
        "the read-only refusal changed no receipt, candidate or record"
    );

    // The admin adds the one capture-only space grant over the same session; no
    // review capability travels with it.
    issue_grant(
        &admin,
        &admin_principal,
        json!({
            "space_id": space,
            "principal_id": scoped_principal,
            "capabilities": ["capture"],
            "idempotency_key": "scoped-capture-grant",
        }),
    )
    .await;

    // The granted capture runs exactly once...
    let captured = scoped
        .create_pending_review(&scoped_label, capture_body(CAPTURE_PENDING))
        .await
        .expect("the scoped capture grant admits the capture");
    assert!(captured.success);
    let candidates_after_capture = provider_candidates(&pool).await;
    assert_eq!(
        candidates_after_capture.len(),
        1,
        "the granted capture produced one candidate"
    );
    let capture_receipts = committed_receipts(&pool, "memory.capture").await;
    assert_eq!(
        capture_receipts.len(),
        1,
        "the granted capture committed one receipt"
    );
    assert_eq!(
        capture_receipts[0].requester, scoped_principal,
        "the receipt requester is the authenticated scoped principal"
    );
    assert_eq!(
        receipt_candidate_ids(&capture_receipts[0]),
        vec![candidates_after_capture[0].0.clone()],
        "the committed receipt names the original candidate it produced"
    );

    // ...but review is outside the grant: the dismissal is refused by the
    // provider and the candidate stays pending with no receipt of its own.
    // The FIRST denied attempt is a warm-up that materializes the adapter s own
    // idempotent `memory.subjects.ensure` (a legitimate provider write, not the
    // denied operation); the measured second attempt replays it and must add
    // nothing at all.
    let warm = scoped
        .delete_pending_review(&scoped_label, CAPTURE_PENDING.to_string())
        .await
        .expect_err("a capture-only grant cannot dismiss a candidate");
    assert!(
        matches!(warm, CoreError::MemoryProviderRefused { .. }),
        "the warm-up dismissal is the same typed refusal: {warm:?}"
    );
    let effects_before_dismissal = provider_effect_snapshot(&pool).await;
    let dismissal = scoped
        .delete_pending_review(&scoped_label, CAPTURE_PENDING.to_string())
        .await
        .expect_err("a capture-only grant cannot dismiss a candidate");
    match &dismissal {
        CoreError::MemoryProviderRefused { code, family, .. } => {
            assert_eq!(code, "NOT_AUTHORIZED");
            assert_eq!(*family, MemoryProviderRefusal::Forbidden);
        }
        other => panic!("expected the classified provider refusal, got {other:?}"),
    }
    let after_dismissal = provider_effect_snapshot(&pool).await;
    let added: Vec<&String> = after_dismissal
        .iter()
        .filter(|row| !effects_before_dismissal.contains(row))
        .collect();
    let removed: Vec<&String> = effects_before_dismissal
        .iter()
        .filter(|row| !after_dismissal.contains(row))
        .collect();
    assert!(
        added.is_empty() && removed.is_empty(),
        "the refused dismissal changed no receipt, candidate or record: added {added:?}, removed {removed:?}"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.dismiss").await,
        0,
        "the refused dismissal committed no receipt"
    );

    // A payload creator is not the requester: the product refuses the foreign
    // creator before any wire attempt, and nothing changes on the provider.
    let effects_before_spoof = provider_effect_snapshot(&pool).await;
    let spoofed = scoped
        .create_pending_review(
            &scoped_label,
            capture_body_as("pend_spoofed", "ctr_ffffffffffffffffffffffffffffffff"),
        )
        .await
        .expect_err("a payload creator is never authority");
    assert!(
        matches!(spoofed, CoreError::ForbiddenReason { .. }),
        "got {spoofed:?}"
    );
    assert_eq!(
        provider_effect_snapshot(&pool).await,
        effects_before_spoof,
        "the payload-identity refusal changed no receipt, candidate or record"
    );

    assert!(scoped.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&scoped_responder).await;
    admin.close();
    wait_for_closed(&admin_responder).await;
    pool.close().await;
}

/// The credential seed and the capability proof never reach a public surface:
/// the persisted configuration the real writer produced and the real reader
/// consumed, a real signed authority refusal, a complete public receipt read
/// back over the signed session, and the provider's durable bodies are all
/// searched for two distinct sentinels.
///
/// Log applicability: `memory_remote.rs` and `memory_credentials.rs` have no
/// tracing/log statement of their own and the refusal path returns a static
/// `CoreError`, so no exercised secret-bearing log sink exists to capture on this
/// call path. (`service.rs` has an unrelated workspace-path warning.) This is a
/// scoped statement, not a claim that Core logging is absent everywhere.
#[tokio::test]
async fn credential_sentinels_never_reach_config_errors_or_receipts() {
    // Distinct sentinels: the credential seed itself and the token's signature
    // material. Both are unique text no incidental value can reproduce.
    const SEED_SENTINEL: [u8; 32] = [0xA7; 32];
    const SIGNATURE_SENTINEL: &str = "sig-sentinel-0f3c9a-never-public";

    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&SEED_SENTINEL);
    let client_peer = peer_id(&SEED_SENTINEL);
    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory =
        Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id.clone()));
    let env = Env::new();
    let mut token = expired_token(&client_peer, &peer_id(&HOST_SEED));
    token["sig"] = json!(SIGNATURE_SENTINEL);
    env.write_credential_with_token(&SEED_SENTINEL, Some(&token));
    let seed_text = hex(&SEED_SENTINEL);
    let pool = provider_pool(&fixture_home).await;

    // The real public credential value, loaded through the public API: its
    // `Debug` must expose neither secret. This is the actual secret-bearing
    // value, not a stand-in selection.
    let credential = MemoryCredential::load(&env.user_home, CREDENTIAL_ID)
        .expect("the operator credential loads through the public API");
    let credential_debug = format!("{credential:?}");
    assert!(!credential_debug.contains(&seed_text));
    assert!(!credential_debug.contains(SIGNATURE_SENTINEL));

    // A real signed refusal *after* the credential was loaded and the signed
    // session established: the discovered space is not authoritative on this
    // authenticated host, and the classified error carries no credential
    // material either way it is rendered.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest("fixture-not-the-space-authority"),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let error = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .err()
        .expect("an authority mismatch fails the open");
    assert_eq!(coded(&error), "memory_remote_authority");
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(&seed_text));
    assert!(!rendered.contains(SIGNATURE_SENTINEL));
    let responder = responder_of(task).await;
    wait_for_closed(&responder).await;

    // The real configuration writer persists the remote selection; the real
    // reader (an open with no override) is what loads it below.
    let remote = selection(
        &served_space,
        "127.0.0.1:1".parse().expect("port"),
        &host_pubkey,
    );
    CoreHomeService::open(env.user_home.clone())
        .expect("open the real home service")
        .set_memory_provider_selection(&remote)
        .expect("the product config writer persists the selection");
    let persisted = fs::read_to_string(env.user_home.join(".nexus42").join("config.toml"))
        .expect("read the persisted configuration");
    assert!(
        persisted.contains("kind = \"remote\""),
        "the remote table persisted"
    );
    assert!(!persisted.contains(&seed_text));
    assert!(!persisted.contains(SIGNATURE_SENTINEL));

    // The admitted session really captures, so the provider's receipt and
    // candidate bodies and the public receipt are meaningful sinks.
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let remote = selection(&served_space, addr, &host_pubkey);
    CoreHomeService::open(env.user_home.clone())
        .expect("open the real home service")
        .set_memory_provider_selection(&remote)
        .expect("persist the served selection");
    let core = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("the persisted selection opens through the real reader");
    let responder = responder_of(task).await;
    let principal = core.active_principal().await.expect("principal");
    core.create_pending_review(&principal, capture_body(CAPTURE_PENDING))
        .await
        .expect("the admitted capture");

    let bodies = provider_body_text(&pool).await;
    assert!(
        !bodies.is_empty(),
        "the receipt/candidate sink is populated"
    );
    assert!(
        !bodies.contains(&seed_text),
        "the seed never entered a receipt"
    );
    assert!(
        !bodies.contains(SIGNATURE_SENTINEL),
        "the proof signature never entered a receipt"
    );

    // The complete public receipt, read by the same authenticated identity over
    // its own signed session.
    let committed = committed_receipts(&pool, "memory.capture").await;
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].requester, principal_id);
    let (consumer_addr, consumer_task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let consumer = dial_adapter(consumer_addr, SEED_SENTINEL).await;
    let consumer_responder = responder_of(consumer_task).await;
    let receipt = match consumer
        .get_receipt(
            &principal_label(&principal_id),
            dto::<MemoryReceiptsGetRequest>(json!({
                "space_id": served_space,
                "receipt_id": committed[0].receipt_id,
            })),
        )
        .await
    {
        SpokeResult::Ok(receipt) => receipt,
        SpokeResult::Reject(reject) => panic!("the receipt is readable: {reject:?}"),
    };
    let receipt_text = response_json(&receipt).to_string();
    assert!(!receipt_text.contains(&seed_text));
    assert!(!receipt_text.contains(SIGNATURE_SENTINEL));

    // Neither did the workspace state database grow a copy of the secret.
    let state_db = fs::read(env.state_db()).expect("read the workspace state database");
    let state_text = String::from_utf8_lossy(&state_db);
    assert!(!state_text.contains(&seed_text));
    assert!(!state_text.contains(SIGNATURE_SENTINEL));

    assert!(core.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&responder).await;
    consumer.close();
    wait_for_closed(&consumer_responder).await;
    pool.close().await;
}

/// The pinned SDK's own capability-token authority: a minimal test-host
/// admission decorator verifies the exact proof the credential carries with
/// `spoke_connect::core::verify_capability_token` (real JCS/Ed25519, trusted
/// issuer, session subject, audience and expiry) against the SDK-authenticated
/// peer, and the real host refuses each invalid proof with no effect. A valid,
/// otherwise identical proof opens and proceeds into a real capture.
///
/// The proof is structurally valid in every case, so the loader accepts it; the
/// refusal is the verifier's, not a local shape rule or a fabricated class.
#[tokio::test]
async fn capability_token_verification_refuses_expired_and_forged_proofs() {
    const ISSUER_SEED: [u8; 32] = [0x77; 32];

    let host_pubkey = pubkey(&HOST_SEED);
    let host_peer = peer_id(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);
    let principal_id = format!("prn_{client_peer}");
    let issuer_peer = peer_id(&ISSUER_SEED);
    let other_peer = peer_id(&OTHER_SEED);
    let now = now_seconds();

    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory =
        Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id.clone()));
    let pool = provider_pool(&fixture_home).await;
    let effects_before = provider_effect_snapshot(&pool).await;

    let valid = capability_proof_at(&ISSUER_SEED, &client_peer, &host_peer, now + 600, now);
    // Expired but otherwise validly signed: issued at epoch 0 with a 1000s exp,
    // so issuance succeeded and only the verifier's clock check rejects it.
    let expired = capability_proof_at(&ISSUER_SEED, &client_peer, &host_peer, 1_000, 0);
    // Valid signature, wrong session subject.
    let wrong_subject = capability_proof_at(&ISSUER_SEED, &other_peer, &host_peer, now + 600, now);
    // Valid signature, wrong verifying node.
    let wrong_audience =
        capability_proof_at(&ISSUER_SEED, &client_peer, &other_peer, now + 600, now);
    // Valid signature from an issuer this host does not trust.
    let untrusted_issuer =
        capability_proof_at(&OTHER_SEED, &client_peer, &host_peer, now + 600, now);
    // Valid signature over different claims than the ones presented.
    let mut forged = valid.clone();
    forged.claims.capabilities.push("admin".to_string());

    let cases = [
        ("expired", expired),
        ("wrong-subject", wrong_subject),
        ("wrong-audience", wrong_audience),
        ("untrusted-issuer", untrusted_issuer),
        ("forged-signature", forged),
    ];
    let valid_json = serde_json::to_value(&valid).expect("the proof serializes");
    for (name, proof) in &cases {
        // The credential carries the real proof; the host gate verifies exactly
        // that proof against the authenticated peer it is handed.
        let env = Env::new();
        let proof_json = serde_json::to_value(proof).expect("the proof serializes");
        env.write_credential_with_token(&CLIENT_SEED, Some(&proof_json));
        let (addr, task) = start_listener_token_gated(
            &provider,
            &directory,
            manifest(&authority_host_id),
            HashMap::from([(client_peer.clone(), client_pubkey)]),
            vec![issuer_peer.clone()],
            host_peer.clone(),
        )
        .await;
        let error = open(&env, selection(&served_space, addr, &host_pubkey))
            .await
            .err()
            .expect("the host verifier refuses the proof");
        assert_eq!(
            coded(&error),
            "memory_remote_capability",
            "{name} must be refused by the real verifier"
        );
        assert!(
            !format!("{error} {error:?}").contains("token"),
            "{name}: the static client classification carries no peer text"
        );
        let responder = responder_of(task).await;
        wait_for_closed(&responder).await;
        assert_eq!(
            provider_effect_snapshot(&pool).await,
            effects_before,
            "{name} left the provider's effect set untouched"
        );
        assert!(!env.local_provider_store().exists());

        // Positive control on the SAME home: the valid proof opens and the
        // signed session proceeds.
        env.write_credential_with_token(&CLIENT_SEED, Some(&valid_json));
        let (addr, task) = start_listener_token_gated(
            &provider,
            &directory,
            manifest(&authority_host_id),
            HashMap::from([(client_peer.clone(), client_pubkey)]),
            vec![issuer_peer.clone()],
            host_peer.clone(),
        )
        .await;
        let core = open(&env, selection(&served_space, addr, &host_pubkey))
            .await
            .expect("the valid proof opens");
        let responder = responder_of(task).await;
        assert!(
            core.close().await.expect("close").cleanup_confirmed,
            "{name}: the valid open closes its own admission"
        );
        wait_for_closed(&responder).await;
    }

    // The verifier must see the ACTUAL forwarded proof: an authenticated
    // session that carries none is refused, so removing or changing client
    // proof forwarding cannot leave this scenario green.
    let (addr, task) = start_listener_token_gated(
        &provider,
        &directory,
        manifest(&authority_host_id),
        HashMap::from([(client_peer.clone(), client_pubkey)]),
        vec![issuer_peer.clone()],
        host_peer.clone(),
    )
    .await;
    let no_token = dial_adapter(addr, CLIENT_SEED).await;
    let responder = responder_of(task).await;
    match no_token
        .capture(
            &principal_label(&principal_id),
            dto::<MemoryCaptureRequest>(json!({
                "space_id": served_space,
                "subject_id": legacy::creator_subject_id(CREATOR),
                "content": { "text": "A tokenless capture is refused by the host gate." },
                "capture_scope": { "visibility": "shared" },
                "idempotency_key": "r5-tokenless-capture",
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => {
            assert_eq!(reject.code, SpokeRejectCode::NotAuthorized);
        }
        SpokeResult::Ok(_) => panic!("a tokenless invocation must be refused"),
    }
    assert_eq!(provider_effect_snapshot(&pool).await, effects_before);
    no_token.close();
    wait_for_closed(&responder).await;

    // The valid proof also admits a real mutation, so "positive" is a served
    // operation rather than a no-throw open.
    let env = Env::new();
    env.write_credential_with_token(&CLIENT_SEED, Some(&valid_json));
    let (addr, task) = start_listener_token_gated(
        &provider,
        &directory,
        manifest(&authority_host_id),
        HashMap::from([(client_peer.clone(), client_pubkey)]),
        vec![issuer_peer.clone()],
        host_peer.clone(),
    )
    .await;
    let core = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the valid proof opens");
    let responder = responder_of(task).await;
    let principal = core.active_principal().await.expect("principal");
    core.create_pending_review(&principal, capture_body(CAPTURE_PENDING))
        .await
        .expect("the verified session serves a real capture");
    assert_eq!(provider_receipts(&pool, "memory.capture").await, 1);
    assert_eq!(provider_candidates(&pool).await.len(), 1);
    assert!(core.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&responder).await;
    pool.close().await;
}

/// Two DIRECTLY EXERCISED observer events with the same op/payload but different
/// proofs must irreversibly poison the seam: the first admission denies, and no
/// later consume or third observation can ever reach the real fixture — while a
/// fresh observer/session still admits one sequential valid invocation whose
/// effect is observable.
///
/// Scope: these events are test-host observations, not SDK-authenticated wire
/// evidence; the real signed sequential valid/expired/missing/forged controls
/// above supply that independent path.
#[tokio::test]
async fn overlapping_observations_poison_the_seam_without_provider_effect() {
    const ISSUER_SEED: [u8; 32] = [0x77; 32];
    let host_peer = peer_id(&HOST_SEED);
    let client_peer = peer_id(&CLIENT_SEED);
    let principal_id = format!("prn_{client_peer}");
    let issuer_peer = peer_id(&ISSUER_SEED);
    let now = now_seconds();

    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let space = provider.identity().space_id.clone();
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));
    let pool = provider_pool(&fixture_home).await;
    let effects_before = provider_effect_snapshot(&pool).await;

    let valid = capability_proof_at(&ISSUER_SEED, &client_peer, &host_peer, now + 600, now);
    let expired = capability_proof_at(&ISSUER_SEED, &client_peer, &host_peer, 1_000, 0);
    let valid_json = serde_json::to_value(&valid).expect("the proof serializes");

    let capture_payload = json!({
        "space_id": space,
        "subject_id": "sbj_observer_regression",
        "content": { "text": "An ambiguous observation must never commit this capture." },
        "capture_scope": { "visibility": "shared" },
        "idempotency_key": "r5-observer-ambiguity",
    });
    let envelope = |op: &str, nonce: &str, payload: &Value, proof: &Value| {
        json!({
            "op": op,
            "session_id": "sess_observer_regression",
            "request_id": nonce,
            "payload": payload,
            "auth": proof,
        })
    };
    let loopback = loopback_transport_pair();
    let observer = Arc::new(EnvelopeObserver::new(
        Arc::new(loopback.client) as Arc<dyn Transport>
    ));
    // The defect under test left the VALID observation behind after the first
    // refusal, so a later admission could consume it.
    observer.record(&envelope(
        "memory.capture",
        "req_expired",
        &capture_payload,
        &serde_json::to_value(&expired).expect("the proof serializes"),
    ));
    observer.record(&envelope(
        "memory.capture",
        "req_valid",
        &capture_payload,
        &valid_json,
    ));
    let poisoned = TokenGate {
        inner: provider.server(directory.admission()),
        observer: Arc::clone(&observer),
        trusted: vec![issuer_peer.clone()],
        host_peer: host_peer.clone(),
    };
    for attempt in 0..2 {
        match poisoned
            .invoke(&client_peer, "memory.capture", capture_payload.clone())
            .await
        {
            SpokeResult::Reject(reject) => assert_eq!(
                reject.code,
                SpokeRejectCode::NotAuthorized,
                "attempt {attempt} is refused"
            ),
            SpokeResult::Ok(_) => panic!("attempt {attempt} reached the provider"),
        }
    }
    // A later observation cannot revive the poisoned session.
    observer.record(&envelope(
        "memory.capture",
        "req_third",
        &capture_payload,
        &valid_json,
    ));
    match poisoned
        .invoke(&client_peer, "memory.capture", capture_payload.clone())
        .await
    {
        SpokeResult::Reject(reject) => assert_eq!(reject.code, SpokeRejectCode::NotAuthorized),
        SpokeResult::Ok(_) => panic!("a later observation revived the poisoned session"),
    }
    assert_eq!(
        provider_effect_snapshot(&pool).await,
        effects_before,
        "no ambiguous admission committed a receipt, candidate or record"
    );

    // A FRESH observer/session still serves one sequential valid invocation,
    // and the effect the poisoned path did not produce is observable.
    let loopback = loopback_transport_pair();
    let fresh = Arc::new(EnvelopeObserver::new(
        Arc::new(loopback.client) as Arc<dyn Transport>
    ));
    let fresh_gate = TokenGate {
        inner: provider.server(directory.admission()),
        observer: Arc::clone(&fresh),
        trusted: vec![issuer_peer],
        host_peer,
    };
    let ensure_payload = json!({
        "space_id": space,
        "subject_id": "sbj_observer_regression",
    });
    fresh.record(&envelope(
        "memory.subjects.ensure",
        "req_ensure",
        &ensure_payload,
        &valid_json,
    ));
    match fresh_gate
        .invoke(&client_peer, "memory.subjects.ensure", ensure_payload)
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the sequential ensure is admitted: {reject:?}"),
    }
    fresh.record(&envelope(
        "memory.capture",
        "req_fresh",
        &capture_payload,
        &valid_json,
    ));
    match fresh_gate
        .invoke(&client_peer, "memory.capture", capture_payload)
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the sequential capture is admitted: {reject:?}"),
    }
    assert_eq!(
        provider_receipts(&pool, "memory.capture").await,
        1,
        "the fresh sequential capture committed its receipt"
    );
    assert_eq!(
        provider_candidates(&pool).await.len(),
        1,
        "the fresh sequential capture produced its candidate"
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------
// Shared product WS transport close ownership (I-000259)
// ---------------------------------------------------------------------------

/// Dial the product transport to a raw peer address.
async fn raw_ws_transport(addr: SocketAddr) -> Arc<dyn Transport> {
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the dial is bounded")
    .expect("dial the raw peer");
    Arc::new(WsTransport::new(ws))
}

/// A peer that completes the WS upgrade and then never reads a frame, so the
/// transport's write path really parks on native backpressure (no sleep or
/// is-finished proxy). After the test has closed the transport it reads the
/// buffered frames to the end and reports whether the stream really ended.
async fn non_reading_peer() -> (SocketAddr, Arc<Notify>, tokio::task::JoinHandle<bool>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind the silent peer");
    let addr = listener.local_addr().expect("silent peer address");
    let probe = Arc::new(Notify::new());
    let peer_probe = Arc::clone(&probe);
    let peer = tokio::spawn(async move {
        let (tcp, _) = tokio::time::timeout(SETUP_BOUND, listener.accept())
            .await
            .expect("the product transport dials the silent peer")
            .expect("accept the product transport");
        let mut ws = tokio::time::timeout(SETUP_BOUND, tokio_tungstenite::accept_async(tcp))
            .await
            .expect("the upgrade is bounded")
            .expect("websocket upgrade");
        // Stay non-reading until the transport has closed.
        peer_probe.notified().await;
        tokio::time::timeout(SETUP_BOUND, async move {
            loop {
                match ws.next().await {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return true,
                    Some(Ok(_)) => continue,
                }
            }
        })
        .await
        .unwrap_or(false)
    });
    (addr, probe, peer)
}

/// A peer that echoes every binary envelope back, used as an unrelated positive
/// session while the stalled one is torn down.
async fn echo_peer() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind the echo peer");
    let addr = listener.local_addr().expect("echo peer address");
    let peer_task = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("accept the echo client");
        let mut ws = tokio_tungstenite::accept_async(tcp)
            .await
            .expect("echo upgrade");
        while let Some(message) = ws.next().await {
            match message {
                Ok(Message::Binary(bytes)) => {
                    if ws.send(Message::Binary(bytes)).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    (addr, peer_task)
}

/// The shared product `WsTransport` must not report close as done while a send
/// is parked on a non-reading peer: the close signal reaches the parked send,
/// both concurrent closers settle with the same truthful outcome, the send
/// terminates, and dropping the halves really ends the peer's stream. An
/// unrelated session keeps its idle receive, send/recv roundtrip and close.
#[tokio::test]
async fn product_ws_close_settles_a_parked_send_and_releases_the_peer() {
    let (addr, probe, peer) = non_reading_peer().await;
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the dial is bounded")
    .expect("dial the silent peer");
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));

    // Far more than any loopback socket buffer, so the write is really parked.
    let payload = vec![0x5a_u8; 8 * 1024 * 1024];
    let mut sending = transport.send(&payload);
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut sending)
            .await
            .is_err(),
        "the send must be parked on the non-reading peer before close"
    );

    // Close from separate tasks, like the SDK spawns its transport close, while
    // the caller still awaits the parked send.
    let closer = Arc::clone(&transport);
    let first_close = tokio::spawn(async move { closer.close().await });
    let closer = Arc::clone(&transport);
    let second_close = tokio::spawn(async move { closer.close().await });

    // The parked send terminates instead of pinning the sink half forever.
    let settled = tokio::time::timeout(Duration::from_secs(5), sending)
        .await
        .expect("the parked send must settle after close");
    assert_eq!(
        settled,
        Err(TransportError::Closed),
        "a send cancelled by close reports Closed, never success"
    );

    // Both concurrent closers settle, and neither reports close-start as a
    // completed clean close: the close frame cannot flush to a peer that
    // stopped reading, so the terminal error is surfaced.
    let first = tokio::time::timeout(SETUP_BOUND, first_close)
        .await
        .expect("the first close must settle")
        .expect("the close task must not panic");
    let second = tokio::time::timeout(SETUP_BOUND, second_close)
        .await
        .expect("the second close must settle")
        .expect("the close task must not panic");
    assert_eq!(
        first, second,
        "concurrent closers report the same settled outcome"
    );
    assert!(
        matches!(first, Err(TransportError::Io(_))),
        "an unflushed close frame is a surfaced terminal error, got {first:?}"
    );
    assert_eq!(
        transport.close().await,
        first,
        "a repeated close keeps the settled outcome"
    );

    // The physical halves really went away: the peer drains and sees the stream
    // end instead of the socket staying open.
    probe.notify_one();
    assert!(
        tokio::time::timeout(SETUP_BOUND, peer)
            .await
            .expect("the peer observes the end of the stream")
            .expect("the peer task must not panic"),
        "dropping both halves must end the peer's stream"
    );

    // An unrelated session keeps an idle receive, a normal roundtrip and a
    // normal close while/after the stalled one is torn down.
    let (echo_addr, echo_peer) = echo_peer().await;
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{echo_addr}")),
    )
    .await
    .expect("the echo dial is bounded")
    .expect("dial the echo peer");
    let echo: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let mut idle = echo.recv();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut idle)
            .await
            .is_err(),
        "an idle receive parks instead of failing"
    );
    echo.send(b"unrelated-session")
        .await
        .expect("the unrelated session still sends");
    assert_eq!(
        tokio::time::timeout(SETUP_BOUND, idle)
            .await
            .expect("the parked receive settles")
            .expect("the echo arrives"),
        b"unrelated-session"
    );
    echo.close().await.expect("a normal close succeeds");
    echo.close()
        .await
        .expect("a repeated normal close succeeds");
    tokio::time::timeout(SETUP_BOUND, echo_peer)
        .await
        .expect("the echo peer ends")
        .expect("the echo peer task must not panic");
}

/// Cancelling the SOLE raw close must not abandon the physical disposal, and the
/// result a later caller sees must be the real one — never a success cached for
/// an interrupted flush. Two real windows are covered: the closer waiting for
/// the half a parked send still holds, and the closer inside a stalled flush.
#[tokio::test]
async fn sole_cancelled_close_releases_the_halves_and_keeps_a_truthful_result() {
    // ── Window 1: the sole closer is cancelled while it waits for the sink
    // half that the parked send still owns. ──
    let (addr, probe, peer) = non_reading_peer().await;
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the dial is bounded")
    .expect("dial the silent peer");
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let payload = vec![0x5a_u8; 8 * 1024 * 1024];
    let mut sending = transport.send(&payload);
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut sending)
            .await
            .is_err(),
        "the send must be parked on the non-reading peer before close"
    );
    let mut closer = Box::pin(transport.close());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut closer)
            .await
            .is_err(),
        "the sole close must be waiting for the half the parked send owns"
    );
    drop(closer);
    // The close signal wakes the parked send; its caller then abandons it. Both
    // halves are still the transport's to release, and no second close runs.
    drop(sending);
    probe.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), peer)
            .await
            .expect("the peer must observe the end without a second close")
            .expect("the peer task must not panic"),
        "cancelling the sole close must not strand the socket halves"
    );
    // The later caller settles with the real outcome for this ordering: the
    // peer started draining, so the disposal's flush completes. What matters is
    // that it waits for the settled result instead of reporting an early flag.
    let late = tokio::time::timeout(SETUP_BOUND, transport.close())
        .await
        .expect("the later close settles");
    assert!(
        late.is_ok(),
        "the later caller sees the settled disposition, got {late:?}"
    );

    // ── Window 2: the sole closer is cancelled inside a stalled flush. ──
    let (addr, probe, peer) = non_reading_peer().await;
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the dial is bounded")
    .expect("dial the silent peer");
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let mut sending = transport.send(&payload);
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut sending)
            .await
            .is_err(),
        "the send must be parked on the non-reading peer before close"
    );
    let closer = tokio::spawn({
        let transport = Arc::clone(&transport);
        async move { transport.close().await }
    });
    // The close signal releases the parked send, so the closer can take both
    // halves and start the flush, which cannot finish (8 MiB queued, peer not
    // reading).
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), sending)
            .await
            .expect("the parked send settles"),
        Err(TransportError::Closed)
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    closer.abort();
    let _ = closer.await;
    // The peer stays non-reading, so the disposal's flush cannot complete: the
    // later caller must see that real terminal error, not a cached success for
    // the interrupted caller.
    let late = tokio::time::timeout(SETUP_BOUND, transport.close())
        .await
        .expect("the later close settles")
        .expect_err("an interrupted flush is not a clean close");
    assert!(
        matches!(late, TransportError::Io(_)),
        "an interrupted flush is never cached as success, got {late:?}"
    );
    // Only now drain the peer, which observes the physical release the
    // cancelled closer's teardown already performed.
    probe.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), peer)
            .await
            .expect("the peer observes the end after the cancelled flush")
            .expect("the peer task must not panic"),
        "the interrupted closer's halves must still be released"
    );
}

/// A public `send` future that was polled Pending and then RETAINED (never
/// repolled, never dropped) must not block the close: the physical teardown has
/// to settle and the same peer must observe the end while that future is still
/// held. A running ticker proves the runtime is not starved.
#[tokio::test]
async fn retained_pending_send_does_not_block_the_close() {
    let (addr, probe, peer) = non_reading_peer().await;
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the dial is bounded")
    .expect("dial the silent peer");
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let payload = vec![0x5a_u8; 8 * 1024 * 1024];
    let mut retained_send = Box::pin(transport.send(&payload));
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut retained_send)
            .await
            .is_err(),
        "the send must be parked on the non-reading peer before close"
    );

    // A second same-direction send really queues behind the retained holder
    // before the close.
    let queued = tokio::spawn({
        let transport = Arc::clone(&transport);
        async move { transport.send(b"queued-before-close").await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !queued.is_finished(),
        "the second send must be queued behind the retained holder"
    );

    let ticks = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let ticker = tokio::spawn({
        let ticks = Arc::clone(&ticks);
        async move {
            loop {
                ticks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    });

    // The runtime keeps making progress while the send stays unpolled.
    tokio::time::timeout(Duration::from_secs(2), async {
        while ticks.load(std::sync::atomic::Ordering::SeqCst) < 3 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("the unpolled send must not starve the runtime");
    let ticks_before_close = ticks.load(std::sync::atomic::Ordering::SeqCst);

    // The retained send is never polled or dropped from here on.
    let closer = tokio::spawn({
        let transport = Arc::clone(&transport);
        async move { transport.close().await }
    });
    let report = tokio::time::timeout(Duration::from_secs(10), closer)
        .await
        .expect("the close must settle while the send future is retained")
        .expect("the close task must not panic");
    assert!(
        matches!(report, Err(TransportError::Io(_))),
        "a non-reading peer leaves a real terminal close error, got {report:?}"
    );

    // The already-queued send and a NEW post-close send both observe Closed
    // BEFORE the retained holder is released — not a timeout.
    let queued_outcome = tokio::time::timeout(Duration::from_secs(5), queued)
        .await
        .expect("the queued send must settle after close")
        .expect("the queued task must not panic");
    assert_eq!(
        queued_outcome,
        Err(TransportError::Closed),
        "a send queued behind the retained holder observes Closed"
    );
    let late = tokio::time::timeout(Duration::from_secs(2), transport.send(b"late-after-close"))
        .await
        .expect("a new send after close must settle");
    assert_eq!(
        late,
        Err(TransportError::Closed),
        "a new send after close observes Closed without waiting for the slot"
    );
    assert!(
        ticks.load(std::sync::atomic::Ordering::SeqCst) >= 3,
        "the runtime kept making progress while the send stayed unpolled"
    );
    let _ = ticks_before_close;

    // The peer observes the physical end BEFORE the retained future is dropped.
    probe.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), peer)
            .await
            .expect("the peer must observe the end while the send is retained")
            .expect("the peer task must not panic"),
        "the retained send must not keep the socket open"
    );
    drop(retained_send);
    ticker.abort();
    let _ = ticker.await;
}

/// The same invariant for a public `recv` future polled Pending and retained.
#[tokio::test]
async fn retained_pending_recv_does_not_block_the_close() {
    let (addr, probe, peer) = non_reading_peer().await;
    let (ws, _response) = tokio::time::timeout(
        SETUP_BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the dial is bounded")
    .expect("dial the silent peer");
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let mut retained_recv = Box::pin(transport.recv());
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut retained_recv)
            .await
            .is_err(),
        "the recv must be parked before close"
    );

    // A second same-direction receive really queues behind the retained holder
    // before the close.
    let queued = tokio::spawn({
        let transport = Arc::clone(&transport);
        async move { transport.recv().await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !queued.is_finished(),
        "the second recv must be queued behind the retained holder"
    );

    let ticks = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let ticker = tokio::spawn({
        let ticks = Arc::clone(&ticks);
        async move {
            loop {
                ticks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    });

    // The runtime keeps making progress while the recv stays unpolled.
    tokio::time::timeout(Duration::from_secs(2), async {
        while ticks.load(std::sync::atomic::Ordering::SeqCst) < 3 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("the unpolled recv must not starve the runtime");
    let ticks_before_close = ticks.load(std::sync::atomic::Ordering::SeqCst);

    let closer = tokio::spawn({
        let transport = Arc::clone(&transport);
        async move { transport.close().await }
    });
    let report = tokio::time::timeout(Duration::from_secs(10), closer)
        .await
        .expect("the close must settle while the recv future is retained")
        .expect("the close task must not panic");
    assert!(
        report.is_ok(),
        "an idle close flushes cleanly, got {report:?}"
    );

    // The already-queued receive and a NEW post-close receive both observe
    // Closed BEFORE the retained holder is released — not a timeout.
    let queued_outcome = tokio::time::timeout(Duration::from_secs(5), queued)
        .await
        .expect("the queued recv must settle after close")
        .expect("the queued task must not panic");
    assert_eq!(
        queued_outcome,
        Err(TransportError::Closed),
        "a recv queued behind the retained holder observes Closed"
    );
    let late = tokio::time::timeout(Duration::from_secs(2), transport.recv())
        .await
        .expect("a new recv after close must settle");
    assert_eq!(
        late,
        Err(TransportError::Closed),
        "a new recv after close observes Closed without waiting for the slot"
    );
    assert!(
        ticks.load(std::sync::atomic::Ordering::SeqCst) >= 3,
        "the runtime kept making progress while the recv stayed unpolled"
    );
    let _ = ticks_before_close;

    probe.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), peer)
            .await
            .expect("the peer must observe the end while the recv is retained")
            .expect("the peer task must not panic"),
        "the retained recv must not keep the socket open"
    );
    drop(retained_recv);
    ticker.abort();
    let _ = ticker.await;
}

/// A peer that, after `probe`, drains every inbound envelope and records the
/// first byte of each Binary message, returning that wire order at stream end.
async fn draining_peer() -> (SocketAddr, Arc<Notify>, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let probe = Arc::new(Notify::new());
    let peer_probe = Arc::clone(&probe);
    let peer = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("accept");
        let mut ws = tokio_tungstenite::accept_async(tcp).await.expect("upgrade");
        peer_probe.notified().await;
        let mut order = Vec::new();
        while let Some(message) = ws.next().await {
            match message {
                Ok(Message::Binary(bytes)) => {
                    if let Some(first) = bytes.first() {
                        order.push(*first);
                    }
                }
                Ok(Message::Close(_)) | Err(_) | Ok(Message::Text(_)) => break,
                Ok(_) => {}
            }
        }
        order
    });
    (addr, probe, peer)
}

/// A peer that, after `probe`, sends two binary frames and then waits for the
/// connection to end.
async fn two_frame_peer() -> (SocketAddr, Arc<Notify>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let probe = Arc::new(Notify::new());
    let peer_probe = Arc::clone(&probe);
    let peer = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("accept");
        let mut ws = tokio_tungstenite::accept_async(tcp).await.expect("upgrade");
        peer_probe.notified().await;
        if ws
            .send(Message::Binary(b"one".to_vec().into()))
            .await
            .is_err()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
        let _ = ws.send(Message::Binary(b"two".to_vec().into())).await;
        while let Some(message) = ws.next().await {
            match message {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    (addr, probe, peer)
}

/// Real socket READINESS without any close must not evict a queued send: the
/// first operation stays Pending/retained, the second is queued, and draining
/// the peer (ordinary write readiness) must leave it queued until the first
/// completes, then it must really succeed in wire order.
#[tokio::test]
async fn live_write_readiness_does_not_close_a_queued_send() {
    let (addr, probe, peer) = draining_peer().await;
    let transport = raw_ws_transport(addr).await;
    let payload = vec![0x5a_u8; 8 * 1024 * 1024];
    let mut first = Box::pin(transport.send(&payload));
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut first)
            .await
            .is_err(),
        "the first send must be parked before close"
    );
    let queued = tokio::spawn({
        let transport = Arc::clone(&transport);
        async move { transport.send(b"queued-live").await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!queued.is_finished(), "the second send is queued");
    // Ordinary socket write readiness: the peer starts draining. No close.
    probe.notify_one();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !queued.is_finished(),
        "a live write-readiness wake must not evict a queued send"
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), first)
            .await
            .expect("the first send completes"),
        Ok(())
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), queued)
            .await
            .expect("the queued send completes")
            .expect("queued task"),
        Ok(()),
        "the queued send succeeds instead of reporting Closed"
    );
    transport.close().await.expect("normal close");
    let order = tokio::time::timeout(Duration::from_secs(5), peer)
        .await
        .expect("the peer ends")
        .expect("peer task");
    assert_eq!(
        order,
        vec![0x5a, b'q'],
        "wire order is preserved under live readiness"
    );
}

/// The same for real socket READ readiness: a queued receive must not be
/// evicted as Closed when the peer's frame wakes the shared notifier; the first
/// receive returns frame one and the queued one frame two, in order.
#[tokio::test]
async fn live_read_readiness_does_not_close_a_queued_recv() {
    let (addr, probe, peer) = two_frame_peer().await;
    let transport = raw_ws_transport(addr).await;
    let mut first = Box::pin(transport.recv());
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut first)
            .await
            .is_err(),
        "the first recv must be parked before close"
    );
    let queued = tokio::spawn({
        let transport = Arc::clone(&transport);
        async move { transport.recv().await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!queued.is_finished(), "the second recv is queued");
    // Ordinary socket read readiness: the peer sends its first frame. No close.
    probe.notify_one();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        !queued.is_finished(),
        "a live read-readiness wake must not evict a queued recv"
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), first)
            .await
            .expect("the first recv completes"),
        Ok(b"one".to_vec())
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), queued)
            .await
            .expect("the queued recv completes")
            .expect("queued task"),
        Ok(b"two".to_vec()),
        "the queued recv returns the second frame, in order"
    );
    transport.close().await.expect("normal close");
    tokio::time::timeout(Duration::from_secs(5), peer)
        .await
        .expect("the peer ends")
        .expect("peer task");
}

// ---------------------------------------------------------------------------
// The provider's advertised page bound and request-bound receipt identity
// ---------------------------------------------------------------------------

/// A provider that advertises a narrower page bound than the adapter's former
/// constant still serves a whole Core read: the pending page requests, and the
/// count walks them, at the limit this open negotiated with this provider.
///
/// The fixture's advertised bound and its enforced bound are the same constant,
/// so an over-large request is refused `INVALID_INPUT` — which is exactly what a
/// Core that ignored the advertised bound would hit on the second page.
#[tokio::test]
async fn a_narrower_provider_page_bound_still_serves_the_whole_core_read() {
    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);

    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));

    let home = Env::boot("Narrow Bound Owner").await;
    home.write_credential(&CLIENT_SEED);
    let (addr, host) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer,
        client_pubkey,
        5_000,
    )
    .await;
    let core = open(&home, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the exact selection opens against the served space");
    let principal = core.active_principal().await.expect("active principal");
    // The adapter's own open provisions the subject this bearer speaks for, so
    // the first read is the empty queue.
    assert_eq!(
        core.count_pending_reviews(&principal)
            .await
            .expect("the pinned binding serves the empty queue")
            .count,
        0,
    );

    // More pending candidates than the advertised bound: one page cannot hold
    // them, so a read that kept a hard-coded larger limit would be refused.
    let subject_id = legacy::creator_subject_id(&home.creator_id);
    let bound = nexus_memory_conformance::MAX_PAGE_SIZE.get();
    for index in 0..(bound + 5) {
        provider
            .stage_pending_candidate(
                &format!("cand_narrow_bound_{index}"),
                &subject_id,
                None,
                &format!("pending_narrow_bound_{index}"),
                &format!("A narrower page bound still serves row {index}."),
            )
            .await
            .expect("stage one pending candidate");
    }
    assert_eq!(
        core.count_pending_reviews(&principal)
            .await
            .expect("the negotiated bound serves every page")
            .count,
        i64::try_from(bound + 5).expect("the staged count fits"),
        "every pending candidate is reachable through the negotiated bound",
    );

    assert!(core.close().await.expect("close").cleanup_confirmed);
    settle_listener(host).await;
}

/// Rewrites the committed receipt one provider returns, so the Core can be shown
/// a schema-valid answer that belongs to another request. It changes no durable
/// state and every other answer passes through untouched.
struct MismatchedReceipt {
    inner: Arc<dyn RemoteMemoryService>,
    rewrite: Arc<AtomicBool>,
}

#[async_trait]
impl RemoteMemoryService for MismatchedReceipt {
    async fn invoke(&self, peer_id: &str, op: &str, payload: Value) -> SpokeResult<Value> {
        let answer = self.inner.invoke(peer_id, op, payload).await;
        if !self.rewrite.load(Ordering::SeqCst) || op != "memory.capture" {
            return answer;
        }
        match answer {
            SpokeResult::Ok(mut body) => {
                body["idempotency_key"] = json!("a-receipt-for-another-request");
                SpokeResult::Ok(body)
            }
            SpokeResult::Reject(reject) => SpokeResult::Reject(reject),
        }
    }
}

/// A schema-valid capture receipt that does not answer this request must not
/// publish its objects nor retire this attempt's protective admission: the
/// mismatch stays a static fail-closed error, and the binding therefore cannot
/// be removed while it still owns remote memory.
#[tokio::test]
async fn a_mismatched_capture_receipt_keeps_the_binding_protected() {
    let host_pubkey = pubkey(&HOST_SEED);
    let client_pubkey = pubkey(&CLIENT_SEED);
    let client_peer = peer_id(&CLIENT_SEED);
    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let served_space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the admin grant");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));
    let env = Env::boot("Receipt Identity Owner").await;
    env.write_credential(&CLIENT_SEED);

    let rewrite = Arc::new(AtomicBool::new(false));
    let (addr, task) = start_listener(
        Arc::new(MismatchedReceipt {
            inner: provider.server(directory.admission()),
            rewrite: Arc::clone(&rewrite),
        }),
        manifest(&authority_host_id),
        client_peer,
        client_pubkey,
        5_000,
    )
    .await;
    let core = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the exact selection opens");
    let principal = core.active_principal().await.expect("active principal");
    let world = create_world(&core, &principal, "Receipt Identity World").await;
    let (character_id, binding_id, _) =
        create_character(&core, &principal, "Receipt Identity Ada", &world).await;

    // The honest receipt first: this request's own candidate is published.
    core.capture_character_pending_review(
        &principal,
        character_id.clone(),
        dto::<CaptureCharacterPendingReviewRequest>(character_capture_request(
            Some(&binding_id),
            "pend_receipt_identity",
            "The honest receipt publishes exactly this request's candidate.",
        )),
    )
    .await
    .expect("the honest capture publishes its candidate");
    let facts_db = core_db(&env).await;
    let honest = binding_object_facts(&facts_db, &binding_id).await;
    assert_eq!(honest.len(), 1, "one live candidate fact");
    assert_eq!(honest[0].8, "live", "the committed candidate is live");

    // A receipt for another request: the mismatch fails closed, statically.
    rewrite.store(true, Ordering::SeqCst);
    let error = core
        .capture_character_pending_review(
            &principal,
            character_id.clone(),
            dto::<CaptureCharacterPendingReviewRequest>(character_capture_request(
                Some(&binding_id),
                "pend_receipt_mismatch",
                "A mismatched receipt must never publish this request's candidate.",
            )),
        )
        .await
        .expect_err("a mismatched receipt refuses the capture");
    match &error {
        CoreError::Internal { category } => assert!(
            category.starts_with("memory_capture_receipt"),
            "the static receipt-identity refusal, got {category}"
        ),
        other => panic!("expected the static receipt refusal, got {other:?}"),
    }
    let after = binding_object_facts(&facts_db, &binding_id).await;
    assert_eq!(after, honest, "a mismatch publishes no object fact");
    let admissions = binding_admissions(&facts_db, &binding_id).await;
    assert_eq!(
        admissions.len(),
        1,
        "the mismatched attempt keeps its admission, so the binding stays protected"
    );
    assert_eq!(
        admissions[0].0, env.creator_id,
        "the retained admission names its owner"
    );
    facts_db.close().await;

    assert!(core.close().await.expect("close").cleanup_confirmed);
    settle_listener(task).await;
}

/// A reachable decoder failure becomes a static wire refusal: the signed client
/// sees `INTERNAL_ERROR` with no provider storage or decoder text, while a
/// legitimate capture on the same subject still commits.
#[tokio::test]
async fn a_malformed_stored_document_never_reaches_the_wire() {
    const CANARY: &str = "NEXUS-CANARY-3f9a";
    let admin_peer = peer_id(&CLIENT_SEED);
    let fixture_home = TempDir::new().expect("fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&provider_database(&fixture_home))
            .await
            .expect("open the durable fixture"),
    );
    let space = provider.identity().space_id.clone();
    let authority_host_id = provider.identity().authority_host_id.clone();
    let admin_principal = format!("prn_{admin_peer}");
    provider
        .provision_operator_admin(&admin_principal)
        .await
        .expect("operator provisions the admin grant");
    let directory =
        Arc::new(PeerDirectory::new().with_peer(admin_peer.clone(), admin_principal.clone()));
    let env = Env::boot("Wire Error Owner").await;
    env.write_credential(&CLIENT_SEED);
    let pool = provider_pool(&fixture_home).await;

    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        admin_peer,
        pubkey(&CLIENT_SEED),
        5_000,
    )
    .await;
    let core = open(&env, selection(&space, addr, &pubkey(&HOST_SEED)))
        .await
        .expect("the exact selection opens");
    let principal = core.active_principal().await.expect("active principal");
    // The adapter's own open provisions the subject this bearer speaks for.
    core.count_pending_reviews(&principal)
        .await
        .expect("the pinned binding serves the empty queue");
    let subject = legacy::creator_subject_id(&env.creator_id);

    // The legitimate positive on that subject, over the same signed session.
    let capture = |key: &str, text: &str| {
        dto::<MemoryCaptureRequest>(json!({
            "space_id": space.clone(),
            "subject_id": subject.clone(),
            "content": { "text": text },
            "capture_scope": { "visibility": "shared" },
            "idempotency_key": key,
        }))
    };
    // A signed session of its own: one listener admits one established session,
    // so the consumer dials a second listener for the same provider/principal.
    let (consumer_addr, consumer_task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        peer_id(&CLIENT_SEED),
        pubkey(&CLIENT_SEED),
        5_000,
    )
    .await;
    let consumer = dial_adapter(consumer_addr, CLIENT_SEED).await;
    let consumer_principal = principal_label(&admin_principal);
    match consumer
        .capture(
            &consumer_principal,
            capture("canary-honest", "An honest capture."),
        )
        .await
    {
        SpokeResult::Ok(_) => {}
        SpokeResult::Reject(reject) => panic!("the honest capture commits: {reject:?}"),
    }

    // A stored document the provider cannot decode, carrying a sentinel that
    // must never appear on the wire.
    sqlx::query(
        "UPDATE subjects SET fenced_contexts_json = ? WHERE space_id = ? AND subject_id = ?",
    )
    .bind(format!("{{\"sentinel\":\"{CANARY}\"}}"))
    .bind(&space)
    .bind(&subject)
    .execute(&pool)
    .await
    .expect("corrupt one stored document");
    let reject = match consumer
        .capture(
            &consumer_principal,
            capture("canary-corrupt", "This request reaches the decoder."),
        )
        .await
    {
        SpokeResult::Reject(reject) => reject,
        SpokeResult::Ok(body) => panic!("a decoder failure must not commit: {body:?}"),
    };
    let wire = json!({
        "code": reject.code.as_str(),
        "message": reject.message.clone(),
        "details": reject.details.clone(),
    })
    .to_string();
    assert_eq!(
        reject.code.as_str(),
        "INTERNAL_ERROR",
        "a decoder failure is the fixture's retryable internal"
    );
    for leaked in [CANARY, "sqlx", "serde", "invalid type", "expected value"] {
        assert!(
            !wire.contains(leaked),
            "the wire answer leaked provider internals ({leaked}): {wire}"
        );
    }

    pool.close().await;
    assert!(core.close().await.expect("close").cleanup_confirmed);
    settle_listener(task).await;
    settle_listener(consumer_task).await;
}
