//! N3 §L local-provider rows C-112, C-113 (addition) and C-115.
//!
//! Every scenario runs against the real local `LocalMemoryProvider` (or the
//! real `CoreService` + admitted Host authority) over a disposable temp home.
//! The recovering-object window uses the provider's own `test-hooks` crash
//! seam, exactly like the round-A reconcile proofs, so it is deterministic and
//! never timing-based.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    HostOperation, HostStartConfig, LaunchSpec, ManagedSessionHandle, OperationFinishedEvent,
    ProbeRequest, ProtocolKind, ProviderDescriptor, ProviderHealth, SessionOwner,
};
use nexus_agent_host::{
    HostError, HostFacade, HostManager, HostOperationId, HostResult, HostSessionId, LaunchStrategy,
    ProviderAdapter, ProviderId,
};
use nexus_contracts::generated::daemon_api::agent_host::create_session_request::CreateSessionRequest;
use nexus_contracts::generated::daemon_api::agent_host::execute_operation_request::ExecuteOperationRequest;
use nexus_contracts::{ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_core::memory_provider::LocalMemoryProvider;
use nexus_core::{CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreService, Principal};
use nexus_home_layout::operational_workspace_dir;
use nexus_provider_ports::{ProviderPort, ProviderResult};
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeResult};
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORKSPACE: &str = "default";
const SUBJECT: &str = "sbj_mira";
const KIND: &str = "nexus.story_summary";
const TEXT: &str = "Mira promised to return before the northern gate opens at dawn.";
const UPDATED: &str = "Mira promised to return before the western gate opens at dusk.";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
}

fn seed_env() -> Env {
    let tmp = TempDir::new().expect("temp home");
    let user_home = tmp.path().to_path_buf();
    std::fs::create_dir_all(user_home.join(".nexus42")).expect("nexus dir");
    std::fs::create_dir_all(operational_workspace_dir(&user_home, CREATOR, WORKSPACE))
        .expect("workspace dir");
    std::fs::write(
        user_home.join(".nexus42").join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"{WORKSPACE}\"\n"
        ),
    )
    .expect("seed config");
    Env { _tmp: tmp, user_home }
}

async fn open_provider(env: &Env) -> LocalMemoryProvider {
    LocalMemoryProvider::open(env.user_home.clone(), CREATOR, WORKSPACE)
        .await
        .expect("provider open")
}

fn admin() -> schemas::Principal {
    serde_json::from_value(json!({ "principal_id": CREATOR, "extensions": {} }))
        .expect("wire-valid principal")
}

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid payload")
}

/// Project a port result onto one assertion shape (success payload or refusal).
fn wire<T: serde::Serialize>(result: &SpokeResult<T>) -> Value {
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

async fn ensure(provider: &LocalMemoryProvider) {
    let subject = wire(
        &provider
            .ensure_subject(
                &admin(),
                dto(json!({ "space_id": provider.space_id(), "subject_id": SUBJECT })),
            )
            .await,
    );
    assert_eq!(subject["status"], "committed", "{subject}");
    assert_eq!(subject["result"]["subject_id"], SUBJECT, "{subject}");
}

fn create_request(key: &str) -> schemas::MemoryRecordsCreateRequest {
    dto(json!({
        "space_id": "spc_local_default",
        "subject_id": SUBJECT,
        "kind": KIND,
        "content": { "text": TEXT },
        "scope": { "visibility": "shared" },
        "idempotency_key": key,
    }))
}

fn update_request(key: &str, record_id: &str, revision: u64) -> schemas::MemoryRecordsUpdateRequest {
    dto(json!({
        "space_id": "spc_local_default",
        "record_id": record_id,
        "expected_revision": revision,
        "content": { "text": UPDATED },
        "idempotency_key": key,
    }))
}

fn list_request() -> schemas::MemoryRecordsListRequest {
    dto(json!({
        "space_id": "spc_local_default",
        "subject_id": SUBJECT,
        "selection": { "mode": "subject_all" },
    }))
}

fn recall_request() -> schemas::MemoryRecallRequest {
    dto(json!({
        "space_id": "spc_local_default",
        "subject_id": SUBJECT,
        "selection": { "mode": "subject_all" },
        "budget": { "max_items": 10 },
    }))
}

fn get_request(record_id: &str) -> schemas::MemoryRecordsGetRequest {
    dto(json!({ "space_id": "spc_local_default", "record_id": record_id }))
}

fn record_id_of(body: &Value) -> String {
    body["result"]["record_id"]
        .as_str()
        .or_else(|| body["result"]["record_ids"][0].as_str())
        .unwrap_or_else(|| panic!("expected a record id, got {body}"))
        .to_string()
}

/// Every regular file under `root`, as sorted `/`-separated relative paths.
fn files_under(root: &Path) -> Vec<String> {
    fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, out);
            } else {
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

/// Every published markdown projection of the workspace, relative to the home.
fn projections(env: &Env) -> Vec<String> {
    files_under(&operational_workspace_dir(
        &env.user_home,
        CREATOR,
        WORKSPACE,
    ))
    .into_iter()
    .filter(|path| path.ends_with(".md"))
    .collect()
}

fn read(env: &Env, relative: &str) -> String {
    std::fs::read_to_string(
        operational_workspace_dir(&env.user_home, CREATOR, WORKSPACE).join(relative),
    )
    .expect("published projection readable")
}

// ---------------------------------------------------------------------------
// C-112: the local layout and `kind` frontmatter survive a normal operation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_112_local_layout_and_kind_frontmatter_preserved_local() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;

    let created = wire(
        &provider
            .create_record(&admin(), create_request("c112-create"))
            .await,
    );
    assert_eq!(created["status"], "committed", "{created}");
    let record_id = record_id_of(&created);

    // The projection lands at the provider's documented record path — found by
    // walking the real workspace, not by a hardcoded file name.
    assert_eq!(
        projections(&env),
        vec![format!("memory-store/records/{record_id}.md")],
        "the record projection owns exactly one markdown file"
    );
    let path = format!("memory-store/records/{record_id}.md");
    let published = read(&env, &path);
    let (frontmatter, body) = published
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n\n"))
        .expect("markdown frontmatter and body");
    for expected in [
        format!("record_id: {record_id}"),
        "space_id: spc_local_default".to_string(),
        format!("subject_id: {SUBJECT}"),
        format!("kind: {KIND}"),
        "visibility: shared".to_string(),
        "revision: 1".to_string(),
    ] {
        assert!(
            frontmatter.lines().any(|line| line == expected),
            "frontmatter carries {expected:?}: {frontmatter}"
        );
    }
    assert_eq!(body, TEXT, "the projection body is the record text");
    let created_at = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix("created_at: "))
        .expect("created_at frontmatter")
        .to_string();

    // A normal guarded update keeps the same layout and kind; only revision,
    // updated_at and the body move.
    let updated = wire(
        &provider
            .update_record(&admin(), update_request("c112-update", &record_id, 1))
            .await,
    );
    assert_eq!(updated["status"], "committed", "{updated}");
    assert_eq!(
        projections(&env),
        vec![path.clone()],
        "an update rewrites the same projection, it does not add one"
    );
    let published = read(&env, &path);
    let (frontmatter, body) = published
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n\n"))
        .expect("markdown frontmatter and body");
    for expected in [
        format!("kind: {KIND}"),
        "visibility: shared".to_string(),
        "revision: 2".to_string(),
        format!("created_at: {created_at}"),
    ] {
        assert!(
            frontmatter.lines().any(|line| line == expected),
            "post-update frontmatter carries {expected:?}: {frontmatter}"
        );
    }
    assert_eq!(body, UPDATED, "the projection body is the new record text");

    let served = wire(
        &provider
            .get_record(&admin(), get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["revision"], 2, "{served}");
    assert_eq!(served["record"]["content"]["text"], UPDATED, "{served}");
    assert_eq!(served["record"]["kind"], KIND, "{served}");
}

// ---------------------------------------------------------------------------
// C-113 addition: a recovering object is withheld until it is resolved
// ---------------------------------------------------------------------------

/// After the journal commit, the object's SQL effect is durable but its file
/// effect is not published yet. Until an open's bounded recovery settles that
/// intent, ordinary list/recall reads must not expose it and no file may exist.
/// The reopen then settles it into exactly one truthful projection.
#[tokio::test]
async fn c_113_recovering_objects_excluded_until_resolved_local() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;

    // A crash after the journal COMMIT: the intent committed, no file effect.
    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .create_record(&admin(), create_request("c113-recovering"))
            .await,
    );
    assert_eq!(lost["error"]["code"], "OUTCOME_UNCERTAIN", "{lost}");

    // D-10 isolation: the recovering object is excluded from every normal read
    // and nothing was published.
    let listed = wire(&provider.list_records(&admin(), list_request()).await);
    assert_eq!(listed["total_authorized"], 0, "{listed}");
    let recalled = wire(&provider.recall(&admin(), recall_request()).await);
    assert_eq!(recalled["total_authorized"], 0, "{recalled}");
    assert_eq!(
        projections(&env),
        Vec::<String>::new(),
        "a recovering intent has no published projection"
    );

    // The reopen runs the bounded recovery: the object becomes readable once,
    // with its exact body.
    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(
        projections(&env).len(),
        1,
        "recovery publishes exactly one projection"
    );
    let listed = wire(&provider.list_records(&admin(), list_request()).await);
    assert_eq!(listed["total_authorized"], 1, "{listed}");
    let record_id = listed["records"][0]["record_id"]
        .as_str()
        .expect("the recovered record id")
        .to_string();
    let served = wire(
        &provider
            .get_record(&admin(), get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT, "{served}");
    let published = read(&env, &projections(&env)[0]);
    assert!(
        published.contains(TEXT),
        "the recovered projection carries the body: {published}"
    );

    // One effect: a same-key replay of the settled intent neither re-publishes
    // nor duplicates it.
    let replay = wire(
        &provider
            .create_record(&admin(), create_request("c113-recovering"))
            .await,
    );
    assert_eq!(replay["status"], "committed", "{replay}");
    assert_eq!(record_id_of(&replay), record_id);
    assert_eq!(
        projections(&env).len(),
        1,
        "settlement left exactly one projection"
    );
}

// ---------------------------------------------------------------------------
// C-115: `remember: true` stays refused on both arms
// ---------------------------------------------------------------------------

const PROVIDER: &str = "n3-local-mind-fixture";

#[derive(Default)]
struct MindFixtureAdapter;

#[async_trait]
impl ProviderAdapter for MindFixtureAdapter {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: ProviderId::new(PROVIDER),
            display_name: "N3 local mind fixture".to_string(),
            protocol_kind: ProtocolKind::NativeCli,
            capabilities: CapabilityDescriptor::acp_full(),
        }
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::acp_full()
    }

    async fn probe(&self, _request: ProbeRequest) -> HostResult<ProviderHealth> {
        Ok(ProviderHealth {
            provider_id: ProviderId::new(PROVIDER),
            available: true,
            latency_ms: None,
            message: Some("N3 local mind fixture".to_string()),
        })
    }

    async fn launch(&self, _spec: LaunchSpec) -> HostResult<ManagedSessionHandle> {
        Ok(ManagedSessionHandle {
            provider_id: ProviderId::new(PROVIDER),
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
            return Err(HostError::internal("the fixture executes prompts only"));
        };
        let _ = content
            .into_iter()
            .filter_map(|block| match block {
                HostContentBlock::Text { text } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        Ok(Box::pin(futures_util::stream::iter(vec![Ok(
            HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session.session_id.clone(),
                op_id,
                reason: FinishReason::EndTurn,
            }),
        )])))
    }

    async fn cancel(&self, _session: &ManagedSessionHandle, _op: HostOperationId) -> HostResult<()> {
        Ok(())
    }

    async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
        Ok(())
    }
}

/// The authority's composed provider port; counts calls so a refusal can prove
/// it produced no provider effect.
struct CountingPort {
    calls: AtomicUsize,
}

impl CountingPort {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
        })
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ProviderPort for CountingPort {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        self.calls.fetch_add(1, Ordering::SeqCst);
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

fn execute_prompt(remember: bool) -> ExecuteOperationRequest {
    serde_json::from_value(json!({
        "kind": "prompt",
        "content": "hello",
        "remember": remember,
    }))
    .expect("wire-valid prompt request")
}

#[tokio::test]
async fn c_115_remember_true_refusal_unchanged_local() {
    let tmp = TempDir::new().expect("temp home");
    let user_home = tmp.path().to_path_buf();
    let selector = CoreHomeService::open(user_home.clone()).expect("home entry opens");
    let creator = selector
        .register_creator(nexus_contracts::CoreRegisterCreatorRequest {
            display_name: Some("N3 Remember Author".parse().expect("valid display name")),
            platform_creator_id: None,
        })
        .await
        .expect("register the fixture creator");
    let creator_id = creator.creator_id;
    std::fs::create_dir_all(operational_workspace_dir(&user_home, &creator_id, WORKSPACE))
        .expect("materialize the workspace");
    selector
        .select_workspace(nexus_contracts::SetActiveWorkspaceRequest {
            creator_id: Some(creator_id.clone()),
            workspace_slug: WORKSPACE.to_string(),
        })
        .await
        .expect("select the fixture workspace");

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("open the local core");
    let principal: Principal = core.active_principal().await.expect("principal");

    let world = core
        .create_world(
            &principal,
            dto(json!({ "title": "N3 remember world" })),
        )
        .await
        .expect("create the owned World")
        .world_id;
    let character = core
        .create_character(
            &principal,
            dto(json!({ "display_name": "Ada", "world_id": world })),
        )
        .await
        .expect("create the owned Character with its initial binding");

    // Deterministic local agent host: no process, no model, no network.
    let manager = Arc::new(HostManager::new());
    manager
        .register_provider(
            Arc::new(MindFixtureAdapter),
            LaunchStrategy::NativeCli {
                command: "n3-local-mind-fixture".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
            },
        )
        .await;
    let mut host_config = nexus_agent_host::config::AgentHostConfig::default();
    host_config.max_ops_per_session = 4;
    manager
        .start(HostStartConfig {
            config_path: nexus_agent_host::config::agent_host_config_path(&user_home),
            workspace_root: user_home.clone(),
            max_sessions: host_config.max_sessions,
            max_ops_per_session: host_config.max_ops_per_session,
            timeouts: host_config.timeouts.clone(),
            host_config: Some(host_config),
            admitted_catalog: None,
            probe_owner: Some(SessionOwner {
                creator_id: creator_id.clone(),
                workspace_root: user_home.clone(),
                orchestration_run_id: None,
            }),
        })
        .await
        .expect("the deterministic host starts");
    let port = CountingPort::new();
    let handle = core
        .attach_host(manager, port.clone())
        .expect("the admitted Host authority attaches");

    let session = handle
        .create_session(
            &principal,
            dto::<CreateSessionRequest>(json!({
                "provider_id": PROVIDER,
                "actor_ref": { "actor_kind": "character", "character_id": character.character.character_id.as_str() },
                "viewpoint": { "world_id": world, "binding_id": character.binding.binding_id.as_str() },
            })),
        )
        .await
        .expect("the admitted Character session is created");

    // (a) Character arm: the typed not_supported refusal, before any effect.
    let refused = handle
        .execute(&principal, session.session_id.clone(), execute_prompt(true))
        .await
        .expect_err("a Character capture request is refused");
    match &refused {
        CoreError::Coded { code, .. } => assert_eq!(code, "not_supported"),
        other => panic!("expected the not_supported envelope, got {other:?}"),
    }
    assert_eq!(
        port.call_count(),
        0,
        "the Character refusal reaches no provider effect"
    );

    // (b) non-Character arm: the 422 invalid_input refusal is unchanged.
    let refused = handle
        .execute(
            &principal,
            uuid::Uuid::new_v4().to_string(),
            execute_prompt(true),
        )
        .await
        .expect_err("remember on a non-Character session stays invalid_input");
    match &refused {
        CoreError::InvalidInput { field, .. } => assert_eq!(field, "remember"),
        other => panic!("expected the invalid `remember` refusal, got {other:?}"),
    }
    assert_eq!(
        port.call_count(),
        0,
        "the non-Character refusal reaches no provider effect"
    );

    // The same Character session still serves an ordinary prompt after the two
    // refusals: the refusals were the request, not a broken session.
    handle
        .execute(&principal, session.session_id.clone(), execute_prompt(false))
        .await
        .expect("a plain Character prompt is still served");
    assert_eq!(port.call_count(), 0, "the Character prompt path is the adapter");
}
