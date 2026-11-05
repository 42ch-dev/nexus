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
use serde_json::json;
use tempfile::TempDir;
use tokio::net::TcpListener;

use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreResult, CoreService, MemoryProviderSelection,
    RemoteMemoryProviderSelection,
};
use nexus_memory_conformance::{FixtureMemoryProvider, PeerDirectory};
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_responder, ConnectResponder, ConnectResponderOptions, RemoteIdentity, RemoteServePorts,
    RemoteServePortsComposite, Transport,
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
/// The dialing client's Connect identity. Its credential file holds exactly
/// this seed, so the public peer id the signed hello presents is derivable.
const CLIENT_SEED: [u8; 32] = [0x31; 32];
/// The one capability both ends negotiate: the fixture serves no other family.
const MEMORY_CAPABILITIES: [&str; 1] = ["spoke-memory"];
const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORKSPACE: &str = "default";
const CREDENTIAL_ID: &str = "cred-remote-open";
const BOUND: Duration = Duration::from_secs(5);

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
/// the signed responder's transport. Dial the returned address, then await the
/// handle for the responder that served the session.
async fn start_listener(
    provider: &Arc<FixtureMemoryProvider>,
    directory: &Arc<PeerDirectory>,
    host_manifest: HostCapabilityManifest,
    client_peer: String,
    client_pubkey: [u8; 32],
) -> (SocketAddr, tokio::task::JoinHandle<Arc<ConnectResponder>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind an ephemeral localhost port");
    let addr = listener.local_addr().expect("listener address");
    let provider = Arc::clone(provider);
    let directory = Arc::clone(directory);
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
                .with_memory(provider.server(directory.admission())),
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
        let dir = self
            .user_home
            .join(".nexus42")
            .join("memory-credentials");
        fs::create_dir_all(&dir).expect("credential dir");
        let path = dir.join(format!("{CREDENTIAL_ID}.json"));
        fs::write(
            &path,
            format!("{{\"identity_seed_hex\":\"{}\",\"capability_token\":null}}", hex(seed)),
        )
        .expect("write the credential");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .expect("credential mode 0600");
        }
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
    MemoryProviderSelection::Remote(RemoteMemoryProviderSelection {
        space_id: space_id.to_string(),
        credential_id: CREDENTIAL_ID.to_string(),
        connect_endpoint: format!("ws://{endpoint}"),
        remote_pubkey_hex: hex(host_pubkey),
        allowlist: vec![derive_peer_id_from_ed25519_pubkey(host_pubkey)],
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
    tokio::time::timeout(BOUND, task)
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
        &provider,
        &directory,
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
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
        &provider,
        &directory,
        manifest_without_memory(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
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
        &provider,
        &directory,
        manifest("fixture-not-the-space-authority"),
        client_peer.clone(),
        client_pubkey,
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
        &provider,
        &directory,
        manifest(&authority_host_id),
        client_peer.clone(),
        client_pubkey,
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
