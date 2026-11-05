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
use tokio::sync::Notify;

use nexus_contracts::generated::daemon_api::memory::create_pending_review_request::CreatePendingReviewRequest;
use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreResult, CoreService, MemoryProviderRefusal,
    MemoryProviderSelection, RemoteMemoryProviderSelection,
};
use nexus_memory_conformance::{FixtureMemoryProvider, PeerDirectory};
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponder, ConnectResponderOptions,
    ConnectResponderState, RemoteAdapter, RemoteAdapterOptions, RemoteIdentity, RemoteMemoryService,
    RemoteServePorts, RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, MemoryExportsCreateRequest,
    MemoryGrantsGrantRequest, MemoryReflectionsStateRequest, Principal, Relation, Rule, Scope,
    TimelineEvent,
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
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let allowlist = peers.keys().cloned().collect();
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
    (addr, task)
}

// ---------------------------------------------------------------------------
// The disposable user home and the remote selection
// ---------------------------------------------------------------------------

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
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
        let dir = self
            .user_home
            .join(".nexus42")
            .join("memory-credentials");
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
        nexus_home_layout::workspace_state_db_path(&self.user_home, CREATOR, WORKSPACE)
    }

    /// The local provider's private file root for this workspace. Its absence
    /// is the observable that no local provider was constructed.
    fn local_provider_store(&self) -> PathBuf {
        nexus_home_layout::operational_workspace_dir(&self.user_home, CREATOR, WORKSPACE)
            .join("memory-store")
    }

    /// The legacy bearer file the remote refusal must not touch.
    fn legacy_bearer(&self) -> PathBuf {
        self.user_home
            .join(".nexus42")
            .join("creators")
            .join(CREATOR)
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

/// The provider's committed capture receipts: id, authenticated requester,
/// idempotency key and payload digest — the durable receipt surface a replay
/// must answer unchanged.
async fn committed_capture_receipts(
    pool: &SqlitePool,
) -> Vec<(String, String, Option<String>, String)> {
    sqlx::query(
        "SELECT receipt_id, requester_principal_id, idempotency_key, payload_digest \
         FROM receipts WHERE op = 'memory.capture' AND status = 'committed' \
         ORDER BY receipt_id",
    )
    .fetch_all(pool)
    .await
    .expect("provider capture receipts")
    .into_iter()
    .map(|row| {
        (
            row.get(0),
            row.get(1),
            row.get::<Option<String>, _>(2),
            row.get(3),
        )
    })
    .collect()
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

/// The client-side admission boundaries: a credential that is not the published
/// proof shape, an unpinned/allowlist-mismatched host, an untrusted dialer, and
/// a host that serves the memory sub-families without the umbrella. Each one
/// fails the whole open with its static class, and none leaves an effect.
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
    let dead_endpoint: SocketAddr = "127.0.0.1:9".parse().expect("dead endpoint");

    // A capability token that is not the published `{v, claims, sig}` shape:
    // the loader refuses the whole open before any socket is dialed.
    let malformed = Env::new();
    malformed.write_credential_with_token(&CLIENT_SEED, Some(&json!({ "v": 1 })));
    let error = open(&malformed, selection(&served_space, dead_endpoint, &host_pubkey))
        .await
        .err()
        .expect("a malformed token refuses the open");
    assert_eq!(
        coded(&error),
        "memory_credential_invalid",
        "the credential refusal is static and precedes the dial"
    );

    // A structurally valid but already expired proof is NOT refused here:
    // §3.1 is structural validation, so the refusal that follows is the
    // transport's. Expiry belongs to the verifying host.
    let expired = Env::new();
    expired.write_credential_with_token(
        &CLIENT_SEED,
        Some(&expired_token(&client_peer, &host_peer)),
    );
    let error = open(&expired, selection(&served_space, dead_endpoint, &host_pubkey))
        .await
        .err()
        .expect("a dead endpoint fails the open");
    assert_eq!(
        coded(&error),
        "memory_remote_unavailable",
        "token expiry is the host's verification, not a local structural refusal"
    );

    let env = Env::new();
    env.write_credential(&CLIENT_SEED);

    // The client's pinned host key is not the host's: the signed hello does not
    // verify, so no session is ever admitted.
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
    task.abort();

    // The host's key is pinned but it is not on the client's allowlist.
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
    task.abort();

    // The dialer's own key is not trusted by the host: it refuses the hello.
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
    task.abort();

    // `spoke-memory` is the required umbrella: a host that declares only the
    // reflection/transfer sub-families negotiates nothing this client speaks.
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
    responder_of(task).await;

    // No refusal above reached the provider's durable state or the local
    // provider, and the home still opens for real afterwards.
    assert!(committed_capture_receipts(&pool).await.is_empty());
    assert!(provider_candidates(&pool).await.is_empty());
    assert!(
        !env.local_provider_store().exists(),
        "no refusal falls back to the local provider"
    );
    let (addr, task) = start_listener(
        provider.server(directory.admission()),
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
        5_000,
    )
    .await;
    let recovered = open(&env, selection(&served_space, addr, &host_pubkey))
        .await
        .expect("the same home opens after every refusal released its own guards");
    responder_of(task).await;
    recovered.close().await.expect("close");
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
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id.clone()));
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

/// A real capture over the admitted transport, the transport dropped, and a
/// reconnect with the SAME persisted seed: the host authenticates the same peer,
/// the replay of the same idempotency key answers the ORIGINAL receipt, and the
/// provider still holds exactly one original candidate. The pinned generation
/// is observed from the provider's own authority row; no cutover or retarget is
/// claimed.
#[tokio::test]
async fn reconnect_replays_the_original_capture_receipt_and_candidate() {
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
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id.clone()));
    let env = Env::new();
    env.write_credential(&CLIENT_SEED);
    let pool = provider_pool(&fixture_home).await;

    // First admitted session: the loaded public peer id is the host's
    // authenticated dialer, and one capture commits one receipt and one
    // candidate.
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
        .expect("the exact selection opens");
    let responder = responder_of(task).await;
    assert_eq!(
        responder.remote_peer_id().as_deref(),
        Some(client_peer.as_str()),
        "the host authenticated the seed's public peer identity"
    );
    let principal = core.active_principal().await.expect("active principal");
    let body = capture_body(CAPTURE_PENDING);
    assert!(
        core.create_pending_review(&principal, body.clone())
            .await
            .expect("the admitted capture")
            .success
    );
    assert_eq!(
        core.count_pending_reviews(&principal)
            .await
            .expect("the pinned binding serves the memory family")
            .count,
        1
    );
    let receipts_before = committed_capture_receipts(&pool).await;
    assert_eq!(receipts_before.len(), 1, "exactly one committed receipt");
    assert_eq!(
        receipts_before[0].1, principal_id,
        "the receipt's requester is the authenticated peer's principal"
    );
    assert!(
        receipts_before[0]
            .2
            .as_deref()
            .is_some_and(|key| key.starts_with("e4capture:")),
        "the capture committed its own idempotency key: {:?}",
        receipts_before[0].2
    );
    let candidates_before = provider_candidates(&pool).await;
    assert_eq!(candidates_before.len(), 1, "exactly one original candidate");
    let generation_before = provider_generation(&pool).await;
    let receipt_key = receipts_before[0].2.clone();

    // Drop the transport through the product's own close and observe the host
    // side ending the session it admitted.
    assert!(core.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&responder).await;

    // Reconnect with the same persisted seed: same public peer id, same
    // principal, the provider's current (unchanged) generation.
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
        generation_before,
        "the same authority generation is served by the reconnect"
    );
    let reopened_principal = reopened.active_principal().await.expect("principal");
    assert_eq!(
        reopened_principal.creator_id(),
        principal.creator_id(),
        "the same selection resolves the same local principal"
    );
    assert_eq!(
        reopened_principal.workspace_slug(),
        principal.workspace_slug()
    );

    // Replay the SAME capture: same idempotency key, original receipt, one
    // original candidate.
    let replay = reopened
        .create_pending_review(&reopened_principal, body)
        .await
        .expect("the same-key replay is answered");
    assert_eq!(replay.pending_id.as_str(), CAPTURE_PENDING);
    let receipts_after = committed_capture_receipts(&pool).await;
    assert_eq!(
        receipts_after, receipts_before,
        "the replay answers the ORIGINAL receipt instead of committing a second"
    );
    assert_eq!(
        receipts_after[0].2, receipt_key,
        "the replay reused the original idempotency key"
    );
    assert_eq!(
        provider_candidates(&pool).await,
        candidates_before,
        "still exactly one original candidate"
    );
    assert_eq!(
        reopened
            .count_pending_reviews(&reopened_principal)
            .await
            .expect("the reconnected binding serves the memory family")
            .count,
        1
    );
    assert!(reopened.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&reopened_responder).await;
    pool.close().await;
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
    assert_eq!(provider_receipts(&pool, "memory.capture").await, 0);
    assert!(provider_candidates(&pool).await.is_empty());

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
    assert_eq!(candidates_after_capture.len(), 1);
    assert_eq!(provider_receipts(&pool, "memory.capture").await, 1);

    // ...but review is outside the grant: the dismissal is refused by the
    // provider and the candidate stays pending with no receipt of its own.
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
    assert_eq!(provider_candidates(&pool).await, candidates_after_capture);
    assert_eq!(
        provider_receipts(&pool, "memory.candidates.dismiss").await,
        0,
        "the refused dismissal committed no receipt"
    );

    // A payload creator is not the requester: the product refuses the foreign
    // creator before any wire attempt, and nothing changes on the provider.
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
    assert_eq!(provider_receipts(&pool, "memory.capture").await, 1);
    assert_eq!(provider_candidates(&pool).await, candidates_after_capture);

    assert!(scoped.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&scoped_responder).await;
    admin.close();
    wait_for_closed(&admin_responder).await;
    pool.close().await;
}

/// The credential seed and the capability proof never reach a public surface:
/// the selection's `Debug`, a real refusal, and the provider's durable receipt
/// and candidate bodies are all searched for two distinct sentinels.
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
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id.clone()));
    let env = Env::new();
    let mut token = expired_token(&client_peer, &peer_id(&HOST_SEED));
    token["sig"] = json!(SIGNATURE_SENTINEL);
    env.write_credential_with_token(&SEED_SENTINEL, Some(&token));
    let seed_text = hex(&SEED_SENTINEL);

    // The selection may be logged: it shows ids and the sanitized endpoint, and
    // never the credential file or its proof.
    let logged = selection(
        &served_space,
        "127.0.0.1:1".parse().expect("endpoint"),
        &host_pubkey,
    );
    let selection_debug = format!("{logged:?}");
    assert!(!selection_debug.contains(&seed_text));
    assert!(!selection_debug.contains(SIGNATURE_SENTINEL));

    // A real refused open with this credential: the static error carries no
    // credential material either way it is rendered.
    let dead_endpoint: SocketAddr = "127.0.0.1:9".parse().expect("dead endpoint");
    let error = open(&env, selection(&served_space, dead_endpoint, &host_pubkey))
        .await
        .err()
        .expect("a dead endpoint fails the open");
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(&seed_text));
    assert!(!rendered.contains(SIGNATURE_SENTINEL));

    // The admitted session really captures, so the provider's receipt and
    // candidate bodies are a meaningful sink rather than an empty table.
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
        .expect("a well-formed proof opens the signed session");
    let responder = responder_of(task).await;
    let principal = core.active_principal().await.expect("principal");
    core.create_pending_review(&principal, capture_body(CAPTURE_PENDING))
        .await
        .expect("the admitted capture");
    let pool = provider_pool(&fixture_home).await;
    let bodies = provider_body_text(&pool).await;
    assert!(!bodies.is_empty(), "the receipt/candidate sink is populated");
    assert!(!bodies.contains(&seed_text), "the seed never entered a receipt");
    assert!(
        !bodies.contains(SIGNATURE_SENTINEL),
        "the proof signature never entered a receipt"
    );

    // Neither did the workspace state database grow a copy of the secret.
    let state_db = fs::read(env.state_db()).expect("read the workspace state database");
    let state_text = String::from_utf8_lossy(&state_db);
    assert!(!state_text.contains(&seed_text));
    assert!(!state_text.contains(SIGNATURE_SENTINEL));

    assert!(core.close().await.expect("close").cleanup_confirmed);
    wait_for_closed(&responder).await;
    pool.close().await;
}
