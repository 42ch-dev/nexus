//! Shared harness for the N3 conformance matrix (families A–D).
//!
//! Two provider legs, one protocol surface:
//!
//! - [`Kind::Local`]: the real `nexus_core::memory_provider::LocalMemoryProvider`
//!   over its own workspace state database and workspace home.
//! - [`Kind::Remote`]: the independent, durable `FixtureMemoryProvider` over its
//!   own SQLite file, sharing no storage or policy with the local provider.
//!
//! Both are served by the published `spoke-connect` memory service face and
//! driven by two real `RemoteAdapter` sessions over real `127.0.0.1` WebSocket
//! transports with the SDK's signed hello and peer allowlist. Nothing here
//! echoes, forwards or rewrites a protocol answer: the bytes on the socket are
//! the SDK's own envelopes, and every assertion reads the provider's answer or
//! refusal.
//!
//! The shared surface is the point: one scenario body runs unchanged against
//! both providers, so any difference comes from the provider, never from a
//! different test path. Every scenario gets a fresh temporary directory, its own
//! provider file, its own two authenticated peers (operator-provisioned admin
//! plus a grant-only second principal) and its own durable state after restart.
//!
//! Operator seams (fault points, clock, retention sweep, capture park) are test
//! control only: they never stand in for the protocol answer under test, and
//! each one is the provider's own documented seam for that boundary.

#![allow(dead_code)] // each test target uses a subset of this harness

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::{Barrier, Notify};

use sqlx::Connection as _;

use nexus_contracts::{CoreRegisterCreatorRequest, SetActiveWorkspaceRequest};
use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::memory_provider::LocalMemoryProvider;
use nexus_core::{CoreAccess, CoreHomeService, CoreOpenOptions, CoreService};
use nexus_memory_conformance::{
    FaultHold, FaultHoldLost, FaultPoint, FixtureMemoryProvider, PeerDirectory, RetentionSweep,
    TargetHold,
};
use spoke_connect::core::{
    derive_peer_id_from_ed25519_pubkey, sign_hello_ed25519, verify_hello_ed25519, CoreError,
};
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponder, ConnectResponderOptions,
    ConnectResponderState, MemoryPortService, RemoteAdapter, RemoteAdapterError,
    RemoteAdapterOptions, RemoteIdentity, RemoteMemoryService, RemoteServePorts,
    RemoteServePortsComposite, Transport, TransportError,
};
use spoke_operations::spoke_schemas::connect::{ConnectHello, ConnectSession};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, Principal, Relation, Rule, Scope,
    TimelineEvent,
};
use spoke_operations::{
    FindingPort, HostManifestPort, KnowledgeEntryPort, MemoryPort, RelationPort, RuleQueryPort,
    ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult,
};

/// The provider implementation behind the shared protocol surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The real Nexus local provider on a temporary workspace home.
    Local,
    /// The independent durable fixture provider on its own SQLite file.
    Remote,
}

impl Kind {
    /// The `_local` / `_remote` suffix the matrix §3 naming convention carries.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

/// The operator/admin peer's Connect identity.
pub const ADMIN_SEED: [u8; 32] = [0x51; 32];
/// The grant-only second peer's Connect identity.
pub const PEER_SEED: [u8; 32] = [0x52; 32];
/// The provider host's Connect identity.
pub const HOST_SEED: [u8; 32] = [0x53; 32];

/// The one capability both ends negotiate. The gated families stay
/// unnegotiated, so their refusal is the real dispatch denial.
const MEMORY_CAPABILITIES: [&str; 1] = ["spoke-memory"];

/// Every wait in the harness is bounded.
pub const BOUND: Duration = Duration::from_secs(20);

/// The local provider's workspace slug.
const LOCAL_WORKSPACE: &str = "default";
/// The local provider's operator principal (its own provisioning identity).
const LOCAL_ADMIN_ID: &str = "ctr_c_matrix_admin";
/// The local provider's second, grant-only principal.
const LOCAL_PEER_ID: &str = "ctr_c_matrix_peer";

pub const SUBJECT: &str = "sbj_c_matrix";
pub const OTHER_SUBJECT: &str = "sbj_c_other";
pub const CONTEXT: &str = "ctx_c_harbor";
pub const OTHER_CONTEXT: &str = "ctx_c_market";
pub const TEXT: &str = "Mira promised to return before the northern gate opens at dawn.";

// ---------------------------------------------------------------------------
// A memory-only responder host
// ---------------------------------------------------------------------------

/// The baseline half of the responder ports face.
///
/// `RemoteServePorts` is a baseline-port supertrait; this suite serves
/// `spoke-memory` only, so every baseline call refuses honestly instead of
/// fabricating a baseline object. `spoke-baseline` is never negotiated.
struct MemoryOnlyHost;

fn no_baseline<T>() -> SpokeResult<T> {
    SpokeResult::Reject(SpokeReject {
        code: SpokeRejectCode::CapabilityPortMissing,
        message: "this host serves no baseline family".to_owned(),
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

fn manifest(host_id: &str) -> HostCapabilityManifest {
    manifest_with(host_id, &MEMORY_CAPABILITIES)
}

fn connect_manifest_with(host_id: &str, capabilities: &[&str]) -> ConnectHelloManifest {
    serde_json::from_value(
        serde_json::to_value(manifest_with(host_id, capabilities)).expect("manifest serializes"),
    )
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

pub fn principal(principal_id: &str) -> Principal {
    serde_json::from_value(json!({
        "principal_id": principal_id,
        "extensions": {},
    }))
    .expect("valid principal")
}

/// One signed responder session plus the dialing client and the transport ends,
/// so a restart awaits the real teardown instead of a state flag.
pub struct Endpoint {
    client: Arc<RemoteAdapter>,
    responder: Arc<ConnectResponder>,
    socket: Arc<dyn Transport>,
}

impl Endpoint {
    /// A typed session on this endpoint's authenticated client.
    pub const fn actor<'a>(&'a self, principal: &'a Principal, space_id: &'a str) -> Actor<'a> {
        Actor {
            client: &self.client,
            principal,
            space_id,
        }
    }

    pub async fn close(self) {
        if let Err(error) = self.settle().await {
            panic!("{error}");
        }
    }

    /// [`Self::close`] for an aggregating caller: every failure (transport close
    /// error or timeout, responder not `Closed`) is returned, and a timeout is
    /// never reported as a successful close.
    pub async fn settle(self) -> Result<(), String> {
        self.client.close();
        self.responder.close();
        match tokio::time::timeout(BOUND, self.socket.close()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                return Err(format!(
                    "the session transport close failed within the bound: {error}"
                ));
            }
            Err(_) => {
                return Err(
                    "the session transport close did not finish within the bound".to_owned(),
                );
            }
        }
        if self.responder.state() != ConnectResponderState::Closed {
            return Err(format!(
                "the responder session did not report Closed after the real transport cleanup (state {:?})",
                self.responder.state()
            ));
        }
        Ok(())
    }
}

/// Close one owned responder through its public route, bounded. The failure is
/// returned (not panicked) so a caller can settle every owner before asserting.
pub async fn close_responder(responder: Arc<ConnectResponder>) -> Result<(), String> {
    responder.close();
    let deadline = Instant::now() + BOUND;
    while responder.state() != ConnectResponderState::Closed {
        if Instant::now() >= deadline {
            return Err("the responder session did not reach Closed within the bound".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok(())
}

/// Serve one fresh loopback session for `seed` against `service`.
pub async fn open_endpoint(
    service: Arc<dyn RemoteMemoryService>,
    host_id: &str,
    seed: [u8; 32],
) -> Endpoint {
    open_endpoint_expecting(service, host_id, seed, pubkey(&HOST_SEED))
        .await
        .expect("the endpoint establishes with the published host key")
}

/// A real client transport that only forwards to the real `WsTransport` and
/// retains the first inbound envelope (the signed responder hello) for
/// independent protocol-level verification.
struct ObservedTransport {
    inner: Arc<dyn Transport>,
    first_inbound: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

#[async_trait]
impl Transport for ObservedTransport {
    async fn send(&self, envelope: &[u8]) -> Result<(), TransportError> {
        self.inner.send(envelope).await
    }

    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        let envelope = self.inner.recv().await?;
        let mut first = self
            .first_inbound
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if first.is_none() {
            *first = Some(envelope.clone());
        }
        drop(first);
        Ok(envelope)
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

/// The evidence of one real typed dial attempt that failed.
struct DialAttemptFailure {
    error: RemoteAdapterError,
    first_inbound: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    client_socket: Weak<ObservedTransport>,
}

/// A real setup stage that fails before an endpoint exists. Only the SDK's
/// signed-dial rejection is modelled; every other setup failure still panics,
/// so an `Err` proves the signed stage was reached and rejected.
#[derive(Debug)]
enum EndpointSetupFailure {
    /// The SDK's own dial error from signed responder-hello verification, with
    /// the observation that the responder hello really arrived and both socket
    /// owners observed by the listener side.
    Dial {
        attempt: DialAttemptFailure,
        server_socket: Weak<WsTransport<tokio::net::TcpStream>>,
    },
}

impl std::fmt::Debug for DialAttemptFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DialAttemptFailure")
            .field("error", &self.error)
            .field(
                "first_inbound_received",
                &self
                    .first_inbound
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl EndpointSetupFailure {
    /// A rejected host key is the SDK's handshake classification after the
    /// responder hello was really received: the dial reached signed
    /// responder-hello verification and that verification refused the key.
    fn is_signed_dial_rejection(&self) -> bool {
        match self {
            Self::Dial { attempt, .. } => {
                let received = attempt
                    .first_inbound
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some();
                received && matches!(attempt.error, RemoteAdapterError::Handshake(_))
            }
        }
    }
}

/// The real setup primitive behind [`open_endpoint`]: the same listener,
/// responder, dial and failure-settlement path, parameterised by the host key
/// the signed dial expects. The setup-failure control drives THIS primitive
/// with a wrong key (a deterministic genuine dial failure), so it exercises
/// the real listener/responder ownership instead of a lookalike.
async fn open_endpoint_expecting(
    service: Arc<dyn RemoteMemoryService>,
    host_id: &str,
    seed: [u8; 32],
    expected_host_pubkey: [u8; 32],
) -> Result<Endpoint, EndpointSetupFailure> {
    let (addr, host) = spawn_expected_endpoint_listener(service, host_id, seed).await;
    let port = addr.port();
    // The listener task is owned until the client side succeeds. A connect or
    // dial failure settles that exact task (abort + bounded join, closing a
    // responder/socket it already returned) and confirms the port release
    // before the original failure is re-raised; a join the bound did not
    // confirm is reported, never claimed.
    let client_side = std::panic::AssertUnwindSafe(dial_expected_endpoint(
        addr,
        seed,
        host_id,
        expected_host_pubkey,
    ))
    .catch_unwind()
    .await;
    let (socket, client) = match client_side {
        Ok(Ok(established)) => established,
        Ok(Err(attempt)) => {
            let server_socket = match settle_endpoint_listener(host, port).await {
                Ok(server_socket) => server_socket,
                Err(problem) => panic!(
                    "the endpoint setup failed ({attempt:?}) and its listener settlement also failed: {problem}"
                ),
            };
            return Err(EndpointSetupFailure::Dial {
                attempt,
                server_socket,
            });
        }
        Err(payload) => match settle_endpoint_listener(host, port).await {
            Ok(_) => std::panic::resume_unwind(payload),
            Err(problem) => {
                let original = panic_message(&*payload);
                panic!(
                    "the endpoint setup panicked ({original}) and its listener settlement also failed: {problem}"
                );
            }
        },
    };
    let served = host.await.expect("the listener task");
    Ok(Endpoint {
        client,
        responder: served.responder,
        socket,
    })
}

/// The listener-side owners and the weak observation of the server socket.
struct ServedEndpoint {
    socket: Arc<dyn Transport>,
    responder: Arc<ConnectResponder>,
    socket_weak: Weak<WsTransport<tokio::net::TcpStream>>,
}

/// Bind the ephemeral loopback listener and serve one signed responder task.
async fn spawn_expected_endpoint_listener(
    service: Arc<dyn RemoteMemoryService>,
    host_id: &str,
    seed: [u8; 32],
) -> (
    std::net::SocketAddr,
    tokio::task::JoinHandle<ServedEndpoint>,
) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let peer = peer_id(&seed);
    let host_manifest = manifest(host_id);
    let host = tokio::spawn(async move {
        let (tcp, address) = tokio::time::timeout(BOUND, listener.accept())
            .await
            .expect("a client connects to the localhost listener")
            .expect("accept the localhost peer");
        assert!(
            address.ip().is_loopback(),
            "the listener serves localhost only"
        );
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
        let typed = Arc::new(WsTransport::new(ws));
        let socket_weak = Arc::downgrade(&typed);
        let socket: Arc<dyn Transport> = typed;
        let ports: Arc<dyn RemoteServePorts + Send + Sync> = Arc::new(
            RemoteServePortsComposite::new(Arc::new(MemoryOnlyHost), None, None)
                .with_memory(service),
        );
        let responder = connect_responder(ConnectResponderOptions {
            transport: Arc::clone(&socket),
            identity: RemoteIdentity { seed: HOST_SEED },
            manifest: host_manifest,
            allowlist: vec![peer.clone()],
            peer_keys: HashMap::from([(peer, pubkey(&seed))]),
            ports: Some(ports),
            invoke_timeout_ms: None,
        })
        .await;
        ServedEndpoint {
            socket,
            responder,
            socket_weak,
        }
    });
    (addr, host)
}

/// Connect the local signed client and dial it against `expected_host_pubkey`.
/// The allowlist is derived from that same expected key, so a wrong key passes
/// the SDK's fail-closed precheck and reaches signed responder-hello
/// verification; the observed transport records that the hello really arrived.
async fn dial_expected_endpoint(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    host_id: &str,
    expected_host_pubkey: [u8; 32],
) -> Result<(Arc<dyn Transport>, Arc<RemoteAdapter>), DialAttemptFailure> {
    let (ws, _response) = tokio::time::timeout(
        BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the localhost websocket connect is bounded")
    .expect("connect the localhost websocket");
    let first_inbound = Arc::new(std::sync::Mutex::new(None));
    let inner: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let observed = Arc::new(ObservedTransport {
        inner,
        first_inbound: Arc::clone(&first_inbound),
    });
    let client_socket = Arc::downgrade(&observed);
    let expected_peer = derive_peer_id_from_ed25519_pubkey(&expected_host_pubkey);
    let dialed = connect_remote_adapter(RemoteAdapterOptions {
        transport: Arc::clone(&observed) as Arc<dyn Transport>,
        local_identity: RemoteIdentity { seed },
        local_manifest: manifest(&format!("{host_id}-client")),
        remote_pubkey: expected_host_pubkey,
        allowlist: vec![expected_peer],
        invoke_timeout_ms: None,
        capability_token: None,
    });
    let client = tokio::time::timeout(BOUND, dialed)
        .await
        .expect("the signed dial is bounded")
        .map_err(|error| DialAttemptFailure {
            error,
            first_inbound: Arc::clone(&first_inbound),
            client_socket: client_socket.clone(),
        })?;
    Ok((observed as Arc<dyn Transport>, client))
}

/// Settle the exact endpoint listener task after a client-side setup failure:
/// abort it, bounded-join it, close any responder/socket it already returned,
/// and confirm the listening port is released. An unconfirmed join is an
/// error, never a claimed join.
async fn settle_endpoint_listener(
    host: tokio::task::JoinHandle<ServedEndpoint>,
    port: u16,
) -> Result<Weak<WsTransport<tokio::net::TcpStream>>, String> {
    let mut server_socket: Weak<WsTransport<tokio::net::TcpStream>> = Weak::new();
    host.abort();
    match tokio::time::timeout(BOUND, host).await {
        Ok(Ok(served)) => {
            // The listener completed before the abort: close BOTH owners it
            // created (each bounded, every failure preserved) instead of
            // letting one close failure skip the other.
            let mut failures: Vec<String> = Vec::new();
            if let Err(error) = close_responder(served.responder).await {
                failures.push(format!(
                    "the setup listener responder close failed: {error}"
                ));
            }
            match tokio::time::timeout(BOUND, served.socket.close()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    failures.push(format!("the setup listener socket close failed: {error}"));
                }
                Err(_) => failures.push(
                    "the setup listener socket close did not finish within the bound".to_owned(),
                ),
            }
            if !failures.is_empty() {
                return Err(failures.join("; "));
            }
            server_socket = served.socket_weak;
        }
        Ok(Err(error)) if error.is_cancelled() => {}
        Ok(Err(error)) => return Err(format!("the setup listener task failed: {error}")),
        Err(_) => {
            return Err(
                "the setup listener task did not join within the bound; settlement unconfirmed"
                    .to_owned(),
            );
        }
    }
    match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(_) => Ok(server_socket),
        Err(error) => Err(format!(
            "the setup listener's port is not rebindable after settlement: {error}"
        )),
    }
}

// ---------------------------------------------------------------------------
// The two provider legs
// ---------------------------------------------------------------------------

/// Where a matrix run's durable state lives.
enum Home {
    Local { _home: TempDir, user_home: PathBuf },
    Remote { _home: TempDir, db: PathBuf },
}

enum Provider {
    Local(Arc<LocalMemoryProvider>),
    Fixture(Arc<FixtureMemoryProvider>),
}

fn build_service(
    provider: &Provider,
    directory: &Arc<PeerDirectory>,
) -> Arc<dyn RemoteMemoryService> {
    match provider {
        Provider::Local(provider) => {
            let cloned = Arc::clone(provider);
            let port: Arc<dyn MemoryPort + Send + Sync> = cloned;
            Arc::new(MemoryPortService::new(port, directory.admission()))
        }
        Provider::Fixture(provider) => provider.server(directory.admission()),
    }
}

/// Provision the durable provider for `home` and reopen it where needed.
async fn open_provider(home: &Home) -> Provider {
    match home {
        Home::Local { user_home, .. } => Provider::Local(Arc::new(
            LocalMemoryProvider::open(user_home.clone(), LOCAL_ADMIN_ID, LOCAL_WORKSPACE)
                .await
                .expect("open the local provider"),
        )),
        Home::Remote { db, .. } => Provider::Fixture(Arc::new(
            FixtureMemoryProvider::open(db)
                .await
                .expect("open the durable fixture provider"),
        )),
    }
}

/// Settle a provider owner after a failed setup step. The local pool drops with
/// its owner; the fixture's own pool close and its ownership wait are bounded
/// and returned, never panicked.
async fn settle_provider_setup(provider: Provider) -> Result<(), String> {
    match provider {
        Provider::Local(_provider) => Ok(()),
        Provider::Fixture(provider) => close_fixture_bounded(provider).await,
    }
}

/// The Local provider's existing operator controls, exposed only to let a test
/// prepare operator-only state on the SAME instance the signed SDK serves.
///
/// This is a test-harness arrangement over the provider's own existing
/// `test-hooks` seams; it invents no product getter, fault or wire behaviour.
pub struct LocalOperator {
    provider: Arc<LocalMemoryProvider>,
    file_root: PathBuf,
}

impl LocalOperator {
    /// Arm the provider's existing commit-boundary fault on the next mutation.
    pub fn arm_commit_fault(&self, after_commit: bool) {
        self.provider.inject_commit_fault(after_commit);
    }

    /// The provider's own published projection root (test-hooks control).
    pub fn file_root(&self) -> &std::path::Path {
        &self.file_root
    }
}

/// One matrix run: the provider, its signed service face and the two real
/// client sessions.
pub struct Matrix {
    kind: Kind,
    home: Home,
    provider: Provider,
    directory: Arc<PeerDirectory>,
    service: Arc<dyn RemoteMemoryService>,
    admin: Endpoint,
    peer: Endpoint,
    admin_principal: Principal,
    peer_principal: Principal,
    space_id: String,
    host_id: String,
    /// How far an earlier retention step moved the provider clock (local leg).
    advanced: AtomicI64,
}

/// Open the real local provider and its operator/peer mapping over a prepared
/// home (the migration/character preparation stayed with the caller).
/// `reflections: false` opens the deliberately-unadvertised shape (staging
/// spec §4 N4-R): describe omits `spoke-memory-reflection` and the three
/// reflection ops answer the not-served refusal.
async fn open_local_workspace(
    home_dir: TempDir,
    creator_id: Option<String>,
    reflections: bool,
) -> (
    Home,
    Provider,
    String,
    Arc<PeerDirectory>,
    Principal,
    Principal,
) {
    let user_home = home_dir.path().to_path_buf();
    let admin_id = creator_id.unwrap_or_else(|| LOCAL_ADMIN_ID.to_owned());
    let provider = if reflections {
        LocalMemoryProvider::open(user_home.clone(), &admin_id, LOCAL_WORKSPACE)
            .await
            .expect("open the local provider")
    } else {
        LocalMemoryProvider::open_unadvertised(user_home.clone(), &admin_id, LOCAL_WORKSPACE)
            .await
            .expect("open the unadvertised local provider")
    };
    let space_id = provider.space_id().to_owned();
    let directory = Arc::new(
        PeerDirectory::new()
            .with_peer(peer_id(&ADMIN_SEED), admin_id.as_str())
            .with_peer(peer_id(&PEER_SEED), LOCAL_PEER_ID),
    );
    (
        Home::Local {
            _home: home_dir,
            user_home,
        },
        Provider::Local(Arc::new(provider)),
        space_id,
        directory,
        principal(&admin_id),
        principal(LOCAL_PEER_ID),
    )
}

/// Open the durable fixture provider and its operator/peer mapping over a
/// prepared home.
async fn open_remote_workspace(
    home_dir: TempDir,
) -> (
    Home,
    Provider,
    String,
    Arc<PeerDirectory>,
    Principal,
    Principal,
) {
    let db = home_dir.path().join("fixture.sqlite");
    let provider = FixtureMemoryProvider::open(&db)
        .await
        .expect("open the durable fixture provider");
    let admin_id = format!("prn_{}", peer_id(&ADMIN_SEED));
    let second_id = format!("prn_{}", peer_id(&PEER_SEED));
    provider
        .provision_operator_admin(&admin_id)
        .await
        .expect("operator provisions the admin grant");
    let space_id = provider.identity().space_id.clone();
    let directory = Arc::new(
        PeerDirectory::new()
            .with_peer(peer_id(&ADMIN_SEED), admin_id.clone())
            .with_peer(peer_id(&PEER_SEED), second_id.clone()),
    );
    (
        Home::Remote {
            _home: home_dir,
            db,
        },
        Provider::Fixture(Arc::new(provider)),
        space_id,
        directory,
        principal(&admin_id),
        principal(&second_id),
    )
}

impl Matrix {
    /// A fresh run over the requested provider implementation.
    pub async fn start(kind: Kind) -> Self {
        Self::assemble(kind, TempDir::new().expect("temporary matrix home"), None, true).await
    }

    /// A local run whose provider deliberately withholds the
    /// `spoke-memory-reflection` sub-capability (staging spec §4 N4-R): the
    /// Connect dispatch gate refuses the reflection family, which is the shape
    /// C-003/C-073's continuity rows stay green on.
    pub async fn local_unadvertised() -> Self {
        Self::assemble(
            Kind::Local,
            TempDir::new().expect("temporary matrix home"),
            None,
            false,
        )
        .await
    }

    /// A local run whose workspace already holds a real Character and its
    /// binding, created through the public Core actor surface before the
    /// provider opens (the same workspace state database the Core open used).
    pub async fn local_character() -> (Self, CharacterFixture) {
        let home = TempDir::new().expect("temporary matrix home");
        let (fixture, creator_id) = create_character_fixture(home.path()).await;
        let matrix = Self::assemble(Kind::Local, home, Some(creator_id), true).await;
        (matrix, fixture)
    }

    /// A fixture run exercising the same real Character/binding protocol
    /// identities. The independent provider has no Nexus actor surface, so the
    /// Character is created through the public Core actor path once and the
    /// identical subject/binding ids are driven over the fixture.
    pub async fn remote_character() -> (Self, CharacterFixture) {
        let scratch = TempDir::new().expect("character identity home");
        let (fixture, _creator_id) = create_character_fixture(scratch.path()).await;
        let matrix = Self::start(Kind::Remote).await;
        (matrix, fixture)
    }

    /// Assemble a run over a prepared home.
    async fn assemble(
        kind: Kind,
        home_dir: TempDir,
        creator_id: Option<String>,
        reflections: bool,
    ) -> Self {
        let host_id = match kind {
            Kind::Local => "c-matrix-local",
            Kind::Remote => "c-matrix-remote",
        }
        .to_owned();
        let (home, provider, space_id, directory, admin_principal, peer_principal) = match kind {
            Kind::Local => open_local_workspace(home_dir, creator_id, reflections).await,
            Kind::Remote => open_remote_workspace(home_dir).await,
        };
        let service = build_service(&provider, &directory);
        // One owner at a time: if the admin endpoint fails, the provider is
        // settled; if the peer fails, the admin endpoint AND the provider are
        // settled, with both errors preserved on the re-raised failure.
        let admin = match std::panic::AssertUnwindSafe(open_endpoint(
            Arc::clone(&service),
            &host_id,
            ADMIN_SEED,
        ))
        .catch_unwind()
        .await
        {
            Ok(endpoint) => endpoint,
            Err(payload) => {
                // The local service holds a provider reference; release it so
                // the fixture's own bounded close can actually take ownership.
                drop(service);
                match settle_provider_setup(provider).await {
                    Ok(()) => std::panic::resume_unwind(payload),
                    Err(problem) => {
                        let original = panic_message(&*payload);
                        panic!(
                            "the admin endpoint setup failed ({original}) and the provider settlement also failed: {problem}"
                        );
                    }
                }
            }
        };
        let peer = match std::panic::AssertUnwindSafe(open_endpoint(
            Arc::clone(&service),
            &host_id,
            PEER_SEED,
        ))
        .catch_unwind()
        .await
        {
            Ok(endpoint) => endpoint,
            Err(payload) => {
                let mut problems: Vec<String> = Vec::new();
                if let Err(error) = admin.settle().await {
                    problems.push(format!("the admin session did not settle: {error}"));
                }
                // After the endpoint owners are closed, the local service is the
                // last provider holder this path owns; drop it before the
                // fixture's try_unwrap close.
                drop(service);
                if let Err(error) = settle_provider_setup(provider).await {
                    problems.push(format!("the provider did not settle: {error}"));
                }
                if problems.is_empty() {
                    std::panic::resume_unwind(payload);
                }
                let original = panic_message(&*payload);
                panic!(
                    "the peer endpoint setup failed ({original}) and cleanup also failed: {}",
                    problems.join("; ")
                );
            }
        };
        Self {
            kind,
            home,
            provider,
            directory,
            service,
            admin,
            peer,
            admin_principal,
            peer_principal,
            space_id,
            host_id,
            advanced: AtomicI64::new(0),
        }
    }

    /// A fresh run over the real Nexus local provider.
    pub async fn local() -> Self {
        Self::start(Kind::Local).await
    }

    /// A fresh run over the independent durable fixture provider.
    pub async fn remote() -> Self {
        Self::start(Kind::Remote).await
    }

    pub const fn kind(&self) -> Kind {
        self.kind
    }

    pub fn space_id(&self) -> &str {
        &self.space_id
    }

    pub const fn admin_principal(&self) -> &Principal {
        &self.admin_principal
    }

    pub const fn peer_principal(&self) -> &Principal {
        &self.peer_principal
    }

    /// The operator/admin session.
    pub fn admin(&self) -> Actor<'_> {
        Actor {
            client: &self.admin.client,
            principal: &self.admin_principal,
            space_id: &self.space_id,
        }
    }

    /// The admin session's client handle, for a task that outlives the borrow.
    pub fn admin_client(&self) -> Arc<RemoteAdapter> {
        Arc::clone(&self.admin.client)
    }

    /// The second peer's own signed client handle: a distinct authenticated
    /// session, not the admin connection with a different principal argument.
    pub fn peer_client(&self) -> Arc<RemoteAdapter> {
        Arc::clone(&self.peer.client)
    }

    /// The grant-only second client session.
    pub fn peer(&self) -> Actor<'_> {
        Actor {
            client: &self.peer.client,
            principal: &self.peer_principal,
            space_id: &self.space_id,
        }
    }

    /// Restart the provider over the SAME file: close both responder sessions,
    /// close the owned pool, reopen the file as a fresh instance and serve two
    /// fresh sessions. The previous instance is never reused.
    pub async fn restart(self) -> Self {
        let Self {
            kind,
            home,
            provider,
            directory,
            service,
            admin,
            peer,
            admin_principal,
            peer_principal,
            space_id,
            host_id,
            ..
        } = self;
        admin.close().await;
        peer.close().await;
        drop(service);
        match provider {
            Provider::Local(provider) => drop(provider),
            Provider::Fixture(provider) => close_fixture(provider).await,
        }
        let provider = open_provider(&home).await;
        let service = build_service(&provider, &directory);
        // The reopened provider is settled if either fresh session fails to
        // set up; the admin endpoint is settled before the provider.
        let admin = match std::panic::AssertUnwindSafe(open_endpoint(
            Arc::clone(&service),
            &host_id,
            ADMIN_SEED,
        ))
        .catch_unwind()
        .await
        {
            Ok(endpoint) => endpoint,
            Err(payload) => {
                // Release this path's local provider reference before the
                // fixture's bounded close tries to take sole ownership.
                drop(service);
                match settle_provider_setup(provider).await {
                    Ok(()) => std::panic::resume_unwind(payload),
                    Err(problem) => {
                        let original = panic_message(&*payload);
                        panic!(
                            "the restarted admin endpoint setup failed ({original}) and the provider settlement also failed: {problem}"
                        );
                    }
                }
            }
        };
        let peer = match std::panic::AssertUnwindSafe(open_endpoint(
            Arc::clone(&service),
            &host_id,
            PEER_SEED,
        ))
        .catch_unwind()
        .await
        {
            Ok(endpoint) => endpoint,
            Err(payload) => {
                let mut problems: Vec<String> = Vec::new();
                if let Err(error) = admin.settle().await {
                    problems.push(format!("the admin session did not settle: {error}"));
                }
                drop(service);
                if let Err(error) = settle_provider_setup(provider).await {
                    problems.push(format!("the provider did not settle: {error}"));
                }
                if problems.is_empty() {
                    std::panic::resume_unwind(payload);
                }
                let original = panic_message(&*payload);
                panic!(
                    "the restarted peer endpoint setup failed ({original}) and cleanup also failed: {}",
                    problems.join("; ")
                );
            }
        };
        Self {
            kind,
            home,
            provider,
            directory,
            service,
            admin,
            peer,
            admin_principal,
            peer_principal,
            space_id,
            host_id,
            advanced: AtomicI64::new(0),
        }
    }

    /// Reconnect only the second peer's signed session over the same provider,
    /// keeping its Connect identity (and therefore its admitted principal).
    pub async fn reconnect_peer(&mut self) {
        let endpoint = std::mem::replace(
            &mut self.peer,
            Endpoint {
                client: self.admin.client.clone(),
                responder: Arc::clone(&self.admin.responder),
                socket: Arc::clone(&self.admin.socket),
            },
        );
        endpoint.close().await;
        self.peer = open_endpoint(Arc::clone(&self.service), &self.host_id, PEER_SEED).await;
    }

    /// Park the exact mutating handler named by `op` at its admission point
    /// (fixture only: the local leg exposes the provider admission seam instead).
    pub fn park_next_target(&self, op: &str) -> CaptureHold {
        match &self.provider {
            Provider::Local(_) => panic!("use the provider admission seam on the local leg"),
            Provider::Fixture(provider) => {
                CaptureHold::FixtureTarget(provider.faults().hold_next_for_bounded(op, BOUND))
            }
        }
    }

    /// [`Self::park_next_target`] with an explicit fixture-side bound, so an
    /// expiry case can let the provider's own wait end before the client's
    /// invoke bound and still observe the operation's ordinary refusal.
    pub fn park_next_target_bounded(&self, op: &str, bound: Duration) -> CaptureHold {
        match &self.provider {
            Provider::Local(_) => panic!("use the provider admission seam on the local leg"),
            Provider::Fixture(provider) => {
                CaptureHold::FixtureTarget(provider.faults().hold_next_for_bounded(op, bound))
            }
        }
    }

    /// Install the local provider's disposition-wait report: it notifies when a
    /// candidate-consuming request is about to block on the owning subject's
    /// disposition section (a real provider-side arrival acknowledgment).
    pub fn report_disposition_waits(&self, waiter: Arc<Notify>) {
        match &self.provider {
            Provider::Local(provider) => provider.report_disposition_waits(waiter),
            Provider::Fixture(_) => panic!("the fixture has no disposition section"),
        }
    }

    /// A raw signed session for wire-level rows (`op_unsupported`, typed-decode
    /// refusals) that the typed client cannot express.
    pub async fn raw_admin(&self) -> RawSession {
        RawSession::open(
            Arc::clone(&self.service),
            &self.host_id,
            ADMIN_SEED,
            &MEMORY_CAPABILITIES,
        )
        .await
    }

    /// A raw signed session whose client hello does not advertise
    /// `spoke-memory`, so the Connect dispatch gate answers memory ops with
    /// `capability_missing` before any handler runs (protocol §13).
    pub async fn raw_admin_without_memory_capability(&self) -> RawSession {
        RawSession::open(Arc::clone(&self.service), &self.host_id, ADMIN_SEED, &[]).await
    }

    /// The operator controls of this run's Local provider, or `None` on the
    /// fixture leg (which arms its own faults through [`Matrix::park_next_*`]).
    pub fn local_operator(&self) -> Option<LocalOperator> {
        match &self.provider {
            Provider::Local(provider) => Some(LocalOperator {
                provider: Arc::clone(provider),
                file_root: provider.file_root(),
            }),
            Provider::Fixture(_) => None,
        }
    }

    /// Release every session and the provider pool.
    pub async fn shutdown(self) {
        let Self {
            provider,
            service,
            admin,
            peer,
            ..
        } = self;
        admin.close().await;
        peer.close().await;
        drop(service);
        match provider {
            Provider::Local(provider) => drop(provider),
            Provider::Fixture(provider) => close_fixture(provider).await,
        }
    }

    /// [`Self::shutdown`] for an aggregating caller: every owned close is
    /// attempted and its failure returned instead of panicked, so one failure
    /// can never prevent the remaining owners from being settled.
    pub async fn settle(self) -> Result<(), String> {
        let Self {
            provider,
            service,
            admin,
            peer,
            ..
        } = self;
        let mut failures: Vec<String> = Vec::new();
        if let Err(error) = admin.settle().await {
            failures.push(error);
        }
        if let Err(error) = peer.settle().await {
            failures.push(error);
        }
        drop(service);
        match provider {
            Provider::Local(provider) => drop(provider),
            Provider::Fixture(provider) => {
                if let Err(error) = close_fixture_bounded(provider).await {
                    failures.push(error);
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

/// Close the fixture's own pool once every session that held it has ended.
async fn close_fixture(provider: Arc<FixtureMemoryProvider>) {
    let deadline = Instant::now() + BOUND;
    let mut provider = provider;
    loop {
        tokio::task::yield_now().await;
        match Arc::try_unwrap(provider) {
            Ok(fixture) => {
                fixture.close().await;
                return;
            }
            Err(shared) => {
                assert!(
                    Instant::now() < deadline,
                    "the fixture provider is still shared after the session teardown"
                );
                provider = shared;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

/// [`close_fixture`] for an aggregating caller: the ownership wait and the
/// provider's own close are both bounded and returned, never panicked.
async fn close_fixture_bounded(provider: Arc<FixtureMemoryProvider>) -> Result<(), String> {
    let deadline = Instant::now() + BOUND;
    let mut provider = provider;
    loop {
        tokio::task::yield_now().await;
        match Arc::try_unwrap(provider) {
            Ok(fixture) => {
                return match tokio::time::timeout(BOUND, fixture.close()).await {
                    Ok(()) => Ok(()),
                    Err(_) => {
                        Err("the fixture provider close did not finish within the bound".to_owned())
                    }
                };
            }
            Err(shared) => {
                if Instant::now() >= deadline {
                    return Err(
                        "the fixture provider is still shared after the session teardown"
                            .to_owned(),
                    );
                }
                provider = shared;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

/// [`CoreService::close`] for an aggregating caller: the bound and the inner
/// result are both reported as a failure string.
pub async fn close_core_bounded(
    core: &CoreService,
) -> Result<nexus_contracts::CoreCloseReport, String> {
    match tokio::time::timeout(BOUND, core.close()).await {
        Ok(Ok(report)) => {
            if report.cleanup_confirmed {
                Ok(report)
            } else {
                Err(format!(
                    "the CoreService close did not confirm cleanup (state {}, pending {:?}, reason {:?})",
                    report.state, report.pending_operations, report.reason
                ))
            }
        }
        Ok(Err(error)) => Err(format!("the CoreService close failed: {error}")),
        Err(_) => Err("the CoreService close did not finish within the bound".to_owned()),
    }
}

/// Owned operation tasks for one scenario, so a deadline or an earlier failure
/// can never detach live work: every handle stays owned until the scenario
/// joins it or [`TaskSet::settle`] aborts and bounded-joins it.
///
/// Each task lives in its own async slot owned by the set. A join awaits that
/// slot's handle **in place**, so cancelling the join future (an assertion, a
/// test timeout, the runner) leaves the same handle owned for the later
/// settlement instead of dropping it detach-by-value. A deadline aborts and
/// bounded-joins the same handle; an unconfirmed settlement is reported and the
/// handle stays owned, never claimed as joined.
#[derive(Clone, Default)]
pub struct TaskSet {
    slots: Arc<std::sync::Mutex<Vec<TaskSlot>>>,
}

type TaskSlot = Arc<tokio::sync::Mutex<Option<AbortOnDropHandle>>>;

/// A scenario task handle whose drop requests abort. When the last owner of a
/// slot (including a cancelled `run_matrix_scenario` future and every `TaskSet`
/// clone) is dropped, the task is aborted instead of being detach-by-drop; an
/// explicit bounded join still observes and clears it.
struct AbortOnDropHandle(Option<tokio::task::JoinHandle<Value>>);

impl AbortOnDropHandle {
    const fn new(handle: tokio::task::JoinHandle<Value>) -> Self {
        Self(Some(handle))
    }

    const fn handle(&mut self) -> Option<&mut tokio::task::JoinHandle<Value>> {
        self.0.as_mut()
    }

    fn clear(&mut self) {
        self.0 = None;
    }
}

impl Drop for AbortOnDropHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

impl TaskSet {
    /// Spawn one owned operation future and return its slot.
    pub fn spawn<F>(&self, future: F) -> usize
    where
        F: Future<Output = Value> + Send + 'static,
    {
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slots.push(Arc::new(tokio::sync::Mutex::new(Some(
            AbortOnDropHandle::new(tokio::spawn(future)),
        ))));
        slots.len() - 1
    }

    fn slot(&self, index: usize, what: &str) -> Result<TaskSlot, String> {
        self.slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(index)
            .cloned()
            .ok_or_else(|| format!("{what} has no owned task"))
    }

    /// Join one owned handle under the harness bound, keeping it in its slot
    /// for the whole await. A deadline aborts and bounded-joins the same
    /// handle; if that settlement is unconfirmed the handle stays owned for
    /// the runner's final `settle`, and the error says so.
    pub async fn join(&self, index: usize, what: &str) -> Result<Value, String> {
        let slot = self.slot(index, what)?;
        let mut guard = slot.lock().await;
        let owner = guard
            .as_mut()
            .ok_or_else(|| format!("{what} has no owned task"))?;
        let handle = owner
            .handle()
            .ok_or_else(|| format!("{what} has no owned task"))?;
        let joined = match tokio::time::timeout(BOUND, &mut *handle).await {
            Ok(Ok(value)) => {
                owner.clear();
                Ok(value)
            }
            Ok(Err(error)) => {
                owner.clear();
                Err(format!("{what} task failed: {error}"))
            }
            Err(_) => {
                handle.abort();
                match tokio::time::timeout(BOUND, &mut *handle).await {
                    Ok(Ok(_)) => {
                        owner.clear();
                        Err(format!(
                            "{what} exceeded the harness bound; the owned task was aborted and joined"
                        ))
                    }
                    Ok(Err(error)) if error.is_cancelled() => {
                        owner.clear();
                        Err(format!(
                            "{what} exceeded the harness bound; the owned task was aborted and joined"
                        ))
                    }
                    Ok(Err(error)) => {
                        owner.clear();
                        Err(format!(
                            "{what} exceeded the harness bound and its task failed while being aborted: {error}"
                        ))
                    }
                    Err(_) => Err(format!(
                        "{what} exceeded the harness bound and did not settle after abort; settlement unconfirmed and the handle is still owned"
                    )),
                }
            }
        };
        drop(guard);
        joined
    }

    /// Abort and bounded-join every still-owned task, attempting all of them.
    pub async fn settle(&self, what: &str) -> Result<(), String> {
        let slots: Vec<TaskSlot> = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        let mut failures: Vec<String> = Vec::new();
        for (index, slot) in slots.iter().enumerate() {
            let mut guard = slot.lock().await;
            let Some(owner) = guard.as_mut() else {
                continue;
            };
            let Some(handle) = owner.handle() else {
                continue;
            };
            handle.abort();
            match tokio::time::timeout(BOUND, &mut *handle).await {
                Ok(Ok(_)) => {
                    owner.clear();
                }
                Ok(Err(error)) if error.is_cancelled() => {
                    owner.clear();
                }
                Ok(Err(error)) => {
                    owner.clear();
                    failures.push(format!(
                        "{what}: a task ({index}) failed during settlement: {error}"
                    ));
                }
                Err(_) => failures.push(format!(
                    "{what}: a task ({index}) did not settle after abort within the bound; settlement unconfirmed"
                )),
            }
            drop(guard);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

/// Raw sessions and durable observers owned by the runner for the whole
/// scenario, so a failing assertion can never drop them unclosed.
#[derive(Default)]
pub struct OwnedResources {
    raw: Vec<Option<RawSession>>,
    observers: Vec<Option<DurableObserver>>,
}

impl OwnedResources {
    pub async fn open_raw_admin(&mut self, matrix: &Matrix) -> usize {
        self.push_raw(matrix.raw_admin().await)
    }

    pub async fn open_raw_without_memory(&mut self, matrix: &Matrix) -> usize {
        self.push_raw(matrix.raw_admin_without_memory_capability().await)
    }

    fn push_raw(&mut self, session: RawSession) -> usize {
        self.raw.push(Some(session));
        self.raw.len() - 1
    }

    pub async fn invoke(&mut self, index: usize, op: &str, payload: &Value) -> Value {
        self.raw
            .get_mut(index)
            .and_then(Option::as_mut)
            .expect("the raw session is owned by the runner")
            .invoke(op, payload)
            .await
    }

    pub async fn close_raw(&mut self, index: usize) -> Result<(), String> {
        match self.raw.get_mut(index).and_then(Option::take) {
            Some(session) => session.close().await,
            None => Ok(()),
        }
    }

    pub async fn open_observer(
        &mut self,
        matrix: &Matrix,
        subject_id: &str,
        record_ids: &[String],
        candidate_id: Option<&str>,
        receipt_id: Option<&str>,
        profile_kind: Option<&str>,
    ) -> usize {
        self.observers.push(Some(
            matrix
                .durable_observer(
                    subject_id,
                    record_ids,
                    candidate_id,
                    receipt_id,
                    profile_kind,
                )
                .await,
        ));
        self.observers.len() - 1
    }

    pub async fn observe(&mut self, index: usize) -> DurableState {
        self.observers
            .get_mut(index)
            .and_then(Option::as_mut)
            .expect("the durable observer is owned by the runner")
            .observe()
            .await
    }

    pub async fn receipt_ids_for(&mut self, index: usize, op: &str) -> Vec<String> {
        self.observers
            .get_mut(index)
            .and_then(Option::as_mut)
            .expect("the durable observer is owned by the runner")
            .receipt_ids_for(op)
            .await
    }

    pub async fn journal_count(&mut self, index: usize) -> Option<i64> {
        self.observers
            .get_mut(index)
            .and_then(Option::as_mut)
            .expect("the durable observer is owned by the runner")
            .journal_count()
            .await
    }

    pub async fn close_observer(&mut self, index: usize) -> Result<(), String> {
        match self.observers.get_mut(index).and_then(Option::take) {
            Some(observer) => {
                observer.close().await;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Attempt every remaining close; one failure never prevents the rest.
    pub async fn settle(&mut self) -> Result<(), String> {
        let mut failures: Vec<String> = Vec::new();
        for slot in &mut self.raw {
            if let Some(session) = slot.take() {
                if let Err(error) = session.close().await {
                    failures.push(format!("raw session close: {error}"));
                }
            }
        }
        for slot in &mut self.observers {
            if let Some(observer) = slot.take() {
                let outcome = std::panic::AssertUnwindSafe(async move { observer.close().await })
                    .catch_unwind()
                    .await;
                if let Err(payload) = outcome {
                    failures.push(format!(
                        "durable observer close: {}",
                        panic_message(&*payload)
                    ));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload.downcast_ref::<&str>().map_or_else(
        || {
            payload
                .downcast_ref::<String>()
                .map_or_else(|| "non-string panic payload".to_owned(), Clone::clone)
        },
        |text| (*text).to_owned(),
    )
}

/// Every string value of length >= 4 anywhere in `value`.
fn collect_receipt_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if text.len() >= 4 {
                out.push(text.clone());
            }
        }
        Value::Object(map) => {
            for child in map.values() {
                collect_receipt_strings(child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_receipt_strings(item, out);
            }
        }
        _ => {}
    }
}

/// The sensitive result identifiers (keys ending `_id`/`_ids`) of a receipt.
fn collect_result_identifiers(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key.ends_with("_id") || key.ends_with("_ids") {
                    collect_receipt_strings(child, out);
                } else if child.is_object() || child.is_array() {
                    collect_result_identifiers(child, out);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_result_identifiers(item, out);
            }
        }
        _ => {}
    }
}

/// The refusal never renders a sensitive authorized-receipt value that this
/// request did not itself supply.
fn assert_no_sensitive_receipt_values(
    response: &Value,
    authorized_receipt: &Value,
    allowed: &[String],
) {
    let mut sensitive: Vec<String> = Vec::new();
    for path in [
        "payload_digest",
        "created_at",
        "completed_at",
        "idempotency_key",
    ] {
        if let Some(value) = authorized_receipt.get(path).and_then(Value::as_str) {
            if value.len() >= 4 {
                sensitive.push(value.to_owned());
            }
        }
    }
    // The result is op-specific: its identifiers are sensitive values even under
    // another key, while its booleans/enums are not banned as such.
    if let Some(result) = authorized_receipt.get("result") {
        collect_result_identifiers(result, &mut sensitive);
    }

    let rendered = response.to_string();
    for value in sensitive {
        if allowed.iter().any(|permitted| permitted == &value) {
            continue;
        }
        assert!(
            !rendered.contains(&value),
            "the refusal discloses the authorized receipt value {value}: {response}"
        );
    }
}

/// The one receipt-privacy oracle shared by the grant and error targets.
///
/// Compares an actual authorized receipt body against the ENTIRE refusal. It
/// rejects: any object carrying two or more of the authorized receipt's actual
/// protected values (a receipt body or nested tuple, with or without
/// `receipt_id`), and any of the receipt's sensitive identifying values
/// (payload digest, timestamps, old idempotency key, result `*_id` values)
/// anywhere in the refusal.
///
/// `current_request` is what the caller itself supplied for THIS request (op,
/// space, requested ids, correlation ids). Those values are legitimate echoes
/// and are allowed; so are unrelated non-sensitive diagnostics such as a
/// `health: true` boolean or an object carrying an unrelated `op`/`result`.
/// Recognition is by actual receipt values, not generic field names.
pub fn assert_no_receipt_disclosure(
    response: &Value,
    authorized_receipt: &Value,
    current_request: &Value,
) {
    fn is_allowed(value: &Value, allowed: &[String]) -> bool {
        matches!(value, Value::String(text) if allowed.iter().any(|permitted| permitted == text))
    }

    /// One field carries the same value as the authorized receipt's field of
    /// that name AND the caller did not supply that value in this request.
    /// Field names alone are never evidence: a current op/space or a
    /// caller-supplied receipt id may echo freely.
    fn matches_receipt(key: &str, value: &Value, receipt: &Value, allowed: &[String]) -> bool {
        let Some(expected) = receipt.get(key) else {
            return false;
        };
        if expected.is_null() || value != expected {
            return false;
        }
        !is_allowed(value, allowed)
    }

    fn walk(value: &Value, meaning: &str, receipt: &Value, allowed: &[String]) {
        match value {
            Value::Object(map) => {
                // A protected tuple/body is recognized by its relationship to
                // the ACTUAL authorized receipt (and what this request already
                // supplied), never by a generic key count. A bare echo of the
                // caller's requested receipt id is legal, but that same id
                // bound to any actual protected receipt field is a disclosure
                // association, not mere echo.
                let authorized_id = receipt.get("receipt_id").and_then(Value::as_str);
                let carries_authorized_id = authorized_id
                    .is_some_and(|id| map.values().any(|field| field.as_str() == Some(id)));
                let protected = map
                    .iter()
                    .filter(|(key, field)| matches_receipt(key, field, receipt, allowed))
                    .count();
                assert!(
                    protected < 2 && !(carries_authorized_id && protected >= 1),
                    "a refusal embeds protected authorized-receipt values in {meaning}: {value}"
                );
                for (key, child) in map {
                    walk(child, &format!("{meaning}.{key}"), receipt, allowed);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &format!("{meaning}[{index}]"), receipt, allowed);
                }
            }
            _ => {}
        }
    }

    let mut allowed: Vec<String> = Vec::new();
    collect_receipt_strings(current_request, &mut allowed);
    walk(response, "$response", authorized_receipt, &allowed);
    assert_no_sensitive_receipt_values(response, authorized_receipt, &allowed);
}

/// Run one scenario body over `matrix` with runner-owned tasks/resources, then
/// settle every owner on BOTH success and failure.
///
/// Tasks are aborted and bounded-joined, raw sessions and observers are closed,
/// and the matrix sessions/provider are settled; every cleanup failure is
/// reported independently and a scenario failure is re-raised afterwards.
pub async fn run_matrix_scenario<F>(matrix: Matrix, body: F)
where
    F: for<'a> FnOnce(
            &'a Matrix,
            &'a mut OwnedResources,
            TaskSet,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send,
{
    let mut resources = OwnedResources::default();
    let tasks = TaskSet::default();
    let scenario = std::panic::AssertUnwindSafe(body(&matrix, &mut resources, tasks.clone()))
        .catch_unwind()
        .await;
    let mut cleanup: Vec<String> = Vec::new();
    if let Err(error) = tasks.settle("scenario tasks").await {
        cleanup.push(error);
    }
    if let Err(error) = resources.settle().await {
        cleanup.push(error);
    }
    if let Err(error) = matrix.settle().await {
        cleanup.push(error);
    }
    match (scenario, cleanup.is_empty()) {
        (Ok(()), true) => {}
        (Ok(()), false) => panic!(
            "cleanup failed after a passing scenario: {}",
            cleanup.join("; ")
        ),
        (Err(payload), true) => std::panic::resume_unwind(payload),
        (Err(payload), false) => {
            eprintln!("cleanup also failed: {}", cleanup.join("; "));
            std::panic::resume_unwind(payload);
        }
    }
}

/// Real early-failure control over the ACTUAL endpoint setup helper: a wrong
/// host key makes the primitive's genuine signed dial fail after its real
/// listener accepted and started a responder. The primitive itself settles the
/// same listener task (abort, bounded join, close returned owners, port release),
/// and this control then proves the real fixture provider it served is still
/// closeable. No lookalike listener/abort code is introduced.
pub async fn prove_failed_endpoint_settles() {
    let home = TempDir::new().expect("the setup-control fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&home.path().join("control.sqlite"))
            .await
            .expect("open the setup-control fixture"),
    );
    let directory = Arc::new(PeerDirectory::new());
    let service = build_service(&Provider::Fixture(Arc::clone(&provider)), &directory);
    let Err(failure) = open_endpoint_expecting(
        Arc::clone(&service),
        "c-matrix-setup-control",
        ADMIN_SEED,
        // A wrong host key: a deterministic genuine dial failure after the
        // real listener accepted and its responder started.
        pubkey(&PEER_SEED),
    )
    .await
    else {
        panic!("the wrong host key must fail the real signed dial");
    };
    assert!(
        failure.is_signed_dial_rejection(),
        "the setup failed in the SDK's signed responder-hello verification after receiving the real hello, not another step: {failure:?}"
    );
    let EndpointSetupFailure::Dial {
        attempt,
        server_socket,
    } = &failure;
    let inbound = attempt
        .first_inbound
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .expect("the typed dial received the responder envelope before verification");
    let hello: ConnectHello = serde_json::from_slice(&inbound)
        .expect("the observed inbound envelope decodes as a ConnectHello");
    let host_pubkey = pubkey(&HOST_SEED);
    let host_peer = peer_id(&HOST_SEED);
    assert_eq!(
        hello.peer_id.as_str(),
        host_peer,
        "the observed envelope is the real host's responder hello"
    );
    verify_hello_ed25519(&host_pubkey, &host_peer, &hello, None).expect(
        "the observed envelope carries a valid signature and identity for the real host key",
    );
    let wrong_pubkey = pubkey(&PEER_SEED);
    let wrong_peer = derive_peer_id_from_ed25519_pubkey(&wrong_pubkey);
    let rejected = verify_hello_ed25519(&wrong_pubkey, &wrong_peer, &hello, None);
    assert!(
        matches!(rejected, Err(CoreError::HandshakeFailed { .. })),
        "the supplied wrong expected key is rejected by hello verification of the decodable envelope: {rejected:?}"
    );
    // The listener settlement above already bounded-awaited responder and
    // server-transport close and the port release. This poll proves the test's
    // observed client transport object and the listener-side socket object have
    // no remaining owners; it does not by itself claim that the SDK's internal
    // detached teardown task has completed.
    tokio::time::timeout(BOUND, async {
        while attempt.client_socket.upgrade().is_some() || server_socket.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("no owner of the observed client transport or the listener socket remains");
    drop(service);
    close_fixture_bounded(provider)
        .await
        .expect("the failed setup leaves the real fixture provider closeable");
}

/// Real raw-setup failure control over the ACTUAL `RawSession::open` primitive:
/// a wrong expected host key fails the raw client's verification of the signed
/// responder hello after the real listener accepted and started its responder.
/// The raw failure path reuses the same bounded listener settlement (completed
/// responder and socket both closed, port released), and the fixture provider it
/// served is still closeable afterwards.
pub async fn prove_raw_setup_failure_settles() {
    let home = TempDir::new().expect("the raw setup-control fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&home.path().join("raw-control.sqlite"))
            .await
            .expect("open the raw setup-control fixture"),
    );
    let directory = Arc::new(PeerDirectory::new());
    let service = build_service(&Provider::Fixture(Arc::clone(&provider)), &directory);
    let Err(failure) = RawSession::open_expecting(
        Arc::clone(&service),
        "c-matrix-raw-setup-control",
        ADMIN_SEED,
        &MEMORY_CAPABILITIES,
        // A wrong host key: a deterministic genuine signed-hello failure.
        pubkey(&PEER_SEED),
    )
    .await
    else {
        panic!("the wrong host key must fail the raw signed hello");
    };
    assert!(
        failure.is_signed_hello_rejection(),
        "the setup failed in the SDK's signed responder-hello verification, not another step: {failure:?}"
    );
    drop(service);
    close_fixture_bounded(provider)
        .await
        .expect("the failed raw setup leaves the real fixture provider closeable");
}

// ---------------------------------------------------------------------------
// Operator seams (test control only, one real seam per provider)
// ---------------------------------------------------------------------------

/// The receipt/dedup windows the provider itself advertises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiptWindows {
    pub receipt: i64,
    pub dedup: i64,
}

/// The advertised receipt/dedup retention windows.
pub fn receipt_windows(described: &Value) -> ReceiptWindows {
    let limit = |name: &str| -> i64 {
        described["limits"][name].as_u64().map_or_else(
            || panic!("describe advertises {name}: {described}"),
            |value| i64::try_from(value).unwrap_or(i64::MAX),
        )
    };
    ReceiptWindows {
        receipt: limit("receipt_retention_seconds"),
        dedup: limit("dedup_retention_seconds"),
    }
}

/// A capture parked at its provider's real capture point.
pub enum CaptureHold {
    /// Local: parked at a provider seam. The token lets the handle disarm only
    /// its own arm; the provider's wait is bounded, so a dropped handle cannot
    /// park a write forever.
    Local {
        provider: Arc<LocalMemoryProvider>,
        token: u64,
        entered: Arc<Notify>,
        release: Arc<Barrier>,
    },
    /// Fixture: the bounded `BeforeCommit` hold, taken with the effect, receipt,
    /// dedup row and target staged but not committed.
    FixturePoint(FaultHold),
    /// Fixture: the bounded op-targeted admission arm, taken at the exact
    /// mutating handler entry named by the op.
    FixtureTarget(TargetHold),
}

/// Disarms this handle's own untaken local arm when its observation ends without
/// a take (timeout, cancellation, unpolled drop).
struct LocalReachGuard {
    provider: Arc<LocalMemoryProvider>,
    token: u64,
}

impl Drop for LocalReachGuard {
    fn drop(&mut self) {
        self.provider.disarm_next_admission(self.token);
    }
}

/// How long the harness waits for a peer at a release barrier that the provider
/// may already have left (its own wait is bounded).
const RELEASE_BOUND: Duration = Duration::from_secs(1);

impl Drop for CaptureHold {
    fn drop(&mut self) {
        if let Self::Local {
            provider, token, ..
        } = self
        {
            provider.disarm_next_admission(*token);
        }
    }
}

impl CaptureHold {
    /// Wait, bounded, until the parked capture reached its point.
    pub fn wait_reached(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), FaultHoldLost>> + Send + '_>> {
        self.wait_reached_within(BOUND)
    }

    /// [`Self::wait_reached`] with an explicit observation window. Ownership of
    /// an untaken local arm is established at the call, so a timeout, cancel or
    /// dropped-unpolled future disarms this handle's own token even when the
    /// handle is retained.
    pub fn wait_reached_within(
        &mut self,
        window: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), FaultHoldLost>> + Send + '_>> {
        match self {
            Self::Local {
                provider,
                token,
                entered,
                ..
            } => {
                let guard = LocalReachGuard {
                    provider: Arc::clone(provider),
                    token: *token,
                };
                let entered = Arc::clone(entered);
                Box::pin(async move {
                    let _guard = guard;
                    match tokio::time::timeout(window, entered.notified()).await {
                        Ok(()) => Ok(()),
                        Err(_) => Err(FaultHoldLost::Unreached),
                    }
                })
            }
            Self::FixturePoint(hold) => {
                let future = hold.wait_reached();
                Box::pin(future)
            }
            Self::FixtureTarget(hold) => {
                let future = hold.wait_reached();
                Box::pin(future)
            }
        }
    }

    /// Release the parked capture so it proceeds.
    pub async fn release(&mut self) {
        match self {
            Self::Local { release, .. } => {
                // Bounded: the provider's own wait can expire first, leaving no
                // peer; a later release must not wait forever.
                let _ = tokio::time::timeout(RELEASE_BOUND, release.wait()).await;
            }
            Self::FixturePoint(hold) => hold.release(),
            Self::FixtureTarget(hold) => hold.release(),
        }
    }
}

impl Matrix {
    /// Arm the provider's after-COMMIT response loss: the effect commits and the
    /// caller learns `OUTCOME_UNCERTAIN`.
    pub fn arm_lost_response(&self) {
        match &self.provider {
            Provider::Local(provider) => provider.inject_commit_fault(true),
            Provider::Fixture(provider) => {
                provider
                    .faults()
                    .fail_next(FaultPoint::AfterCommitBeforeResponse);
            }
        }
    }

    /// Arm the provider's before-COMMIT commit-boundary failure: no effect is
    /// committed and the outcome is still uncertain.
    pub fn arm_commit_boundary_failure(&self) {
        match &self.provider {
            Provider::Local(provider) => provider.inject_commit_fault(false),
            Provider::Fixture(provider) => {
                provider.faults().fail_next(FaultPoint::BeforeCommit);
            }
        }
    }

    /// Park the next capture at the provider's own capture point.
    pub fn park_next_capture(&self) -> CaptureHold {
        match &self.provider {
            Provider::Local(provider) => {
                let entered = Arc::new(Notify::new());
                let release = Arc::new(Barrier::new(2));
                provider.park_next_capture(Arc::clone(&entered), Arc::clone(&release));
                CaptureHold::Local {
                    provider: Arc::clone(provider),
                    token: 0,
                    entered,
                    release,
                }
            }
            Provider::Fixture(provider) => {
                CaptureHold::FixturePoint(provider.faults().hold_next(FaultPoint::BeforeCommit))
            }
        }
    }

    /// Park the next write once it has entered the provider's write-admission
    /// machinery and before its writer transaction. Local uses its own admission
    /// park seam; the fixture uses its existing bounded `AdmissionPause` hold.
    /// Either way `wait_reached` is the provider-side arrival acknowledgment.
    pub fn park_next_admission(&self) -> CaptureHold {
        match &self.provider {
            Provider::Local(provider) => {
                let entered = Arc::new(Notify::new());
                let release = Arc::new(Barrier::new(2));
                let token =
                    provider.park_next_admission(Arc::clone(&entered), Arc::clone(&release));
                CaptureHold::Local {
                    provider: Arc::clone(provider),
                    token,
                    entered,
                    release,
                }
            }
            Provider::Fixture(provider) => CaptureHold::FixturePoint(
                provider
                    .faults()
                    .hold_next_bounded(FaultPoint::AdmissionPause, BOUND),
            ),
        }
    }

    /// The local provider's own write-admission counter: how many write entries
    /// reached its admission machinery. `None` on the fixture, which exposes no
    /// equivalent counter.
    pub fn write_admission_count(&self) -> Option<usize> {
        match &self.provider {
            Provider::Local(provider) => Some(provider.lock_attempts()),
            Provider::Fixture(_) => None,
        }
    }

    /// Move time past the receipt body window, or past the dedup tombstone window
    /// that follows it, using each provider's own operator maintenance.
    ///
    /// Local: its documented test clock seam; the reclamation runs in the next
    /// mutating admission, and reads already judge the window logically.
    /// Fixture: its public retention pass at the same absolute instant.
    pub async fn expire_receipts(&self, past_dedup: bool, windows: ReceiptWindows) {
        let target = if past_dedup {
            windows.receipt + windows.dedup + 2
        } else {
            windows.receipt + 1
        };
        match &self.provider {
            Provider::Local(provider) => {
                let previous = self.advanced.fetch_max(target, Ordering::SeqCst);
                if target > previous {
                    provider.advance_clock(target - previous);
                }
            }
            Provider::Fixture(provider) => {
                let now = Utc::now() + ChronoDuration::seconds(target);
                let _sweep: RetentionSweep = provider
                    .sweep_retained(&self.space_id, now)
                    .await
                    .expect("the provider retention pass runs");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The typed protocol surface
// ---------------------------------------------------------------------------

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid request")
}

/// One authenticated client session bound to its admitted principal and space.
pub struct Actor<'a> {
    client: &'a Arc<RemoteAdapter>,
    principal: &'a Principal,
    space_id: &'a str,
}

impl Actor<'_> {
    pub const fn space(&self) -> &str {
        self.space_id
    }

    pub fn principal_id(&self) -> &str {
        self.principal.principal_id.as_str()
    }

    pub async fn describe(&self) -> Value {
        wire(&self.client.describe(self.principal, dto(json!({}))).await)
    }

    pub async fn list_spaces(&self) -> Value {
        wire(
            &self
                .client
                .list_spaces(self.principal, dto(json!({})))
                .await,
        )
    }

    /// `memory.spaces.list` with an explicit page bound.
    pub async fn list_spaces_page(&self, limit: u64) -> Value {
        wire(
            &self
                .client
                .list_spaces(self.principal, dto(json!({ "limit": limit })))
                .await,
        )
    }

    pub async fn freeze_space(&self, key: &str, revision: u64) -> Value {
        wire(
            &self
                .client
                .freeze_space(
                    self.principal,
                    dto(json!({
                        "space_id": self.space_id,
                        "expected_revision": revision,
                        "idempotency_key": key,
                    })),
                )
                .await,
        )
    }

    pub async fn unfreeze_space(&self, key: &str, revision: u64) -> Value {
        wire(
            &self
                .client
                .unfreeze_space(
                    self.principal,
                    dto(json!({
                        "space_id": self.space_id,
                        "expected_revision": revision,
                        "idempotency_key": key,
                    })),
                )
                .await,
        )
    }

    pub async fn ensure_subject(&self, subject: &str) -> Value {
        wire(
            &self
                .client
                .ensure_subject(
                    self.principal,
                    dto(json!({ "space_id": self.space_id, "subject_id": subject })),
                )
                .await,
        )
    }

    pub async fn get_subject(&self, subject: &str) -> Value {
        wire(
            &self
                .client
                .get_subject(
                    self.principal,
                    dto(json!({ "space_id": self.space_id, "subject_id": subject })),
                )
                .await,
        )
    }

    pub async fn fence_subject(
        &self,
        subject: &str,
        target: Value,
        action: &str,
        revision: u64,
        key: &str,
    ) -> Value {
        wire(
            &self
                .client
                .fence_subject(
                    self.principal,
                    dto(json!({
                        "space_id": self.space_id,
                        "subject_id": subject,
                        "target": target,
                        "action": action,
                        "expected_revision": revision,
                        "idempotency_key": key,
                    })),
                )
                .await,
        )
    }

    pub async fn list_grants(&self, filter: Value) -> Value {
        let mut request = json!({ "space_id": self.space_id });
        if let Some(object) = filter.as_object() {
            for (key, value) in object {
                request[key] = value.clone();
            }
        }
        wire(&self.client.list_grants(self.principal, dto(request)).await)
    }

    pub async fn grant(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.grant(self.principal, dto(request)).await)
    }

    pub async fn revoke_grant(&self, grant_id: &str, revision: u64, key: &str) -> Value {
        wire(
            &self
                .client
                .revoke_grant(
                    self.principal,
                    dto(json!({
                        "space_id": self.space_id,
                        "grant_id": grant_id,
                        "expected_revision": revision,
                        "idempotency_key": key,
                    })),
                )
                .await,
        )
    }

    pub async fn capture(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.capture(self.principal, dto(request)).await)
    }

    pub async fn list_candidates(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .list_candidates(self.principal, dto(request))
                .await,
        )
    }

    pub async fn get_candidate(&self, candidate_id: &str) -> Value {
        wire(
            &self
                .client
                .get_candidate(
                    self.principal,
                    dto(json!({ "space_id": self.space_id, "candidate_id": candidate_id })),
                )
                .await,
        )
    }

    pub async fn dismiss_candidate(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .dismiss_candidate(self.principal, dto(request))
                .await,
        )
    }

    pub async fn review_candidate(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .review_candidate(self.principal, dto(request))
                .await,
        )
    }

    pub async fn create_record(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .create_record(self.principal, dto(request))
                .await,
        )
    }

    pub async fn get_record(&self, record_id: &str) -> Value {
        wire(
            &self
                .client
                .get_record(
                    self.principal,
                    dto(json!({ "space_id": self.space_id, "record_id": record_id })),
                )
                .await,
        )
    }

    pub async fn update_record(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .update_record(self.principal, dto(request))
                .await,
        )
    }

    pub async fn delete_record(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .delete_record(self.principal, dto(request))
                .await,
        )
    }

    pub async fn list_records(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.list_records(self.principal, dto(request)).await)
    }

    pub async fn promote_record(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .promote_record(self.principal, dto(request))
                .await,
        )
    }

    pub async fn recall(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.recall(self.principal, dto(request)).await)
    }

    pub async fn get_profile(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.get_profile(self.principal, dto(request)).await)
    }

    pub async fn update_profile(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(
            &self
                .client
                .update_profile(self.principal, dto(request))
                .await,
        )
    }

    pub async fn get_receipt(&self, receipt_id: &str) -> Value {
        wire(
            &self
                .client
                .get_receipt(
                    self.principal,
                    dto(json!({ "space_id": self.space_id, "receipt_id": receipt_id })),
                )
                .await,
        )
    }
}

// ---------------------------------------------------------------------------
// Raw signed session (wire-level rows only)
// ---------------------------------------------------------------------------

/// A raw signed session, so the exact wire object can carry shapes the typed
/// client cannot build (an unknown op, an unknown decision action).
pub struct RawSession {
    socket: Arc<WsTransport<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    session_id: String,
    responder: Arc<ConnectResponder>,
    server_socket: Arc<dyn Transport>,
    sequence: i64,
    /// Ephemeral loopback port the owned listener accepted on.
    port: u16,
}

/// A raw setup stage that fails before a session exists. Only the SDK's
/// signed-hello verification is modelled; every other setup failure still
/// panics, so an `Err` proves the signed hello was received and rejected.
#[derive(Debug)]
enum RawSetupFailure {
    /// The SDK's own hello-verification error for the expected host key.
    Hello(CoreError),
}

impl RawSetupFailure {
    /// A rejected host key is the SDK's handshake classification: the raw
    /// exchange reached signed responder-hello verification and that
    /// verification refused the expected key.
    const fn is_signed_hello_rejection(&self) -> bool {
        matches!(self, Self::Hello(CoreError::HandshakeFailed { .. }))
    }
}

/// Connect the raw client, send its signed initiator hello, verify the signed
/// responder hello against `expected_host_pubkey` and read the session snapshot.
async fn dial_raw_hello(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    host_id: &str,
    client_capabilities: &[&str],
    expected_host_pubkey: [u8; 32],
) -> Result<
    (
        Arc<WsTransport<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
        String,
    ),
    RawSetupFailure,
> {
    let (ws, _response) = tokio::time::timeout(
        BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the localhost websocket connect is bounded")
    .expect("connect the localhost websocket");
    let socket = Arc::new(WsTransport::new(ws));
    let hello = sign_hello_ed25519(
        &seed,
        &format!(
            "c-matrix-raw-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ),
        &connect_manifest_with(&format!("{host_id}-raw-client"), client_capabilities),
        None,
    )
    .expect("sign the initiator hello");
    socket
        .send(&serde_json::to_vec(&hello).expect("hello bytes"))
        .await
        .expect("hello send");
    let responder_hello: ConnectHello = serde_json::from_slice(
        &tokio::time::timeout(BOUND, socket.recv())
            .await
            .expect("the responder hello is bounded")
            .expect("responder hello"),
    )
    .expect("decode the responder hello");
    verify_hello_ed25519(
        &expected_host_pubkey,
        &peer_id(&HOST_SEED),
        &responder_hello,
        None,
    )
    .map_err(RawSetupFailure::Hello)?;
    let session: ConnectSession = serde_json::from_slice(
        &tokio::time::timeout(BOUND, socket.recv())
            .await
            .expect("the session snapshot is bounded")
            .expect("session snapshot"),
    )
    .expect("decode the session snapshot");
    Ok((socket, session.session_id.to_string()))
}

impl RawSession {
    async fn open(
        service: Arc<dyn RemoteMemoryService>,
        host_id: &str,
        seed: [u8; 32],
        client_capabilities: &[&str],
    ) -> Self {
        Self::open_expecting(
            service,
            host_id,
            seed,
            client_capabilities,
            pubkey(&HOST_SEED),
        )
        .await
        .expect("the raw session establishes with the published host key")
    }

    /// The real raw setup primitive: the same listener/responder/hello path,
    /// parameterised by the host key the responder hello must verify against.
    /// A wrong key is the raw setup-failure control's deterministic signed
    /// failure; the failure path reuses the same bounded listener settlement as
    /// [`open_endpoint_expecting`].
    async fn open_expecting(
        service: Arc<dyn RemoteMemoryService>,
        host_id: &str,
        seed: [u8; 32],
        client_capabilities: &[&str],
        expected_host_pubkey: [u8; 32],
    ) -> Result<Self, RawSetupFailure> {
        let (addr, host) = spawn_expected_endpoint_listener(service, host_id, seed).await;
        let port = addr.port();
        // The listener task is owned until the hello/session exchange succeeds;
        // a failure aborts and bounded-joins it before re-raising.
        let client_side = std::panic::AssertUnwindSafe(dial_raw_hello(
            addr,
            seed,
            host_id,
            client_capabilities,
            expected_host_pubkey,
        ))
        .catch_unwind()
        .await;
        let (socket, session_id) = match client_side {
            Ok(Ok(established)) => established,
            Ok(Err(failure)) => match settle_endpoint_listener(host, port).await {
                Ok(_) => return Err(failure),
                Err(problem) => panic!(
                    "the raw endpoint setup failed ({failure:?}) and its listener settlement also failed: {problem}"
                ),
            },
            Err(payload) => match settle_endpoint_listener(host, port).await {
                Ok(_) => std::panic::resume_unwind(payload),
                Err(problem) => {
                    let original = panic_message(&*payload);
                    panic!(
                        "the raw endpoint setup panicked ({original}) and its listener settlement also failed: {problem}"
                    );
                }
            },
        };
        let served = host.await.expect("the listener task");
        Ok(Self {
            socket,
            session_id,
            responder: served.responder,
            server_socket: served.socket,
            sequence: 0,
            port,
        })
    }

    /// The ephemeral loopback port this session's listener accepted on.
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Send one signed invoke envelope and return the response envelope as-is:
    /// a success carries `payload`, a refusal carries `error`.
    pub async fn invoke(&mut self, op: &str, payload: &Value) -> Value {
        let request = self.signed_invoke(op, payload);
        self.sequence += 1;
        self.socket
            .send(&serde_json::to_vec(&request).expect("request bytes"))
            .await
            .expect("request send");
        serde_json::from_slice(
            &tokio::time::timeout(BOUND, self.socket.recv())
                .await
                .expect("the invoke response is bounded")
                .expect("response recv"),
        )
        .expect("decode the invoke response")
    }

    fn signed_invoke(&self, op: &str, payload: &Value) -> Value {
        let signed = json!({
            "session_id": self.session_id,
            "sequence": self.sequence,
            "request_id": format!("c-matrix-raw-{}", self.sequence),
            "op": op,
            "payload": payload,
        });
        let bytes = serde_jcs::to_vec(&signed).expect("signed object canonicalizes");
        let signature = SigningKey::from_bytes(&ADMIN_SEED).sign(&bytes);
        let mut envelope = signed.as_object().expect("signed object").clone();
        envelope.insert("extensions".into(), json!({}));
        envelope.insert(
            "signature".into(),
            json!(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes())),
        );
        Value::Object(envelope)
    }

    /// Close every owned transport and the responder, bounded. Each failure is
    /// returned instead of discarded, so a caller can settle every owner before
    /// reporting the original scenario failure; the responder must really reach
    /// `Closed` (a dropped handle is not a joined listener).
    pub async fn close(self) -> Result<(), String> {
        let responder = close_responder(Arc::clone(&self.responder));
        let client = tokio::time::timeout(BOUND, self.socket.close());
        let server = tokio::time::timeout(BOUND, self.server_socket.close());
        let (responder, client, server) = tokio::join!(responder, client, server);
        let mut failures: Vec<String> = Vec::new();
        if let Err(error) = responder {
            failures.push(error);
        }
        match client {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                failures.push(format!("the raw client transport close failed: {error}"));
            }
            Err(_) => failures
                .push("the raw client transport close did not finish within the bound".to_owned()),
        }
        match server {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                failures.push(format!("the raw server transport close failed: {error}"));
            }
            Err(_) => failures
                .push("the raw server transport close did not finish within the bound".to_owned()),
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

// ---------------------------------------------------------------------------
// Wire projection and assertions
// ---------------------------------------------------------------------------

/// Project one typed port result onto its wire shape: the success payload, or
/// the refusal as the envelope carries it. A refusal is never a success value.
pub fn wire<T: serde::Serialize>(result: &SpokeResult<T>) -> Value {
    match result {
        SpokeResult::Ok(value) => serde_json::to_value(value).expect("serializable response"),
        SpokeResult::Reject(reject) => json!({
            "error": {
                "code": reject.code.as_str(),
                "details": reject.details.clone().map_or(Value::Null, Value::Object),
                "message": reject.message,
            }
        }),
    }
}

/// The committed receipt of one mutation; a refusal panics with its code.
pub fn receipt(body: &Value) -> &Value {
    assert_eq!(
        body["status"], "committed",
        "expected a committed receipt, got {body}"
    );
    body
}

/// The `result` member of a committed receipt.
pub fn result(body: &Value) -> &Value {
    &receipt(body)["result"]
}

pub fn receipt_id(body: &Value) -> &str {
    receipt(body)["receipt_id"]
        .as_str()
        .expect("a committed receipt names itself")
}

pub fn error_code(body: &Value) -> &str {
    body["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("expected a memory error envelope, got {body}"))
}

pub fn error_details(body: &Value) -> &Value {
    &body["error"]["details"]
}

pub fn retry_class(body: &Value) -> &str {
    error_details(body)["retry_class"]
        .as_str()
        .unwrap_or_else(|| panic!("expected details.retry_class, got {body}"))
}

pub fn is_committed(body: &Value) -> bool {
    body["status"] == json!("committed")
}

pub fn is_rejected(body: &Value) -> bool {
    body.get("error").is_some()
}

/// The candidate ids of a committed capture, in order.
pub fn candidate_ids(body: &Value) -> Vec<String> {
    result(body)["candidate_ids"]
        .as_array()
        .expect("a committed capture names its candidates")
        .iter()
        .map(|id| id.as_str().expect("candidate id").to_owned())
        .collect()
}

/// The record ids of a committed review, in order.
pub fn record_ids(body: &Value) -> Vec<String> {
    result(body)["record_ids"]
        .as_array()
        .expect("a committed review names its records")
        .iter()
        .map(|id| id.as_str().expect("record id").to_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// Request builders
// ---------------------------------------------------------------------------

/// A capture body for an explicit subject: `scope` is the capture placement
/// (`shared` or a context).
pub fn capture_body_for(subject: &str, key: &str, text: &str, scope: &Value) -> Value {
    let context = scope["context_id"].as_str();
    json!({
        "subject_id": subject,
        "content": { "text": text },
        "capture_scope": scope,
        "source_context_ids": context.map_or_else(Vec::new, |context| vec![context]),
        "idempotency_key": key,
    })
}

/// A capture body for the matrix subject.
pub fn capture_body(key: &str, text: &str, scope: &Value) -> Value {
    let context = scope["context_id"].as_str();
    json!({
        "subject_id": SUBJECT,
        "content": { "text": text },
        "capture_scope": scope,
        "source_context_ids": context.map_or_else(Vec::new, |context| vec![context]),
        "idempotency_key": key,
    })
}

pub fn shared_scope() -> Value {
    json!({ "visibility": "shared" })
}

pub fn context_scope(context_id: &str) -> Value {
    json!({ "visibility": "context", "context_id": context_id })
}

pub fn review_body(candidate_id: &str, revision: u64, key: &str) -> Value {
    json!({
        "candidate_id": candidate_id,
        "expected_revision": revision,
        "decision": { "action": "promote" },
        "idempotency_key": key,
    })
}

pub fn dismiss_body(candidate_id: &str, revision: u64, key: &str) -> Value {
    json!({
        "candidate_id": candidate_id,
        "expected_revision": revision,
        "reason": "dismissed by the conformance matrix",
        "idempotency_key": key,
    })
}

pub fn record_body(key: &str, kind: &str, text: &str, scope: &Value) -> Value {
    json!({
        "subject_id": SUBJECT,
        "kind": kind,
        "content": { "text": text },
        "scope": scope,
        "idempotency_key": key,
    })
}

pub fn subject_all(space_id: &str) -> Value {
    json!({
        "space_id": space_id,
        "subject_id": SUBJECT,
        "selection": { "mode": "subject_all" },
    })
}

pub fn recall_request(space_id: &str, selection: &Value, max_items: u64) -> Value {
    json!({
        "space_id": space_id,
        "subject_id": SUBJECT,
        "selection": selection,
        "budget": { "max_items": max_items },
    })
}

pub fn grant_body(capability: &str, grantee: &str, key: &str) -> Value {
    json!({
        "principal_id": grantee,
        "capabilities": [capability],
        "idempotency_key": key,
    })
}

pub fn subject_grant_body(capability: &str, grantee: &str, subject: &str, key: &str) -> Value {
    json!({
        "subject_id": subject,
        "principal_id": grantee,
        "capabilities": [capability],
        "idempotency_key": key,
    })
}

pub fn context_grant_body(
    capability: &str,
    grantee: &str,
    subject: &str,
    contexts: &[&str],
    key: &str,
) -> Value {
    json!({
        "subject_id": subject,
        "principal_id": grantee,
        "capabilities": [capability],
        "context_ids": contexts,
        "shared_access": false,
        "idempotency_key": key,
    })
}

/// Register the subject and the two grants a matrix scenario normally needs:
/// the admin keeps its operator grant, and the second peer is given the listed
/// capabilities at space level.
pub async fn bootstrap(matrix: &Matrix, capabilities: &[&str]) {
    let admin = matrix.admin();
    assert_eq!(
        result(&admin.ensure_subject(SUBJECT).await)["created"],
        json!(true)
    );
    let peer_id = matrix.peer_principal().principal_id.as_str();
    for (index, capability) in capabilities.iter().enumerate() {
        let key = format!("c-bootstrap-{index}-{capability}");
        let granted = admin.grant(grant_body(capability, peer_id, &key)).await;
        assert!(
            is_committed(&granted),
            "the bootstrap {capability} grant commits: {granted}"
        );
    }
}

// ---------------------------------------------------------------------------
// Owned-input operations for a racing task
// ---------------------------------------------------------------------------

/// Capture with owned inputs, so the future can be spawned while the scenario
/// keeps the matrix.
pub async fn capture_with(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    body: Value,
) -> Value {
    let mut body = body;
    body["space_id"] = json!(space_id);
    wire(&client.capture(&principal, dto(body)).await)
}

/// Freeze with owned inputs, so the future can be spawned while the scenario
/// keeps the matrix.
pub async fn freeze_with(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    key: &'static str,
    revision: u64,
) -> Value {
    wire(
        &client
            .freeze_space(
                &principal,
                dto(json!({
                    "space_id": space_id,
                    "expected_revision": revision,
                    "idempotency_key": key,
                })),
            )
            .await,
    )
}

/// Fence with owned inputs, so the future can be raced against another client.
pub async fn fence_with(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    key: &'static str,
    revision: u64,
    subject_id: &'static str,
) -> Value {
    fence_action_with(
        client, principal, space_id, key, revision, "fence", subject_id,
    )
    .await
}

/// One subject-target fence transition with owned inputs.
pub async fn fence_action_with(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    key: &'static str,
    revision: u64,
    action: &'static str,
    subject_id: &'static str,
) -> Value {
    wire(
        &client
            .fence_subject(
                &principal,
                dto(json!({
                    "space_id": space_id,
                    "subject_id": subject_id,
                    "target": { "type": "subject" },
                    "action": action,
                    "expected_revision": revision,
                    "idempotency_key": key,
                })),
            )
            .await,
    )
}

/// Review with owned inputs, so the future can be spawned for a real race.
pub async fn review_with(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    body: Value,
) -> Value {
    let mut body = body;
    body["space_id"] = json!(space_id);
    wire(&client.review_candidate(&principal, dto(body)).await)
}

// ---------------------------------------------------------------------------
// The real Character/binding fixture (public Core actor surface)
// ---------------------------------------------------------------------------

/// A Character created through the public Core actor surface, with its initial
/// active binding. Both ids are real product identities: the Character subject
/// and the binding context a capture is placed into.
pub struct CharacterFixture {
    pub subject_id: String,
    pub binding_id: String,
}

/// Create one owned World with one Character and its initial binding through
/// the public Core actor path, then release the Core open so the caller can
/// open the workspace's provider itself.
async fn create_character_fixture(user_home: &std::path::Path) -> (CharacterFixture, String) {
    let home = CoreHomeService::open(user_home.to_path_buf()).expect("open the core home");
    let creator_id = home
        .register_creator(CoreRegisterCreatorRequest {
            display_name: Some("C Matrix".parse().expect("valid display name")),
            platform_creator_id: None,
        })
        .await
        .expect("register the active creator")
        .creator_id;
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        user_home,
        &creator_id,
        LOCAL_WORKSPACE,
    ))
    .expect("workspace dir");
    home.select_workspace(SetActiveWorkspaceRequest {
        creator_id: None,
        workspace_slug: LOCAL_WORKSPACE.to_owned(),
    })
    .await
    .expect("select the workspace");

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.to_path_buf(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("open Core over the disposable home");
    let owner = core.active_principal().await.expect("active principal");
    let world = core
        .create_world(&owner, dto(json!({ "title": "C matrix world" })))
        .await
        .expect("create an owned World");
    let created = core
        .create_character(
            &owner,
            dto(json!({
                "display_name": "Mira",
                "world_id": world.world_id.as_str(),
            })),
        )
        .await
        .expect("create an owned Character with its initial binding");
    let fixture = CharacterFixture {
        subject_id: created.character.character_id.as_str().to_owned(),
        binding_id: created.binding.binding_id.as_str().to_owned(),
    };
    drop(core);
    (fixture, creator_id)
}

// ---------------------------------------------------------------------------
// Durable state observation (I253): the provider's own storage, read-only
// ---------------------------------------------------------------------------

/// The provider's durable state around a read series, observed through ONE
/// read-only connection held for the whole observation: that connection's
/// `data_version` pragma (SQLite changes it only when *another* connection
/// commits, so any provider write — insertion or in-place update — moves it),
/// the exact rows of the seeded objects as the database reports their own
/// columns, and (local only) the exact bytes published under the provider's
/// public file root.
///
/// Private page images and a copied schema are never used; the rows are the
/// seeded keys the scenario already owns.
#[derive(Debug, PartialEq, Eq)]
pub struct DurableState {
    /// `PRAGMA data_version` of the held read-only connection.
    pub data_version: i64,
    /// `(label, columns)` per seeded row; an absent row is an empty list.
    pub rows: Vec<(String, Vec<(String, String)>)>,
    pub publications: Vec<(String, Vec<u8>)>,
}

#[derive(Clone, Copy)]
enum Store {
    Local,
    Fixture,
}

/// One held read-only connection to the provider's own durable file.
pub struct DurableObserver {
    conn: sqlx::sqlite::SqliteConnection,
    store: Store,
    space_id: String,
    subject_id: String,
    record_ids: Vec<String>,
    candidate_id: Option<String>,
    receipt_id: Option<String>,
    profile_kind: Option<String>,
    publications_root: Option<PathBuf>,
}

/// Every file under `root`, as (relative path, exact bytes), sorted.
///
/// The root is the provider's own published projection — its absence or an
/// unreadable entry/file is an observation failure, never an empty projection.
fn published_files(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        let entries = std::fs::read_dir(dir).unwrap_or_else(|error| {
            panic!(
                "the provider projection directory {} is enumerable: {error}",
                dir.display()
            )
        });
        for entry in entries {
            let entry = entry.unwrap_or_else(|error| {
                panic!(
                    "an entry under the provider projection {} is readable: {error}",
                    dir.display()
                )
            });
            let path = entry.path();
            let file_type = entry.file_type().unwrap_or_else(|error| {
                panic!(
                    "the provider projection entry {} reports its type: {error}",
                    path.display()
                )
            });
            if file_type.is_dir() {
                walk(root, &path, out);
            } else {
                let bytes = std::fs::read(&path).unwrap_or_else(|error| {
                    panic!(
                        "the published provider file {} is readable: {error}",
                        path.display()
                    )
                });
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.push((relative, bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
}

impl Matrix {
    /// Open the durable observer **before** the first protocol read, on one
    /// held read-only connection.
    pub async fn durable_observer(
        &self,
        subject_id: &str,
        record_ids: &[String],
        candidate_id: Option<&str>,
        receipt_id: Option<&str>,
        profile_kind: Option<&str>,
    ) -> DurableObserver {
        let (db, store, publications_root) = match (&self.home, &self.provider) {
            (Home::Local { user_home, .. }, Provider::Local(provider)) => (
                nexus_home_layout::workspace_state_db_path(
                    user_home,
                    self.admin_principal.principal_id.as_str(),
                    LOCAL_WORKSPACE,
                ),
                Store::Local,
                Some(provider.file_root()),
            ),
            (Home::Remote { db, .. }, Provider::Fixture(_)) => (db.clone(), Store::Fixture, None),
            _ => panic!("the provider kind and its durable home must agree"),
        };
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db)
            .read_only(true);
        let conn = sqlx::sqlite::SqliteConnection::connect_with(&options)
            .await
            .expect("one read-only connection to the provider's own file");
        DurableObserver {
            conn,
            store,
            space_id: self.space_id.clone(),
            subject_id: subject_id.to_owned(),
            record_ids: record_ids.to_vec(),
            candidate_id: candidate_id.map(str::to_owned),
            receipt_id: receipt_id.map(str::to_owned),
            profile_kind: profile_kind.map(str::to_owned),
            publications_root,
        }
    }
}

impl DurableObserver {
    /// Observe the same held connection: its `data_version`, the seeded rows
    /// and the published file projection.
    pub async fn observe(&mut self) -> DurableState {
        let (data_version,): (i64,) = sqlx::query_as("PRAGMA data_version")
            .fetch_one(&mut self.conn)
            .await
            .expect("the held connection reports its data_version");
        let mut rows: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let record_ids = self.record_ids.clone();
        let candidate_id = self.candidate_id.clone();
        let receipt_id = self.receipt_id.clone();
        let profile_kind = self.profile_kind.clone();
        let space_id = self.space_id.clone();
        let subject_id = self.subject_id.clone();
        for record_id in &record_ids {
            let sql = match self.store {
                Store::Local => "SELECT * FROM memory_records WHERE record_id = ?",
                Store::Fixture => "SELECT * FROM records WHERE record_id = ?",
            };
            rows.push((
                format!("record:{record_id}"),
                self.row(sql, &[record_id.as_str()]).await,
            ));
        }
        if let Some(candidate_id) = candidate_id {
            let candidate = match self.store {
                Store::Local => "SELECT * FROM memory_candidates WHERE candidate_id = ?",
                Store::Fixture => "SELECT * FROM candidates WHERE candidate_id = ?",
            };
            rows.push((
                format!("candidate:{candidate_id}"),
                self.row(candidate, &[candidate_id.as_str()]).await,
            ));
        }
        if let Some(receipt_id) = receipt_id {
            let receipt = match self.store {
                Store::Local => "SELECT * FROM memory_receipts WHERE receipt_id = ?",
                Store::Fixture => "SELECT * FROM receipts WHERE receipt_id = ?",
            };
            rows.push((
                format!("receipt:{receipt_id}"),
                self.row(receipt, &[receipt_id.as_str()]).await,
            ));
        }
        if let Some(profile_kind) = profile_kind {
            let profile = match self.store {
                Store::Local => {
                    "SELECT * FROM memory_profiles WHERE space_id = ? AND subject_id = ? \
                     AND profile_kind = ?"
                }
                Store::Fixture => {
                    "SELECT * FROM profiles WHERE space_id = ? AND subject_id = ? \
                     AND profile_kind = ?"
                }
            };
            rows.push((
                format!("profile:{profile_kind}"),
                self.row(
                    profile,
                    &[
                        space_id.as_str(),
                        subject_id.as_str(),
                        profile_kind.as_str(),
                    ],
                )
                .await,
            ));
        }
        let subject = match self.store {
            Store::Local => "SELECT * FROM memory_subjects WHERE space_id = ? AND subject_id = ?",
            Store::Fixture => "SELECT * FROM subjects WHERE space_id = ? AND subject_id = ?",
        };
        rows.push((
            format!("subject:{subject_id}"),
            self.row(subject, &[space_id.as_str(), subject_id.as_str()])
                .await,
        ));
        let space = match self.store {
            Store::Local => "SELECT * FROM memory_spaces WHERE space_id = ?",
            Store::Fixture => "SELECT * FROM spaces WHERE space_id = ?",
        };
        rows.push((
            format!("space:{space_id}"),
            self.row(space, &[space_id.as_str()]).await,
        ));
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        DurableState {
            data_version,
            rows,
            publications: self
                .publications_root
                .as_deref()
                .map(published_files)
                .unwrap_or_default(),
        }
    }

    /// One seeded row, as the database reports its own columns.
    async fn row(&mut self, sql: &str, binds: &[&str]) -> Vec<(String, String)> {
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for bind in binds {
            query = query.bind(*bind);
        }
        let rows = query
            .fetch_all(&mut self.conn)
            .await
            .unwrap_or_else(|error| panic!("the seeded row is readable: {error}"));
        rows.first().map(row_columns).unwrap_or_default()
    }

    /// The local provider's record-journal row count on the SAME held
    /// connection. `None` for the fixture, which has no journal table.
    pub async fn journal_count(&mut self) -> Option<i64> {
        match self.store {
            Store::Local => {
                let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory_journal")
                    .fetch_one(&mut self.conn)
                    .await
                    .expect("the held connection reads the record journal row count");
                Some(count)
            }
            Store::Fixture => None,
        }
    }

    /// The receipt ids recorded for one op in this space, as the provider's own
    /// table reports them. Exact ids, never a count heuristic, so a refused op
    /// can prove it created no receipt while a committed op names the one it
    /// did.
    pub async fn receipt_ids_for(&mut self, op: &str) -> Vec<String> {
        let sql = match self.store {
            Store::Local => {
                "SELECT receipt_id FROM memory_receipts WHERE space_id = ? AND op = ? \
                 ORDER BY receipt_id"
            }
            Store::Fixture => {
                "SELECT receipt_id FROM receipts WHERE space_id = ? AND op = ? ORDER BY receipt_id"
            }
        };
        let space_id = self.space_id.clone();
        let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(op)
            .fetch_all(&mut self.conn)
            .await
            .expect("the held connection reads the op's receipt ids");
        rows.into_iter().map(|(receipt_id,)| receipt_id).collect()
    }

    /// Close the held connection explicitly: both the bound and the connection's
    /// own close result must succeed.
    pub async fn close(self) {
        match tokio::time::timeout(BOUND, self.conn.close()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => panic!("the held read-only connection closes: {error}"),
            Err(tokio::time::error::Elapsed { .. }) => {
                panic!("the held read-only connection close is bounded");
            }
        }
    }
}

/// Every column of one row as `(name, value)` — the database's own report, not
/// a copied schema.
fn row_columns(row: &sqlx::sqlite::SqliteRow) -> Vec<(String, String)> {
    use sqlx::{Column, Row, ValueRef};
    let mut out = Vec::with_capacity(row.columns().len());
    for (index, column) in row.columns().iter().enumerate() {
        let raw = row.try_get_raw(index).expect("a readable column");
        let value = if raw.is_null() {
            "null".to_owned()
        } else if let Ok(text) = row.try_get::<String, _>(index) {
            format!("text:{text}")
        } else if let Ok(number) = row.try_get::<i64, _>(index) {
            format!("int:{number}")
        } else if let Ok(number) = row.try_get::<f64, _>(index) {
            format!("real:{number}")
        } else {
            match row.try_get::<Vec<u8>, _>(index) {
                Ok(bytes) => format!("blob:{}", hex(&bytes)),
                Err(error) => panic!("an unsupported column value: {error}"),
            }
        };
        out.push((column.name().to_owned(), value));
    }
    out
}

// ---------------------------------------------------------------------------
// Real provider selection / open controls (I256)
// ---------------------------------------------------------------------------

/// The dialing identity of a selection control, and the credential id the
/// disposable home provisions for it.
pub const SELECTION_CLIENT_SEED: [u8; 32] = [0x61; 32];
pub const SELECTION_CREDENTIAL_ID: &str = "cred-c-matrix-selection";

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").expect("a String write never fails");
    }
    out
}

/// A one-shot signed host for a real `CoreService` remote open: the endpoint,
/// the host identity and the dialing peer the open must authenticate.
pub struct SelectionHost {
    pub endpoint: String,
    pub host_pubkey: [u8; 32],
    addr: std::net::SocketAddr,
    task: tokio::task::JoinHandle<Result<Arc<ConnectResponder>, String>>,
}

/// How long a failed open's listener is given to finish an already-dialed
/// handshake before its owned task is aborted (loopback handshakes are
/// immediate).
const SELECTION_SETTLE_GRACE: Duration = Duration::from_secs(2);

impl SelectionHost {
    /// Await the responder that served a successful open's signed session. The
    /// handle is retained across the deadline: an expired session aborts and
    /// boundedly joins its own listener instead of dropping the `JoinHandle`
    /// and detaching a task that still owns the port and the service.
    pub async fn session(self) -> Result<Arc<ConnectResponder>, String> {
        let mut task = self.task;
        match tokio::time::timeout(BOUND, &mut task).await {
            Ok(Ok(Ok(responder))) => Ok(responder),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(join)) => Err(format!("the selection listener task failed: {join}")),
            Err(_) => {
                task.abort();
                // A task that completed just before the abort still yields its
                // responder here, so the caller closes it through the public
                // route instead of losing it.
                match tokio::time::timeout(BOUND, &mut task).await {
                    Ok(Ok(Ok(responder))) => Ok(responder),
                    Ok(Ok(Err(error))) => Err(error),
                    Ok(Err(join)) if join.is_cancelled() => Err(
                        "the selection session did not establish within the bound; its listener was aborted and joined"
                            .to_owned(),
                    ),
                    Ok(Err(join)) => Err(format!(
                        "the timed-out selection listener failed to join: {join}"
                    )),
                    Err(_) => Err(
                        "the timed-out selection listener did not join within the bound".to_owned(),
                    ),
                }
            }
        }
    }

    /// Settle the listener of an open that failed: an already-dialed handshake
    /// gets a short bounded grace, otherwise the owned task is aborted and
    /// joined so its port is released. An established responder is returned for
    /// the caller's own bounded close; nothing here panics or reports a timeout
    /// as a live session.
    pub async fn settle_failed(self) -> Result<Option<Arc<ConnectResponder>>, String> {
        let mut task = self.task;
        match tokio::time::timeout(SELECTION_SETTLE_GRACE, &mut task).await {
            Ok(Ok(Ok(responder))) => Ok(Some(responder)),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(join)) => Err(format!("the selection listener task failed: {join}")),
            Err(_) => {
                task.abort();
                match tokio::time::timeout(BOUND, &mut task).await {
                    Ok(Ok(Ok(responder))) => Ok(Some(responder)),
                    Ok(Ok(Err(error))) => Err(error),
                    Ok(Err(join)) if join.is_cancelled() => Ok(None),
                    Ok(Err(join)) => Err(format!("the aborted selection listener failed: {join}")),
                    Err(_) => Err(
                        "the aborted selection listener did not join within the bound".to_owned(),
                    ),
                }
            }
        }
    }

    /// The listener's port, so a caller can verify it was released.
    pub const fn port(&self) -> u16 {
        self.addr.port()
    }
}

/// Serve `service` for one real `CoreService::open` over a signed loopback
/// session. `capabilities` is the host's own signed hello, so an empty list is
/// a host that negotiates no memory family.
pub async fn serve_selection_host(
    service: Arc<dyn RemoteMemoryService>,
    host_id: &str,
    capabilities: &[&str],
    client_seed: [u8; 32],
) -> SelectionHost {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let client_peer = peer_id(&client_seed);
    let client_pubkey = pubkey(&client_seed);
    let host_manifest = manifest_with(host_id, capabilities);
    // The spawned listener returns its own failure instead of panicking; the
    // `Ok(...)` tail and the `return Err(...)` arms infer
    // `Result<Arc<ConnectResponder>, String>`.
    let task = tokio::spawn(async move {
        let accepted = tokio::time::timeout(BOUND, listener.accept()).await;
        let (tcp, address) = match accepted {
            Ok(Ok(accepted)) => accepted,
            Ok(Err(error)) => {
                return Err(format!("the selection listener accepts the open: {error}"));
            }
            Err(_) => {
                return Err(
                    "the selection listener received no connection within the bound".to_owned(),
                );
            }
        };
        if !address.ip().is_loopback() {
            return Err(format!(
                "the selection listener serves localhost only, got {address}"
            ));
        }
        let upgraded = tokio::time::timeout(
            BOUND,
            tokio_tungstenite::accept_async_with_config(
                tcp,
                Some(ws_config(DEFAULT_MAX_ENVELOPE_BYTES)),
            ),
        )
        .await;
        let ws = match upgraded {
            Ok(Ok(ws)) => ws,
            Ok(Err(error)) => return Err(format!("the selection handshake completes: {error}")),
            Err(_) => return Err("the selection handshake is bounded".to_owned()),
        };
        let socket: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
        let ports: Arc<dyn RemoteServePorts + Send + Sync> = Arc::new(
            RemoteServePortsComposite::new(Arc::new(MemoryOnlyHost), None, None)
                .with_memory(service),
        );
        Ok(connect_responder(ConnectResponderOptions {
            transport: socket,
            identity: RemoteIdentity { seed: HOST_SEED },
            manifest: host_manifest,
            allowlist: vec![client_peer.clone()],
            peer_keys: HashMap::from([(client_peer, client_pubkey)]),
            ports: Some(ports),
            invoke_timeout_ms: Some(5_000),
        })
        .await)
    });
    SelectionHost {
        endpoint: format!("ws://{addr}"),
        host_pubkey: pubkey(&HOST_SEED),
        addr,
        task,
    }
}

/// A disposable Core home for a selection control: the active creator and
/// workspace table plus the operator-provisioned credential for `client_seed`.
pub struct SelectionHome {
    _home: TempDir,
    pub user_home: PathBuf,
}

impl SelectionHome {
    pub fn new(creator: &str, client_seed: [u8; 32]) -> Self {
        let home = TempDir::new().expect("selection home");
        let user_home = home.path().to_path_buf();
        std::fs::create_dir_all(user_home.join(".nexus42")).expect("nexus home");
        std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home,
            creator,
            LOCAL_WORKSPACE,
        ))
        .expect("workspace dir");
        std::fs::write(
            user_home.join(".nexus42").join("config.toml"),
            format!(
                "active_creator_id = \"{creator}\"\n\
                 [active_workspace_slug_by_creator]\n\
                 \"{creator}\" = \"{LOCAL_WORKSPACE}\"\n"
            ),
        )
        .expect("seed the selection config");
        let credentials = user_home.join(".nexus42").join("memory-credentials");
        std::fs::create_dir_all(&credentials).expect("credential dir");
        let path = credentials.join(format!("{SELECTION_CREDENTIAL_ID}.json"));
        std::fs::write(
            &path,
            format!(
                "{{\"identity_seed_hex\":\"{}\",\"capability_token\":null}}",
                hex(&client_seed)
            ),
        )
        .expect("write the operator credential");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("credential mode 0600");
        }
        Self {
            _home: home,
            user_home,
        }
    }
}

/// The remote selection a Core open would persist for the served fixture.
pub fn remote_selection(
    space_id: &str,
    host: &SelectionHost,
) -> nexus_core::MemoryProviderSelection {
    nexus_core::MemoryProviderSelection::Remote(nexus_core::RemoteMemoryProviderSelection {
        space_id: space_id.to_owned(),
        credential_id: SELECTION_CREDENTIAL_ID.to_owned(),
        connect_endpoint: host.endpoint.clone(),
        remote_pubkey_hex: hex(&host.host_pubkey),
        allowlist: vec![peer_id(&HOST_SEED)],
    })
}

/// The typed code of a `CoreError::Coded` refusal.
pub fn core_code(error: &nexus_core::CoreError) -> &str {
    match error {
        nexus_core::CoreError::Coded { code, .. } => code,
        other => panic!("expected a coded Core refusal, got {other:?}"),
    }
}

/// The real local selection control: `CoreService::open` with the supported
/// local memory selection, then the pinned provider's own declarations and a
/// product memory read through the same open.
pub struct LocalSelection {
    pub described: Value,
    pub profiles: Value,
    pub pending: i64,
}

pub async fn open_selected_local(subject: &str, profile_kinds: &[&str]) -> LocalSelection {
    let home = SelectionHome::new(LOCAL_ADMIN_ID, SELECTION_CLIENT_SEED);
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.user_home.clone(),
        access: CoreAccess::EngineOwner,
        memory: Some(nexus_core::MemoryProviderSelection::Local),
    })
    .await
    .expect("the supported local memory selection opens");
    // Collect every observation, close the open, THEN surface the outcome: an
    // assertion must not run while this control still owns the service.
    let observed: Result<(Value, Value, i64), String> = match core.active_principal().await {
        Ok(core_principal) => {
            let pending = core
                .count_pending_reviews(&core_principal)
                .await
                .map(|value| value.count)
                .map_err(|error| format!("the pinned local binding serves memory: {error}"));
            match (pending, core.memory_provider_for_test()) {
                (Ok(pending), Ok(provider)) => {
                    // The provider's typed port takes the spoke-schema principal
                    // the admission maps; the Core principal is its
                    // creator-scoped counterpart.
                    let principal = principal(core_principal.creator_id());
                    let described = wire(&provider.describe(&principal, dto(json!({}))).await);
                    let profiles = wire(
                        &provider
                            .get_profile(
                                &principal,
                                dto(json!({
                                    "space_id": provider.space_id(),
                                    "subject_id": subject,
                                    "profile_kinds": profile_kinds,
                                })),
                            )
                            .await,
                    );
                    Ok((described, profiles, pending))
                }
                (Err(error), _) => Err(error),
                (_, Err(error)) => Err(format!(
                    "the local selection pins the local provider: {error}"
                )),
            }
        }
        Err(error) => Err(format!(
            "the local selection has an active principal: {error}"
        )),
    };
    let closed = core
        .close()
        .await
        .map_err(|error| format!("the local selection closes: {error}"));
    match (observed, closed) {
        (Ok((described, profiles, pending)), Ok(_)) => LocalSelection {
            described,
            profiles,
            pending,
        },
        (observed, closed) => panic!(
            "the local selection control failed before its assertion: observed={observed:?}, closed={closed:?}"
        ),
    }
}

/// The Core creator/workspace a selection control opens under (a Core-local
/// identity; the provider's admission maps the signed peer, not this id).
pub const SELECTION_CREATOR: &str = "ctr_c_matrix_selection";

/// A fresh durable fixture prepared for a real `CoreService` remote open: its
/// served service, the authority host label its manifest names, its space and
/// the operator-provisioned grant for the selection peer.
pub struct SelectionFixture {
    pub service: Arc<dyn RemoteMemoryService>,
    pub authority_host_id: String,
    pub space_id: String,
    /// The operator-provisioned principal the signed selection peer maps to.
    pub principal: String,
    provider: Arc<FixtureMemoryProvider>,
    _home: TempDir,
}

impl SelectionFixture {
    /// Release the erased service first, then close the provider it served
    /// through the bounded result-returning route, so an aggregating caller can
    /// finish every other owner before deciding.
    pub async fn settle(self) -> Result<(), String> {
        let Self {
            service,
            provider,
            _home,
            ..
        } = self;
        drop(service);
        close_fixture_bounded(provider).await
    }
}

pub async fn selection_fixture() -> SelectionFixture {
    let home = TempDir::new().expect("selection fixture home");
    let provider = Arc::new(
        FixtureMemoryProvider::open(&home.path().join("selection.sqlite"))
            .await
            .expect("open the selection fixture"),
    );
    let client_peer = peer_id(&SELECTION_CLIENT_SEED);
    let principal_id = format!("prn_{client_peer}");
    provider
        .provision_operator_admin(&principal_id)
        .await
        .expect("operator provisions the selection principal");
    let directory = Arc::new(PeerDirectory::new().with_peer(client_peer, principal_id.clone()));
    let service = provider.server(directory.admission());
    SelectionFixture {
        service,
        authority_host_id: provider.identity().authority_host_id.clone(),
        space_id: provider.identity().space_id.clone(),
        principal: principal_id,
        provider,
        _home: home,
    }
}
