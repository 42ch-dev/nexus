//! C-111: the REAL `nexus42` binary's `creator memory list|show` reads the
//! PINNED provider selected in `[memory_provider]` — local and remote.
//!
//! Both arms spawn the actual binary against a hermetic `HOME`. The remote arm
//! serves the independent in-repo `FixtureMemoryProvider` through a real signed
//! `127.0.0.1` Connect session. A row that exists ONLY in the remote fixture is
//! seeded through the fixture's own service before the CLI runs, a test
//! decorator counts the provider's record list/get operations, and a
//! conflicting local artifact is present: the CLI must return the remote row's
//! authored identity and body, not the local one.
//!
//! The remote arm is compiled only with the non-default `memory-remote` feature
//! (`cargo test -p nexus42 --features memory-remote --test creator_memory_remote`).
//! Without it the target still runs the local pin and the feature-off refusal.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const WORKSPACE: &str = "default";
const SLUG: &str = "c111-pinned-memory";
const KIND: &str = "story_summary";
const BODY: &str = "C111MARKER the pinned provider served this row to the real CLI";
const CREDENTIAL_ID: &str = "cred-c111-remote";
/// Bound on one spawned CLI child, covering startup, dial and output drain.
const CLI_BOUND: Duration = Duration::from_secs(180);
/// Bound on observing a killed/errored child's exit status (never a bare wait).
const SETTLE_BOUND: Duration = Duration::from_secs(5);
/// The fixture host's Connect identity (the provider side of the session).
#[cfg(feature = "memory-remote")]
const HOST_SEED: [u8; 32] = [0x42; 32];
/// The CLI's Connect identity; the credential file holds exactly this seed.
#[cfg(feature = "memory-remote")]
const CLIENT_SEED: [u8; 32] = [0x31; 32];

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

/// Spawn the real binary and bound the WHOLE child/output lifecycle.
///
/// Child output goes to regular temp files (not inherited pipes), so a
/// descendant that keeps an inherited pipe open can never block the test after
/// the direct child exits or is killed. On timeout the direct child is killed
/// and reaped before the failure is returned; a normal exit is already reaped
/// by `try_wait`.
fn run_cli(home: &Path, args: &[&str]) -> Result<Output, String> {
    run_cli_with(home, args, CLI_BOUND)
}

fn run_cli_with(home: &Path, args: &[&str], bound: Duration) -> Result<Output, String> {
    let mut out_file = tempfile::tempfile().map_err(|error| format!("stdout temp: {error}"))?;
    let mut err_file = tempfile::tempfile().map_err(|error| format!("stderr temp: {error}"))?;
    let child_out = out_file
        .try_clone()
        .map_err(|error| format!("stdout clone: {error}"))?;
    let child_err = err_file
        .try_clone()
        .map_err(|error| format!("stderr clone: {error}"))?;

    let mut child = Command::new(env!("CARGO_BIN_EXE_nexus42"))
        .args(args)
        .env("HOME", home)
        .env("RUST_LOG", "off")
        .stdin(Stdio::null())
        .stdout(Stdio::from(child_out))
        .stderr(Stdio::from(child_err))
        .spawn()
        .map_err(|error| format!("spawn nexus42 {args:?}: {error}"))?;

    let deadline = Instant::now() + bound;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                return Err(settle_failed_child(
                    &mut child,
                    format!("wait nexus42 {args:?}: {error}"),
                ));
            }
        }
        if Instant::now() >= deadline {
            return Err(settle_failed_child(
                &mut child,
                format!("nexus42 {args:?} exceeded {bound:?}"),
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };

    let mut stdout = Vec::new();
    out_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| format!("seek stdout: {error}"))?;
    out_file
        .read_to_end(&mut stdout)
        .map_err(|error| format!("read stdout: {error}"))?;
    let mut stderr = Vec::new();
    err_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| format!("seek stderr: {error}"))?;
    err_file
        .read_to_end(&mut stderr)
        .map_err(|error| format!("read stderr: {error}"))?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Settle an owned child after a wait/timeout failure without an unbounded
/// `wait()`. `kill` and every later `try_wait` are checked; the message never
/// claims "reaped" unless an exit status was actually observed, and an OS
/// failure is preserved verbatim.
fn settle_failed_child(child: &mut std::process::Child, context: String) -> String {
    let kill_error = child.kill().err().map(|error| error.to_string());
    let deadline = Instant::now() + SETTLE_BOUND;
    let mut exit_seen = false;
    let mut wait_error = None;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                exit_seen = true;
                break;
            }
            Ok(None) => {}
            Err(error) => {
                wait_error = Some(error.to_string());
                break;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let outcome = match (&kill_error, &wait_error, exit_seen) {
        (None, None, true) => "child killed and reaped".to_string(),
        (Some(kill), None, true) => format!(
            "kill failed ({kill}); the child had already exited and its status was reaped"
        ),
        (Some(kill), None, false) => format!(
            "kill failed ({kill}); no exit observed within {SETTLE_BOUND:?}; child NOT confirmed reaped"
        ),
        (None, None, false) => format!(
            "kill issued, but no exit observed within {SETTLE_BOUND:?}; child NOT confirmed reaped"
        ),
        (None, Some(wait), _) => {
            format!("wait failed ({wait}); child reaped state unknown")
        }
        (Some(kill), Some(wait), _) => format!(
            "kill failed ({kill}) and wait failed ({wait}); child NOT confirmed reaped"
        ),
    };
    format!("{context}; {outcome}")
}

fn cli_ok(home: &Path, args: &[&str]) -> Output {
    let output = run_cli(home, args).unwrap_or_else(|error| panic!("{error}"));
    assert!(output.status.success(), "cli {args:?}: {}", stderr(&output));
    output
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use ed25519_dalek::SigningKey;
    use serde_json::{json, Value};
    use tempfile::TempDir;

    use nexus_core::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
    use nexus_core::memory_provider::legacy;
    use nexus_memory_conformance::{FixtureMemoryProvider, PeerDirectory};
    use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
    use spoke_connect::remote::{
        connect_responder, ConnectResponder, ConnectResponderOptions, ConnectResponderState,
        RemoteIdentity, RemoteMemoryService, RemoteServePorts, RemoteServePortsComposite,
        Transport,
    };
    use spoke_operations::spoke_schemas::{
        Finding, HostCapabilityManifest, KnowledgeEntry, Relation, Rule, Scope, TimelineEvent,
    };
    use spoke_operations::{
        FindingPort, HostManifestPort, KnowledgeEntryPort, RelationPort, RuleQueryPort,
        ScopeQueryPort, SpokeReject, SpokeRejectCode, SpokeResult,
    };
    use tokio::net::TcpListener;

    const BOUND: Duration = Duration::from_secs(60);

    macro_rules! ensure {
        ($condition:expr, $($arg:tt)*) => {
            if !$condition {
                return Err(format!($($arg)*));
            }
        };
    }
    /// A fixture-only row seeded before the CLI runs; it exists in no local
    /// store, so the CLI can only return it by reading the remote provider.
    const SEED_SLUG: &str = "c111-remote-seeded";
    const SEED_BODY: &str = "C111REMOTESEED the fixture-only row only the remote provider holds";
    const SEED_MEMORY_ID: &str = "mem_c111remoteseed";
    /// A conflicting local artifact: if the CLI fell back to local content it
    /// would surface this body under this slug.
    const TRAP_SLUG: &str = "c111-local-trap";
    const TRAP_BODY: &str = "LEGACYTRAP a local artifact no pinned remote read may serve";

    /// Test decorator over the REAL fixture service: forwards every operation
    /// unchanged and counts the record list/get boundary the CLI must cross.
    struct CountingService {
        inner: Arc<dyn RemoteMemoryService>,
        lists: AtomicUsize,
        gets: AtomicUsize,
    }

    impl CountingService {
        fn new(inner: Arc<dyn RemoteMemoryService>) -> Self {
            Self {
                inner,
                lists: AtomicUsize::new(0),
                gets: AtomicUsize::new(0),
            }
        }

        fn lists(&self) -> usize {
            self.lists.load(Ordering::SeqCst)
        }

        fn gets(&self) -> usize {
            self.gets.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl RemoteMemoryService for CountingService {
        async fn invoke(
            &self,
            peer_id: &str,
            op: &str,
            payload: Value,
        ) -> SpokeResult<Value> {
            match op {
                "memory.records.list" => {
                    self.lists.fetch_add(1, Ordering::SeqCst);
                }
                "memory.records.get" => {
                    self.gets.fetch_add(1, Ordering::SeqCst);
                }
                _ => {}
            }
            self.inner.invoke(peer_id, op, payload).await
        }
    }

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
    /// Each created responder is appended to the shared list immediately, so
    /// cleanup can close every session that was actually created, on success
    /// and on any failed assertion/CLI error/timeout alike.
    fn serve(
        listener: TcpListener,
        memory_service: Arc<dyn RemoteMemoryService>,
        host_manifest: HostCapabilityManifest,
        client_peer: String,
        client_pubkey: [u8; 32],
        rounds: usize,
        accept_bound: Duration,
        responders: Arc<std::sync::Mutex<Vec<Arc<ConnectResponder>>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            for _ in 0..rounds {
                let accepted = tokio::time::timeout(accept_bound, listener.accept()).await;
                let Ok(Ok((tcp, peer))) = accepted else {
                    return;
                };
                if !peer.ip().is_loopback() {
                    return;
                }
                let upgraded = tokio::time::timeout(
                    BOUND,
                    tokio_tungstenite::accept_async_with_config(
                        tcp,
                        Some(ws_config(DEFAULT_MAX_ENVELOPE_BYTES)),
                    ),
                )
                .await;
                let Ok(Ok(ws)) = upgraded else {
                    return;
                };
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
                responders
                    .lock()
                    .expect("responder list")
                    .push(responder);
            }
        })
    }

    /// Close every responder through the SDK's supported `close()` and wait,
    /// bounded, for its public state to report `Closed`. The SDK's internal
    /// runner task is not publicly joinable (disclosed in the report).
    async fn close_responders(
        responders: &[Arc<ConnectResponder>],
    ) -> Result<(), String> {
        for responder in responders {
            responder.close();
        }
        for responder in responders {
            let deadline = Instant::now() + Duration::from_secs(10);
            while responder.state() != ConnectResponderState::Closed {
                if Instant::now() >= deadline {
                    return Err("a responder did not reach Closed after close()".to_string());
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
        Ok(())
    }

    /// Abort and bounded-join the accept task (when still owned), then close
    /// every responder actually created. Runs on success and on failure so no
    /// owned fixture resource is detached or left open.
    async fn teardown(
        accept: &mut Option<tokio::task::JoinHandle<()>>,
        responders: &Arc<std::sync::Mutex<Vec<Arc<ConnectResponder>>>>,
    ) -> Result<(), String> {
        let mut failures: Vec<String> = Vec::new();
        if let Some(task) = accept.take() {
            task.abort();
            match tokio::time::timeout(BOUND, task).await {
                Ok(Ok(())) => {}
                // Expected after our own abort: a cancelled join is not a
                // cleanup failure.
                Ok(Err(error)) if error.is_cancelled() => {}
                Ok(Err(error)) => failures.push(format!("accept join: {error}")),
                Err(_) => failures.push("accept task did not settle after abort".to_string()),
            }
        }
        let list: Vec<Arc<ConnectResponder>> =
            responders.lock().expect("responder list").clone();
        if let Err(error) = close_responders(&list).await {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
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
    async fn cli(home: PathBuf, args: Vec<String>) -> Result<Output, String> {
        tokio::task::spawn_blocking(move || {
            let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
            run_cli(&home, &borrowed)
        })
        .await
        .map_err(|error| format!("cli task join: {error}"))?
    }

    async fn cli_ok_remote(home: &Path, args: &[&str]) -> Result<Output, String> {
        let output = cli(
            home.to_path_buf(),
            args.iter().map(|arg| (*arg).to_string()).collect(),
        )
        .await?;
        if !output.status.success() {
            return Err(format!("cli {args:?}: {}", stderr(&output)));
        }
        Ok(output)
    }

    /// Seed a row that exists ONLY in the remote fixture, through the fixture's
    /// own service (the same operation mapping the signed session uses).
    async fn seed_fixture_row(
        service: &Arc<dyn RemoteMemoryService>,
        client_peer: &str,
        space_id: &str,
        creator: &str,
    ) -> String {
        let subject = legacy::creator_subject_id(creator);
        let ensured = service
            .invoke(
                client_peer,
                "memory.subjects.ensure",
                json!({
                    "space_id": space_id,
                    "subject_id": subject,
                    "kind": { "ns": legacy::CREATOR_KIND_NS, "id": creator },
                }),
            )
            .await;
        assert!(
            matches!(ensured, SpokeResult::Ok(_)),
            "the fixture subject is ensured: {ensured:?}"
        );
        let created = service
            .invoke(
                client_peer,
                "memory.records.create",
                json!({
                    "space_id": space_id,
                    "subject_id": subject,
                    "kind": "nexus.story_summary",
                    "content": {
                        "text": SEED_BODY,
                        "structured": { "nexus_legacy": {
                            "slug": SEED_SLUG,
                            "memory_id": SEED_MEMORY_ID,
                            "memory_kind": "story_summary",
                            "nexus_memory_version": 1,
                            "source_session_ids": ["sess_c111_remote"],
                        }},
                    },
                    "scope": { "visibility": "shared" },
                    "source_context_ids": [],
                    "idempotency_key": "c111-seed-remote",
                }),
            )
            .await;
        match created {
            SpokeResult::Ok(value) => value["result"]["record_id"]
                .as_str()
                .or_else(|| value["result"]["record_ids"][0].as_str())
                .unwrap_or_else(|| panic!("the seeded fixture record id: {value}"))
                .to_string(),
            SpokeResult::Reject(reject) => {
                panic!("the fixture seeds its own row: {} {}", reject.code.as_str(), reject.message)
            }
        }
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

        // The fixture's real service, wrapped only to count its record list/get
        // boundary. The seeded row exists in no local store.
        let counting = Arc::new(CountingService::new(
            provider.server(directory.admission()),
        ));
        let service: Arc<dyn RemoteMemoryService> = counting.clone();
        let seeded_id = seed_fixture_row(&service, &client_peer, &space_id, &creator).await;

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
        .await
        .expect("the CLI selection is written");

        // A meaningful conflicting local artifact: its authored body differs
        // from every remote row, so a local fallback would be visible.
        let trap = home
            .join(".nexus42")
            .join("creators")
            .join(&creator)
            .join("memory")
            .join("long-term")
            .join(format!("{TRAP_SLUG}.md"));
        std::fs::create_dir_all(trap.parent().expect("trap dir")).expect("trap dir");
        std::fs::write(
            &trap,
            format!(
                "---\nnexus_memory_version: 1\nmemory_id: mem_c111localtrap\nmemory_kind: story_summary\nupdated_at: 2026-01-01T00:00:00Z\nsource_session_ids:\n- sess_local_trap\n---\n## User Notes\n\n{TRAP_BODY}\n\n## Pushed\n\nlocal-only push section\n"
            ),
        )
        .expect("seed the local trap");

        // list, show, create, list: one signed session each.
        let responders = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut accept_task = Some(serve(
            listener,
            Arc::clone(&service),
            manifest(&authority_host_id),
            client_peer.clone(),
            pubkey(&CLIENT_SEED),
            4,
            BOUND,
            Arc::clone(&responders),
        ));

        let outcome: Result<(), String> = async {
            let listed = stdout(&cli_ok_remote(&home, &["creator", "memory", "list"]).await?);
            ensure!(
                listed.contains(SEED_SLUG),
                "the fixture-only row is listed by the real CLI: {listed}"
            );
            ensure!(
                listed.contains("story_summary"),
                "the remote row keeps its authored kind: {listed}"
            );
            ensure!(
                counting.lists() > 0,
                "the CLI's list crossed the remote provider's list boundary"
            );

            let shown =
                stdout(&cli_ok_remote(&home, &["creator", "memory", "show", SEED_SLUG]).await?);
            ensure!(shown.contains(&format!("slug: {SEED_SLUG}")), "show: {shown}");
            ensure!(
                shown.contains(&format!("memory_id: {SEED_MEMORY_ID}")),
                "show: {shown}"
            );
            ensure!(shown.contains(SEED_BODY), "the fixture-only body is served: {shown}");
            ensure!(!shown.contains(TRAP_BODY), "no local trap body is served: {shown}");
            ensure!(
                counting.gets() > 0 || counting.lists() > 1,
                "the CLI's show crossed the remote provider's record read boundary"
            );

            // The remote create path is retained: a CLI-authored row reaches
            // the same pinned provider and is listed with the seeded row.
            cli_ok_remote(
                &home,
                &[
                    "creator", "memory", "create", SLUG, "--kind", KIND, "--content", BODY,
                ],
            )
            .await?;
            let listed = stdout(&cli_ok_remote(&home, &["creator", "memory", "list"]).await?);
            ensure!(listed.contains(SEED_SLUG), "the seeded row survives: {listed}");
            ensure!(listed.contains(SLUG), "the CLI-created remote row is listed: {listed}");
            ensure!(!listed.contains(TRAP_BODY), "no local trap body in list: {listed}");

            ensure!(
                !provider_store(&home, &creator).exists(),
                "a remote-pinned CLI never opens the local provider's store"
            );
            ensure!(trap.exists(), "the local trap artifact itself is untouched");
            ensure!(
                shown.matches(SEED_MEMORY_ID).count() == 1,
                "the shown identity stayed the fixture's (seeded {seeded_id})"
            );

            // Poll the owned handle BY REFERENCE: a timeout must leave the
            // handle owned here so teardown can abort and join the SAME task.
            let Some(task) = accept_task.as_mut() else {
                return Err("accept task missing".to_string());
            };
            match tokio::time::timeout(BOUND, &mut *task).await {
                Ok(Ok(())) => {
                    accept_task.take();
                }
                Ok(Err(error)) => {
                    accept_task.take();
                    return Err(format!("accept task failed: {error}"));
                }
                Err(_) => return Err("accept task did not finish in bound".to_string()),
            }
            ensure!(
                responders.lock().expect("responder list").len() == 4,
                "every CLI open was served"
            );
            Ok(())
        }
        .await;

        let cleanup = teardown(&mut accept_task, &responders).await;
        match (outcome, cleanup) {
            (Ok(()), Ok(())) => {}
            (Err(error), Ok(())) => panic!("remote CLI scenario: {error}"),
            (Ok(()), Err(error)) => panic!("remote CLI cleanup: {error}"),
            (Err(error), Err(cleanup)) => {
                panic!("remote CLI scenario: {error}; cleanup: {cleanup}")
            }
        }
    }

    /// Deterministic small regression for the two failure-only cleanup branches.
    ///
    /// (a) a real owned accept task is still listening when the wait expires;
    /// teardown must abort and bounded-join that SAME task and release the
    /// listener. (b) the real CLI child dials a local endpoint that accepts TCP
    /// but never completes the handshake, and a short TEST deadline expires; the
    /// runner must kill and reap the child and report it truthfully.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn c_111_cleanup_timeout_paths_are_bounded() {
        // (a) accept-timeout ownership and cancellation.
        let fixture_home = TempDir::new().expect("fixture home");
        let provider = Arc::new(
            FixtureMemoryProvider::open(&fixture_home.path().join("memory.sqlite"))
                .await
                .expect("open the cleanup fixture"),
        );
        let directory = Arc::new(PeerDirectory::new());
        let (listener, addr) = bind().await;
        let responders = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut accept_task = Some(serve(
            listener,
            provider.server(directory.admission()),
            manifest("cleanup-host"),
            "12D3KooWCleanup".to_string(),
            [0u8; 32],
            1,
            BOUND,
            Arc::clone(&responders),
        ));

        let accept_wait = async {
            let Some(task) = accept_task.as_mut() else {
                return Err("accept task missing".to_string());
            };
            match tokio::time::timeout(Duration::from_millis(20), &mut *task).await {
                Ok(Ok(())) => {
                    accept_task.take();
                    Ok(())
                }
                Ok(Err(error)) => {
                    accept_task.take();
                    Err(format!("accept task failed: {error}"))
                }
                Err(_) => Err("accept wait timed out (expected here)".to_string()),
            }
        }
        .await;
        assert!(accept_wait.is_err(), "the accept wait must expire");
        assert!(accept_task.is_some(), "the owned handle survives the timeout");
        let cleanup = teardown(&mut accept_task, &responders).await;
        assert!(
            cleanup.is_ok(),
            "abort plus join is a clean cancellation: {cleanup:?}"
        );
        assert!(accept_task.is_none(), "teardown consumed the owned handle");
        let rebind = tokio::time::timeout(Duration::from_secs(2), TcpListener::bind(addr)).await;
        assert!(
            matches!(rebind, Ok(Ok(_))),
            "the aborted task released the listener: {rebind:?}"
        );

        // (b) real-child timeout at a short deterministic test deadline.
        let actor = DirectActor::new().await;
        let home = actor.home().to_path_buf();
        write_credential(&home, &CLIENT_SEED);
        let holder = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("holder listener");
        let hold_addr = holder.local_addr().expect("holder addr");
        let holder_task = tokio::spawn(async move {
            if let Ok((socket, _)) = holder.accept().await {
                tokio::time::sleep(Duration::from_secs(30)).await;
                drop(socket);
            }
        });
        cli_ok_remote(
            &home,
            &[
                "creator",
                "memory",
                "provider",
                "use",
                "remote",
                "--space-id",
                "spc_cleanup",
                "--credential-id",
                CREDENTIAL_ID,
                "--connect-endpoint",
                &format!("ws://{hold_addr}/memory"),
                "--remote-pubkey-hex",
                &hex(&pubkey(&HOST_SEED)),
                "--allowlist",
                &peer_id(&HOST_SEED),
            ],
        )
        .await
        .expect("the cleanup selection is written");

        let short_deadline = Duration::from_secs(3);
        let run_home = home.clone();
        let started = Instant::now();
        let timed_out = tokio::task::spawn_blocking(move || {
            run_cli_with(&run_home, &["creator", "memory", "list"], short_deadline)
        })
        .await
        .expect("the child-timeout task joins");
        let error = timed_out.expect_err("the real CLI child outlives the short deadline");
        assert!(
            error.contains("killed and reaped"),
            "the timeout path kills and reaps the real child: {error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the timeout path settled promptly: {:?}",
            started.elapsed()
        );
        holder_task.abort();
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
    let refused = run_cli(home, &["creator", "memory", "list"])
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!refused.status.success(), "the remote pin refuses");
    let err = stderr(&refused);
    assert!(err.contains("memory_remote_disabled"), "refusal: {err}");
    assert!(!stdout(&refused).contains(BODY), "no local fallback row");
    let shown = run_cli(home, &["creator", "memory", "show", SLUG])
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!shown.status.success(), "the remote pin refuses show too");
    assert!(
        !stdout(&shown).contains(BODY),
        "no local fallback body on show"
    );
}
