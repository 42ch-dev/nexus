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
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::{Barrier, Notify};

use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::memory_provider::LocalMemoryProvider;
use nexus_core::{CoreAccess, CoreHomeService, CoreOpenOptions, CoreService};
use nexus_contracts::{CoreRegisterCreatorRequest, SetActiveWorkspaceRequest};
use nexus_memory_conformance::{
    FaultHold, FaultHoldLost, FaultPoint, FixtureMemoryProvider, PeerDirectory, RetentionSweep,
    TargetHold,
};
use spoke_connect::core::{derive_peer_id_from_ed25519_pubkey, sign_hello_ed25519};
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponder, ConnectResponderOptions,
    ConnectResponderState, MemoryPortService, RemoteAdapter, RemoteAdapterOptions, RemoteIdentity, RemoteMemoryService,
    RemoteServePorts, RemoteServePortsComposite, Transport,
};
use spoke_operations::spoke_schemas::connect::{ConnectHello, ConnectSession};
use spoke_operations::spoke_schemas::{
    Finding, HostCapabilityManifest, KnowledgeEntry, Principal, Relation, Rule, Scope, TimelineEvent,
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
struct Endpoint {
    client: Arc<RemoteAdapter>,
    responder: Arc<ConnectResponder>,
    socket: Arc<dyn Transport>,
}

impl Endpoint {
    async fn close(self) {
        self.client.close();
        self.responder.close();
        let closed = tokio::time::timeout(BOUND, self.socket.close()).await;
        assert!(
            matches!(closed, Ok(Ok(()))),
            "the session transport closes within the bound"
        );
        assert_eq!(
            self.responder.state(),
            ConnectResponderState::Closed,
            "the responder session is closed after the real transport cleanup"
        );
    }
}

/// Serve one fresh loopback session for `seed` against `service`.
async fn open_endpoint(
    service: Arc<dyn RemoteMemoryService>,
    host_id: &str,
    seed: [u8; 32],
) -> Endpoint {
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
        assert!(address.ip().is_loopback(), "the listener serves localhost only");
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
        let socket: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
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
        (socket, responder)
    });
    let (ws, _response) = tokio::time::timeout(
        BOUND,
        tokio_tungstenite::connect_async(format!("ws://{addr}")),
    )
    .await
    .expect("the localhost websocket connect is bounded")
    .expect("connect the localhost websocket");
    let socket: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let dialed = connect_remote_adapter(RemoteAdapterOptions {
        transport: Arc::clone(&socket),
        local_identity: RemoteIdentity { seed },
        local_manifest: manifest(&format!("{host_id}-client")),
        remote_pubkey: pubkey(&HOST_SEED),
        allowlist: vec![peer_id(&HOST_SEED)],
        invoke_timeout_ms: None,
        capability_token: None,
    });
    let client = tokio::time::timeout(BOUND, dialed)
        .await
        .expect("the signed dial is bounded")
        .expect("the signed dial succeeds");
    let (_server_socket, responder) = host.await.expect("the listener task");
    Endpoint {
        client,
        responder,
        socket,
    }
}

// ---------------------------------------------------------------------------
// The two provider legs
// ---------------------------------------------------------------------------

/// Where a matrix run's durable state lives.
enum Home {
    Local {
        _home: TempDir,
        user_home: PathBuf,
    },
    Remote {
        _home: TempDir,
        db: PathBuf,
    },
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
            LocalMemoryProvider::open(
                user_home.clone(),
                LOCAL_ADMIN_ID,
                LOCAL_WORKSPACE,
            )
            .await
            .expect("open the local provider"),
        )),
        Home::Remote { db, .. } => {
            Provider::Fixture(Arc::new(
                FixtureMemoryProvider::open(db)
                    .await
                    .expect("open the durable fixture provider"),
            ))
        }
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

impl Matrix {
    /// A fresh run over the requested provider implementation.
    pub async fn start(kind: Kind) -> Self {
        Self::assemble(
            kind,
            TempDir::new().expect("temporary matrix home"),
            None,
        )
        .await
    }

    /// A local run whose workspace already holds a real Character and its
    /// binding, created through the public Core actor surface before the
    /// provider opens (the same workspace state database the Core open used).
    pub async fn local_character() -> (Self, CharacterFixture) {
        let home = TempDir::new().expect("temporary matrix home");
        let (fixture, creator_id) = create_character_fixture(home.path()).await;
        let matrix = Self::assemble(Kind::Local, home, Some(creator_id)).await;
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
    async fn assemble(kind: Kind, home_dir: TempDir, creator_id: Option<String>) -> Self {
        let host_id = match kind {
            Kind::Local => "c-matrix-local",
            Kind::Remote => "c-matrix-remote",
        }
        .to_owned();
        let (home, provider, space_id, directory, admin_principal, peer_principal) = match kind {
            Kind::Local => {
                let user_home = home_dir.path().to_path_buf();
                let admin_id = creator_id.unwrap_or_else(|| LOCAL_ADMIN_ID.to_owned());
                let provider = LocalMemoryProvider::open(
                    user_home.clone(),
                    &admin_id,
                    LOCAL_WORKSPACE,
                )
                .await
                .expect("open the local provider");
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
            Kind::Remote => {
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
        };
        let service = build_service(&provider, &directory);
        let admin = open_endpoint(Arc::clone(&service), &host_id, ADMIN_SEED).await;
        let peer = open_endpoint(Arc::clone(&service), &host_id, PEER_SEED).await;
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

    pub fn admin_principal(&self) -> &Principal {
        &self.admin_principal
    }

    pub fn peer_principal(&self) -> &Principal {
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
        let admin = open_endpoint(Arc::clone(&service), &host_id, ADMIN_SEED).await;
        let peer = open_endpoint(Arc::clone(&service), &host_id, PEER_SEED).await;
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
        described["limits"][name]
            .as_u64()
            .map_or_else(|| panic!("describe advertises {name}: {described}"), |value| {
                i64::try_from(value).unwrap_or(i64::MAX)
            })
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

impl Drop for CaptureHold {
    fn drop(&mut self) {
        if let Self::Local { provider, token, .. } = self {
            provider.disarm_next_admission(*token);
        }
    }
}

impl CaptureHold {
    /// Wait, bounded, until the parked capture reached its point.
    pub async fn wait_reached(&mut self) -> Result<(), FaultHoldLost> {
        match self {
            Self::Local { entered, .. } => match tokio::time::timeout(BOUND, entered.notified()).await
            {
                Ok(()) => Ok(()),
                Err(_) => Err(FaultHoldLost::Unreached),
            },
            Self::FixturePoint(hold) => hold.wait_reached().await,
            Self::FixtureTarget(hold) => hold.wait_reached().await,
        }
    }

    /// Release the parked capture so it proceeds.
    pub async fn release(&mut self) {
        match self {
            Self::Local { release, .. } => {
                release.wait().await;
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
                provider.faults().fail_next(FaultPoint::AfterCommitBeforeResponse);
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
    pub fn space(&self) -> &str {
        self.space_id
    }

    pub fn principal_id(&self) -> &str {
        self.principal.principal_id.as_str()
    }

    pub async fn describe(&self) -> Value {
        wire(&self.client.describe(self.principal, dto(json!({}))).await)
    }

    pub async fn list_spaces(&self) -> Value {
        wire(&self.client.list_spaces(self.principal, dto(json!({}))).await)
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
        wire(&self.client.list_candidates(self.principal, dto(request)).await)
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
        wire(&self.client.dismiss_candidate(self.principal, dto(request)).await)
    }

    pub async fn review_candidate(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.review_candidate(self.principal, dto(request)).await)
    }

    pub async fn create_record(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.create_record(self.principal, dto(request)).await)
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
        wire(&self.client.update_record(self.principal, dto(request)).await)
    }

    pub async fn delete_record(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.delete_record(self.principal, dto(request)).await)
    }

    pub async fn list_records(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.list_records(self.principal, dto(request)).await)
    }

    pub async fn promote_record(&self, request: Value) -> Value {
        let mut request = request;
        request["space_id"] = json!(self.space_id);
        wire(&self.client.promote_record(self.principal, dto(request)).await)
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
        wire(&self.client.update_profile(self.principal, dto(request)).await)
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
}

impl RawSession {
    async fn open(
        service: Arc<dyn RemoteMemoryService>,
        host_id: &str,
        seed: [u8; 32],
        client_capabilities: &[&str],
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind an ephemeral localhost port");
        let addr = listener.local_addr().expect("listener address");
        let peer = peer_id(&seed);
        let host_manifest = manifest(host_id);
        let host = tokio::spawn(async move {
            let (tcp, address) = tokio::time::timeout(BOUND, listener.accept())
                .await
                .expect("the raw client connects")
                .expect("accept the raw client");
            assert!(address.ip().is_loopback(), "the listener serves localhost only");
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
            let socket: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
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
            (socket, responder)
        });
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
            &format!("c-matrix-raw-{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()),
            &connect_manifest_with(&format!("{host_id}-raw-client"), client_capabilities),
            None,
        )
        .expect("sign the initiator hello");
        socket
            .send(&serde_json::to_vec(&hello).expect("hello bytes"))
            .await
            .expect("hello send");
        let _responder_hello: ConnectHello = serde_json::from_slice(
            &tokio::time::timeout(BOUND, socket.recv())
                .await
                .expect("the responder hello is bounded")
                .expect("responder hello"),
        )
        .expect("decode the responder hello");
        let session: ConnectSession = serde_json::from_slice(
            &tokio::time::timeout(BOUND, socket.recv())
                .await
                .expect("the session snapshot is bounded")
                .expect("session snapshot"),
        )
        .expect("decode the session snapshot");
        let (server_socket, responder) = host.await.expect("the listener task");
        Self {
            socket,
            session_id: session.session_id.to_string(),
            responder,
            server_socket,
            sequence: 0,
        }
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

    pub async fn close(self) {
        self.responder.close();
        let _ = tokio::time::timeout(BOUND, self.socket.close()).await;
        let _ = tokio::time::timeout(BOUND, self.server_socket.close()).await;
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

pub fn recall_request(space_id: &str, selection: Value, max_items: u64) -> Value {
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

pub fn subject_grant_body(
    capability: &str,
    grantee: &str,
    subject: &str,
    key: &str,
) -> Value {
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
        let granted = admin
            .grant(grant_body(capability, peer_id, &key))
            .await;
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
        client,
        principal,
        space_id,
        key,
        revision,
        "fence",
        subject_id,
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
