//! C-111: the REAL `nexus42` binary's `creator memory list|show` runs on the
//! PINNED provider selected in `[memory_provider]` — local and remote.
//!
//! Both arms spawn the actual binary against a hermetic `HOME`. The remote arm
//! serves the independent in-repo `FixtureMemoryProvider` through a real signed
//! `127.0.0.1` Connect session (the same fixture and WS transport the N3 remote
//! proofs use); nothing is mocked, and the local provider's store is asserted
//! absent, so the remote effects cannot come from a local fallback.
//!
//! The remote arm is compiled only with the non-default `memory-remote` feature
//! (`cargo test -p nexus42 --features memory-remote --test creator_memory_remote`).
//! Without it the target still runs the local pin and the feature-off refusal.

use std::path::{Path, PathBuf};
use std::process::Output;

const WORKSPACE: &str = "default";
const SLUG: &str = "c111-pinned-memory";
const KIND: &str = "story_summary";
const BODY: &str = "C111MARKER the pinned provider served this row to the real CLI";
const CREDENTIAL_ID: &str = "cred-c111-remote";
/// The fixture host's Connect identity (the provider side of the session).
#[cfg(feature = "memory-remote")]
const HOST_SEED: [u8; 32] = [0x42; 32];
/// The CLI's Connect identity; the credential file holds exactly this seed.
#[cfg(feature = "memory-remote")]
const CLIENT_SEED: [u8; 32] = [0x31; 32];
#[cfg(feature = "memory-remote")]
const BOUND: std::time::Duration = std::time::Duration::from_secs(60);

#[path = "common/direct.rs"]
mod direct;
#[path = "common/direct_actor.rs"]
mod direct_actor;

use direct_actor::DirectActor;

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn cli_ok(home: &Path, args: &[&str]) -> Output {
    let output = direct_command()
        .args(args)
        .env("HOME", home)
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn nexus42");
    assert!(output.status.success(), "cli {args:?}: {}", stderr(&output));
    output
}

fn direct_command() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("nexus42").expect("nexus42 binary")
}

/// The provider's physical markdown projection files for this workspace.
fn provider_store(home: &Path, creator: &str) -> PathBuf {
    nexus_home_layout::operational_workspace_dir(home, creator, WORKSPACE).join("memory-store")
}

// ---------------------------------------------------------------------------
// C-111 local arm
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_111_creator_cli_uses_pinned_provider_local() {
    let actor = DirectActor::new().await;
    let home = actor.home();
    let creator = actor.creator_id();

    cli_ok(
        home,
        &[
            "creator", "memory", "create", SLUG, "--kind", KIND, "--content", BODY,
        ],
    );

    // The write landed in the PINNED LOCAL PROVIDER's store, not a legacy
    // long-term memory file: the provider owns the record projection.
    let store = provider_store(home, creator);
    let projections = walk_markdown(&store);
    assert_eq!(projections.len(), 1, "one provider record projection: {projections:?}");
    let published = std::fs::read_to_string(store.join(&projections[0])).expect("readable");
    assert!(published.contains(BODY), "the projection carries the body: {published}");
    assert!(published.contains("kind:"), "the projection has kind frontmatter");
    assert!(
        !legacy_memory_dir(home, creator).exists(),
        "the CLI writes no legacy long-term memory file"
    );

    let list = stdout(&cli_ok(home, &["creator", "memory", "list"]));
    assert!(list.contains(SLUG), "list shows the provider row: {list}");
    assert!(list.contains(KIND), "list shows the kind: {list}");
    assert!(list.contains("1 memories"), "list counts one provider row: {list}");

    let show = stdout(&cli_ok(home, &["creator", "memory", "show", SLUG]));
    assert!(show.contains(&format!("slug: {SLUG}")), "show: {show}");
    assert!(show.contains(&format!("memory_kind: {KIND}")), "show: {show}");
    assert!(show.contains("memory_id: "), "show carries the provider id: {show}");
    assert!(show.contains(BODY), "show serves the provider body: {show}");
}

fn legacy_memory_dir(home: &Path, creator: &str) -> PathBuf {
    home.join(".nexus42")
        .join("creators")
        .join(creator)
        .join("memory")
        .join("long-term")
}

fn walk_markdown(root: &Path) -> Vec<String> {
    fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                out.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    collect(root, root, &mut out);
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// C-111 remote arm (the non-default `memory-remote` feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "memory-remote")]
mod remote {
    use super::*;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use async_trait::async_trait;
    use tempfile::TempDir;
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
    use nexus_memory_conformance::{FixtureMemoryProvider, PeerDirectory};
    use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
    use spoke_connect::remote::{
        connect_responder, ConnectResponderOptions, RemoteIdentity, RemoteMemoryService,
        RemoteServePorts, RemoteServePortsComposite, Transport,
    };
    use spoke_operations::spoke_schemas::{
        Finding, HostCapabilityManifest, KnowledgeEntry, Relation, Rule, Scope, TimelineEvent,
    };
    use spoke_operations::{
        FindingPort, HostManifestPort, KnowledgeEntryPort, RelationPort, RuleQueryPort,
        ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult,
    };
    use tokio::net::TcpListener;

    /// The responder's baseline half: `spoke-baseline` is never negotiated, so
    /// every baseline port answers honestly instead of being reached.
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

    fn manifest(host_id: &str) -> HostCapabilityManifest {
        serde_json::from_value(json!({
            "schema_version": 1,
            "host_id": host_id,
            "roles": ["data-store"],
            "capabilities": ["spoke-memory"],
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

    /// Bind the real loopback listener early, so the selection can name it.
    async fn bind() -> (TcpListener, SocketAddr) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind an ephemeral localhost port");
        let addr = listener.local_addr().expect("listener address");
        (listener, addr)
    }

    /// Serve `rounds` sequential CLI opens on the already-bound listener.
    ///
    /// The accept loop starts only after the non-dialing CLI setup, so no
    /// accept deadline can expire before the first real dial.
    fn serve(
        listener: TcpListener,
        memory_service: Arc<dyn RemoteMemoryService>,
        host_manifest: HostCapabilityManifest,
        client_peer: String,
        client_pubkey: [u8; 32],
        rounds: usize,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut responders = Vec::with_capacity(rounds);
            for _ in 0..rounds {
                let (tcp, peer) = tokio::time::timeout(BOUND, listener.accept())
                    .await
                    .expect("the CLI connects to the listener")
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
                let responder = connect_responder(ConnectResponderOptions {
                    transport,
                    identity: RemoteIdentity { seed: HOST_SEED },
                    manifest: host_manifest.clone(),
                    allowlist: vec![client_peer.clone()],
                    peer_keys: HashMap::from([(client_peer.clone(), client_pubkey)]),
                    ports: Some(ports),
                    invoke_timeout_ms: Some(5_000),
                })
                .await;
                // `connect_responder` spawns its own runner; keeping the
                // responder alive owns the session until this task ends.
                responders.push(responder);
            }
        })
    }

    /// Write the operator-provisioned credential for `seed` at mode 0600.
    fn write_credential(home: &Path, seed: &[u8; 32]) {
        let dir = home.join(".nexus42").join("memory-credentials");
        std::fs::create_dir_all(&dir).expect("credential dir");
        let path = dir.join(format!("{CREDENTIAL_ID}.json"));
        std::fs::write(
            &path,
            format!(
                "{{\"identity_seed_hex\":\"{}\",\"capability_token\":null}}",
                hex(seed)
            ),
        )
        .expect("write the credential");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("credential mode 0600");
        }
    }

    /// Run one CLI call without blocking the runtime that serves the fixture.
    async fn cli(home: PathBuf, args: Vec<String>) -> Output {
        tokio::task::spawn_blocking(move || {
            let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
            direct_command()
                .args(&borrowed)
                .env("HOME", &home)
                .env("RUST_LOG", "off")
                .output()
                .expect("spawn nexus42")
        })
        .await
        .expect("cli task joins")
    }

    async fn cli_ok_remote(home: &Path, args: &[&str]) -> Output {
        let output = cli(home.to_path_buf(), args.iter().map(|arg| (*arg).to_string()).collect())
            .await;
        assert!(output.status.success(), "cli {args:?}: {}", stderr(&output));
        output
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn c_111_creator_cli_uses_pinned_provider_remote() {
        // The durable independent fixture. Its admission table maps the dialing
        // credential's peer id to the operator principal holding `admin`.
        let fixture_home = TempDir::new().expect("fixture home");
        let provider = Arc::new(
            FixtureMemoryProvider::open(&fixture_home.path().join("memory.sqlite"))
                .await
                .expect("open the durable fixture"),
        );
        let space_id = provider.identity().space_id.clone();
        let authority_host_id = provider.identity().authority_host_id.clone();
        let client_peer = peer_id(&CLIENT_SEED);
        let principal_id = format!("prn_{client_peer}");
        provider
            .provision_operator_admin(&principal_id)
            .await
            .expect("operator provisions the admin grant");
        let directory =
            Arc::new(PeerDirectory::new().with_peer(client_peer.clone(), principal_id));

        let actor = DirectActor::new().await;
        let home = actor.home().to_path_buf();
        let creator = actor.creator_id().to_string();

        write_credential(&home, &CLIENT_SEED);

        let (listener, addr) = bind().await;
        let endpoint = format!("ws://{addr}/memory");


        cli_ok_remote(
            &home,
            &[
                "creator",
                "memory",
                "provider",
                "use",
                "remote",
                "--space-id",
                &space_id,
                "--credential-id",
                CREDENTIAL_ID,
                "--connect-endpoint",
                &endpoint,
                "--remote-pubkey-hex",
                &hex(&pubkey(&HOST_SEED)),
                "--allowlist",
                &peer_id(&HOST_SEED),
            ],
        )
        .await;

        // create + list + show each open one real signed session; the accept
        // loop starts here, with no deadline over the non-dialing setup.
        let server = serve(
            listener,
            provider.server(directory.admission()),
            manifest(&authority_host_id),
            client_peer.clone(),
            pubkey(&CLIENT_SEED),
            3,
        );

        // A local-only trap: a legacy bearer file no pinned remote read may
        // surface.
        let legacy = home
            .join(".nexus42")
            .join("creators")
            .join(&creator)
            .join("memory")
            .join("long-term")
            .join("legacy-trap.md");
        std::fs::create_dir_all(legacy.parent().expect("legacy dir")).expect("legacy dir");
        std::fs::write(&legacy, "---\nkind: custom\n---\nLEGACYTRAP\n").expect("seed legacy");

        cli_ok_remote(
            &home,
            &[
                "creator", "memory", "create", SLUG, "--kind", KIND, "--content", BODY,
            ],
        )
        .await;

        let list = stdout(&cli_ok_remote(&home, &["creator", "memory", "list"]).await);
        assert!(list.contains(SLUG), "the remote provider row is listed: {list}");
        assert!(list.contains(KIND), "the remote row keeps its kind: {list}");
        assert!(!list.contains("LEGACYTRAP"), "no legacy local row leaks: {list}");
        assert!(!list.contains("legacy-trap"), "no legacy slug leaks: {list}");

        let show = stdout(&cli_ok_remote(&home, &["creator", "memory", "show", SLUG]).await);
        assert!(show.contains(&format!("slug: {SLUG}")), "show: {show}");
        assert!(show.contains(&format!("memory_kind: {KIND}")), "show: {show}");
        assert!(show.contains(BODY), "the remote body is served: {show}");
        assert!(!show.contains("LEGACYTRAP"), "no local fallback body: {show}");

        // Pin proof: the remote arm never constructed the local provider.
        assert!(
            !provider_store(&home, &creator).exists(),
            "a remote-pinned CLI never opens the local provider's store"
        );
        assert!(legacy.exists(), "the legacy trap file itself is untouched");

        server.await.expect("the fixture served every CLI open");
    }
}

// ---------------------------------------------------------------------------
// Feature-off preservation: the remote selection still refuses, never falls back
// ---------------------------------------------------------------------------

#[cfg(not(feature = "memory-remote"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c_111_remote_selection_without_client_refuses_and_never_reads_local() {
    let actor = DirectActor::new().await;
    let home = actor.home();
    let creator = actor.creator_id();

    // A real local provider row exists first.
    cli_ok(
        home,
        &[
            "creator", "memory", "create", SLUG, "--kind", KIND, "--content", BODY,
        ],
    );
    assert_eq!(walk_markdown(&provider_store(home, creator)).len(), 1);

    // The remote selection, written through the real CLI, has no client in this
    // graph: the pinned provider refuses instead of serving the local store.
    cli_ok(
        home,
        &[
            "creator",
            "memory",
            "provider",
            "use",
            "remote",
            "--space-id",
            "space-alpha",
            "--credential-id",
            CREDENTIAL_ID,
            "--connect-endpoint",
            "ws://127.0.0.1:9",
            "--remote-pubkey-hex",
            &"01".repeat(32),
            "--allowlist",
            "12D3KooWPeer",
        ],
    );
    let refused = direct_command()
        .args(["creator", "memory", "list"])
        .env("HOME", home)
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn nexus42");
    assert!(!refused.status.success(), "the remote pin refuses");
    let err = stderr(&refused);
    assert!(err.contains("memory_remote_disabled"), "refusal: {err}");
    assert!(!stdout(&refused).contains(BODY), "no local fallback row");
    let shown = direct_command()
        .args(["creator", "memory", "show", SLUG])
        .env("HOME", home)
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn nexus42");
    assert!(!shown.status.success(), "the remote pin refuses show too");
    assert!(
        !stdout(&shown).contains(BODY),
        "no local fallback body on show"
    );
}
