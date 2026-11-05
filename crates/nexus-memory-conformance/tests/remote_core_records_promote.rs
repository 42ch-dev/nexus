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
//! - The provider's own stale-revision CAS is reached: a caller that read a
//!   revision another writer has since changed keeps the static
//!   `version_mismatch` classification and writes nothing, and the same call
//!   with the current revision succeeds.
//! - Refusals reach the caller as the static classified `MemoryProviderRefused`
//!   carrier: the provider's own message text and any request content never
//!   travel with it.
//!
//! `MemoryProviderBinding` is crate-private, so this crate-external test cannot
//! read the pinned `(space, authority, client identity)` tuple back. The
//! provider's own rows are the observable substitute: their `space_id`,
//! `provenance.captured_by` (the opaque peer-derived principal) and revisions are
//! written from that pinned tuple by the product code.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
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
    MemoryRecordsUpdateRequest, Principal as MemoryPrincipal, Relation, Rule, Scope, TimelineEvent,
};
use spoke_operations::{
    FindingPort, HostManifestPort, KnowledgeEntryPort, MemoryPort, RelationPort, RuleQueryPort,
    ScopeQueryPort, SpokeRejectCode, SpokeResult,
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
            directory: Arc::new(PeerDirectory::new().with_peer(client_peer, principal_id)),
            space_id,
        }
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
        let (addr, task) = self.serve().await;
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
    /// expose on this task — `memory.recall` — and acts as the second writer that
    /// changes a record between a caller's read and its revision-checked write.
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
            local_manifest: manifest("r3-records-client"),
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
        ("character_memory_fragments", "the legacy Character fragments"),
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
        tombstoned: row
            .get::<Option<String>, _>("tombstone_json")
            .is_some(),
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
        tombstoned: row
            .get::<Option<String>, _>("tombstone_json")
            .is_some(),
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
) -> (&str, &str, MemoryProviderRefusal, Option<&str>, Option<&str>) {
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

    // CREATE: the standard `memory.records.create` in the pinned space, with the
    // authored identity the Creator family reads back.
    assert!(
        core.create_creator_memory(&principal, SLUG, "story_summary", BODY)
            .await
            .expect("create the Creator memory")
    );
    // A taken slug is the retained `Ok(false)` answer and writes nothing.
    assert!(
        !core.create_creator_memory(&principal, SLUG, "story_summary", BODY)
            .await
            .expect("the duplicate create is refused"),
        "the slug is already taken"
    );

    let pool = provider.pool().await;
    let rows = provider_records(&pool, &subject_id).await;
    assert_eq!(rows.len(), 1, "one provider record, one create: {rows:?}");
    let created = &rows[0];
    assert_eq!(created.record_id, rows[0].record_id);
    assert_eq!(created.kind, "nexus.story_summary");
    assert_eq!(created.visibility, "shared");
    assert_eq!(created.context_id, None);
    assert_eq!(created.revision, 1);
    assert!(!created.tombstoned);
    assert_eq!(
        provider_receipts(&pool, "memory.records.create").await,
        1,
        "the duplicate never committed a second receipt"
    );
    let content = provider_content(&pool, &created.record_id).await;
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
    let provenance = provider_provenance(&pool, &created.record_id).await;
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
                    &pool,
                    &format!("SELECT COUNT(*) FROM records WHERE {column} LIKE '%{secret}%'"),
                )
                .await,
                0,
                "the credential material never reaches the provider's `{column}`"
            );
        }
    }

    // LIST (`memory.records.list`): the kind-filtered authored read, served from
    // the pinned provider space.
    let listed = core
        .list_creator_memories(&principal)
        .await
        .expect("list the Creator memories");
    assert_eq!(listed.len(), 1);
    let listed = serde_json::to_value(&listed[0]).expect("record json");
    assert_eq!(listed["record_id"], json!(created.record_id));
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
        .get_creator_memory(&principal, SLUG)
        .await
        .expect("get the Creator memory")
        .expect("the record exists on the provider");
    assert_eq!(
        serde_json::to_value(&got).expect("record json")["record_id"],
        json!(created.record_id)
    );

    // UPDATE on the revision the caller read (`memory.records.update`).
    let expected = MemoryRevision {
        record_id: created.record_id.clone(),
        revision: 1,
    };
    assert!(
        core.update_creator_memory(&principal, SLUG, EDITED_BODY, Some(&expected))
            .await
            .expect("update the record")
    );
    let updated = provider_record(&pool, &created.record_id)
        .await
        .expect("the record row");
    assert_eq!(updated.revision, 2, "the provider bumped the revision once");
    assert_eq!(
        provider_content(&pool, &created.record_id).await["text"],
        json!(EDITED_BODY)
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.update").await,
        1
    );

    // A caller whose read is stale is refused before the write, in the retained
    // `version_mismatch` shape, and the provider is untouched. (The provider's
    // own stale-revision CAS is reached in the promotion target below.)
    let stale = core
        .update_creator_memory(&principal, SLUG, BODY, Some(&expected))
        .await
        .expect_err("a stale read is refused");
    assert!(
        matches!(&stale, CoreError::ActorConflict { code, .. } if code == "version_mismatch"),
        "got {stale:?}"
    );
    let unchanged = provider_record(&pool, &created.record_id)
        .await
        .expect("the record row");
    assert_eq!(unchanged.revision, 2);
    assert_eq!(
        provider_receipts(&pool, "memory.records.update").await,
        1,
        "a refused update commits nothing"
    );

    // The edited body is what the read now serves.
    let edited = core
        .get_creator_memory(&principal, SLUG)
        .await
        .expect("get the edited record")
        .expect("the record exists");
    let edited = serde_json::to_value(&edited).expect("record json");
    assert_eq!(edited["revision"], json!(2));
    assert_eq!(edited["content"]["text"], json!(EDITED_BODY));

    // DELETE by slug (`memory.records.delete`) tombstones the provider record.
    assert!(
        core.delete_creator_memory(&principal, SLUG)
            .await
            .expect("delete the record")
    );
    let tombstone = provider_record(&pool, &created.record_id)
        .await
        .expect("the record row");
    assert!(tombstone.tombstoned, "the provider tombstoned the record");
    assert_eq!(tombstone.revision, 3);
    assert_eq!(
        provider_receipts(&pool, "memory.records.delete").await,
        1
    );
    assert!(
        core.get_creator_memory(&principal, SLUG)
            .await
            .expect("get the deleted record")
            .is_none(),
        "a tombstoned record is absent from the authored slug read"
    );
    assert!(
        core.list_creator_memories(&principal)
            .await
            .expect("list after delete")
            .is_empty(),
        "a tombstoned record leaves the listing"
    );
    assert!(
        !core
            .delete_creator_memory(&principal, SLUG)
            .await
            .expect("delete an absent record"),
        "the absent delete is the retained false"
    );
    let create_receipts = provider_receipts(&pool, "memory.records.create").await;
    let update_receipts = provider_receipts(&pool, "memory.records.update").await;
    let delete_receipts = provider_receipts(&pool, "memory.records.delete").await;
    pool.close().await;

    // A read-only core serves the reads and refuses every record write before it
    // can reach the provider.
    let (read_only, ro_responder) = provider.open_core_as(&env, CoreAccess::ReadOnly).await;
    let ro_principal = read_only
        .active_principal()
        .await
        .expect("active principal");
    assert!(
        read_only
            .list_creator_memories(&ro_principal)
            .await
            .expect("a read-only core serves the record read")
            .is_empty()
    );
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
        provider_records(&pool, &subject_id).await.len(),
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
// Character fragment reads and promotion
// ---------------------------------------------------------------------------

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

    // Two binding-local captures, drained by the real review pipeline: each one
    // becomes a binding-local `fact` fragment on the provider.
    for pending_id in ["pend_r3_a", "pend_r3_b"] {
        capture(&core, &principal, &character_id, &binding_id, pending_id)
            .await
            .expect("capture into the binding scope");
    }
    let reviewed = core
        .review_character_memory(
            &principal,
            character_id.clone(),
            dto::<ReviewCharacterMemoryRequest>(json!({ "binding_id": binding_id })),
        )
        .await
        .expect("review drains the binding scope");
    assert_eq!(reviewed.fragmented, 2, "both candidates became fragments");
    assert_eq!(reviewed.promoted, 0);
    assert_eq!(reviewed.dropped, 0);

    let pool = provider.pool().await;
    let records = provider_records(&pool, &subject_id).await;
    assert_eq!(records.len(), 2, "two durable fragments: {records:?}");
    for record in &records {
        assert_eq!(record.kind, "fact");
        assert_eq!(record.visibility, "context");
        assert_eq!(
            record.context_id.as_deref(),
            Some(binding_id.as_str()),
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

    // LIST of the Character fragments (`memory.records.list`, kind `fact`), and
    // the by-id GET the promotion below reads twice.
    let fragments = core
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
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
            fragment.binding_id.as_deref().map(|id| id.as_str()),
            Some(binding_id.as_str())
        );
        assert_eq!(fragment.revision, 0, "the retained DTO 0-based revision");
        assert!(!fragment.summary.as_str().is_empty());
    }
    let first = fragment_ids[0].clone();
    let second = fragment_ids[1].clone();

    // D18: the committed review published each record as a live dependency.
    let nexus = core_db(&env).await;
    let facts = object_facts(&nexus, &binding_id).await;
    for record_id in [&first, &second] {
        assert!(
            facts.contains(&("record".to_string(), record_id.clone(), "live".to_string())),
            "the review published this record's dependency: {facts:?}"
        );
    }
    nexus.close().await;

    // PROMOTE the first fragment to the shared Character scope.
    let promoted = core
        .promote_character_fragment(&principal, character_id.clone(), first.clone(), 0)
        .await
        .expect("promote the fragment");
    assert!(
        promoted.fragment.binding_id.is_none(),
        "the promoted fragment carries no binding placement"
    );
    assert_eq!(promoted.fragment.revision, 1);

    let pool = provider.pool().await;
    let promoted_row = provider_record(&pool, &first).await.expect("the row");
    assert_eq!(promoted_row.visibility, "shared");
    assert_eq!(promoted_row.context_id, None);
    assert_eq!(promoted_row.revision, 2, "the promotion bumped the revision");
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        1
    );
    let untouched = provider_record(&pool, &second).await.expect("the row");
    assert_eq!(
        (untouched.visibility.as_str(), untouched.context_id.clone()),
        ("context", Some(binding_id.clone())),
        "the other fragment keeps its placement"
    );
    pool.close().await;

    // D18: the promotion released the record's binding dependency.
    let nexus = core_db(&env).await;
    let facts = object_facts(&nexus, &binding_id).await;
    assert!(
        facts.contains(&("record".to_string(), first.clone(), "released".to_string())),
        "the promotion receipt released the record's dependency: {facts:?}"
    );
    nexus.close().await;

    // Promoting an already shared fragment is the retained stable conflict.
    let already = core
        .promote_character_fragment(&principal, character_id.clone(), first.clone(), 1)
        .await
        .expect_err("an already shared fragment is a stable conflict");
    assert!(
        matches!(&already, CoreError::ActorConflict { code, .. } if code == "character_fragment_already_shared"),
        "got {already:?}"
    );

    // A second writer changes the other fragment between the caller's read and
    // its revision-checked write: the caller's DTO revision 0 is now stale. It
    // edits the record's body and keeps the record's own authored block, which
    // is what any client editing a stored fragment writes.
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = provider.memory_principal();
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
        provider_record(&pool, &second).await.expect("the row").revision,
        2,
        "the second writer bumped the revision"
    );
    pool.close().await;

    // The promotion at the stale revision reaches the provider's own CAS and
    // keeps the static safe classification — no peer message travels.
    let conflict = core
        .promote_character_fragment(&principal, character_id.clone(), second.clone(), 0)
        .await
        .expect_err("the stale revision is refused by the provider");
    let (code, retry_class, family, wire_code, conflict_code) = classified_refusal(&conflict);
    assert_eq!(code, "REVISION_CONFLICT");
    assert_eq!(retry_class, "terminal");
    assert_eq!(family, MemoryProviderRefusal::Conflict);
    assert_eq!(wire_code, None);
    assert_eq!(conflict_code, Some("version_mismatch"));
    let shown = format!("{conflict} | {conflict:?}");
    assert!(
        !shown.contains("stored revision"),
        "the provider's own CAS message never travels: {shown}"
    );
    let pool = provider.pool().await;
    let refused_row = provider_record(&pool, &second).await.expect("the row");
    assert_eq!(
        (refused_row.visibility.as_str(), refused_row.revision),
        ("context", 2),
        "the refused promotion wrote nothing"
    );
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        1,
        "the refused promotion committed no receipt"
    );
    pool.close().await;

    // The same call with the current DTO revision succeeds.
    let retried = core
        .promote_character_fragment(&principal, character_id.clone(), second.clone(), 1)
        .await
        .expect("promote the current revision");
    assert!(retried.fragment.binding_id.is_none());
    let pool = provider.pool().await;
    let retried_row = provider_record(&pool, &second).await.expect("the row");
    assert_eq!(retried_row.visibility, "shared");
    assert_eq!(retried_row.context_id, None);
    assert_eq!(retried_row.revision, 3);
    assert_eq!(
        provider_receipts(&pool, "memory.records.promote").await,
        2
    );
    pool.close().await;

    // The per-Character activity lease is this branch's outer admission: an
    // archived Character is refused there, before any provider read or write.
    core.transition_character(
        &principal,
        dto::<CoreCharacterTransitionRequest>(json!({
            "character_id": character_id,
            "expected_revision": character_revision,
            "target_status": "archived",
        })),
    )
    .await
    .expect("archive the Character");
    let inactive = core
        .promote_character_fragment(&principal, character_id.clone(), first.clone(), 1)
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
        provider_record(&pool, &first).await.expect("the row").visibility,
        "shared",
        "the lease refusal left the promoted record alone"
    );
    pool.close().await;

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

    assert!(
        core.create_creator_memory(&principal, SLUG, "story_summary", BODY)
            .await
            .expect("create the Creator memory")
    );
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
    // SDK's own signed session against the same durable provider: the pinned
    // space is the one the product's reads and writes above actually used.
    let (adapter, sdk_responder) = provider.sdk_session().await;
    let memory_principal = provider.memory_principal();

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
    let recalled_ids: Vec<&str> = recalled["records"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|record| record["record_id"].as_str())
        .collect();
    assert!(
        recalled_ids.contains(&record_id.as_str()),
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
    let (code, retry_class, family, wire_code, conflict_code) = classified_refusal(&refused);
    assert_eq!((code, retry_class), ("INVALID_INPUT", "terminal"));
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
    assert!(
        core.get_creator_memory(&principal, "r3-never-written")
            .await
            .expect("the absent read is answered")
            .is_none()
    );

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
