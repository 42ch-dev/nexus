//! N3 §L local-provider rows C-112, C-113 (addition) and C-115.
//!
//! Every scenario runs against the real local `LocalMemoryProvider` (or the
//! real `CoreService` + admitted Host authority) over a disposable temp home.
//! The recovering-object window uses the provider's own `test-hooks` crash
//! seam, exactly like the round-A reconcile proofs, so it is deterministic.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
use nexus_contracts::{
    CoreRegisterCreatorRequest, ProviderCall, ProviderEventBatch, ProviderReply,
    SetActiveWorkspaceRequest,
};
use nexus_core::memory_provider::{legacy, LocalMemoryProvider};
use nexus_core::{CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreService, Principal};
use nexus_home_layout::{creator_soul_md_path, operational_workspace_dir, workspace_state_db_path};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeResult};
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORKSPACE: &str = "default";
const SPACE: &str = "spc_local_default";
const SUBJECT: &str = "sbj_mira";
const KIND: &str = "nexus.story_summary";
const TEXT: &str = "Mira promised to return before the northern gate opens at dawn.";
const UPDATED: &str = "Mira promised to return before the western gate opens at dusk.";
const LEGACY_SLUG: &str = "tide-notes";
const LEGACY_MEMORY_ID: &str = "mem_legacytidenotes";
const LEGACY_SESSION: &str = "sess_legacy_tide";
const LEGACY_BODY: &str = "LEGACYBODY the authored tide notes must survive";

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
    Env {
        _tmp: tmp,
        user_home,
    }
}

async fn open_provider(env: &Env) -> LocalMemoryProvider {
    LocalMemoryProvider::open(env.user_home.clone(), CREATOR, WORKSPACE)
        .await
        .expect("provider open")
}

fn principal(id: &str) -> schemas::Principal {
    serde_json::from_value(json!({ "principal_id": id, "extensions": {} }))
        .expect("wire-valid principal")
}

fn admin() -> schemas::Principal {
    principal(CREATOR)
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

async fn ensure(provider: &LocalMemoryProvider, principal: &schemas::Principal) {
    let subject = wire(
        &provider
            .ensure_subject(
                principal,
                dto(json!({ "space_id": provider.space_id(), "subject_id": SUBJECT })),
            )
            .await,
    );
    assert_eq!(subject["status"], "committed", "{subject}");
    assert_eq!(subject["result"]["subject_id"], SUBJECT, "{subject}");
}

fn create_request(key: &str, kind: &str, text: &str) -> schemas::MemoryRecordsCreateRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "kind": kind,
        "content": { "text": text },
        "scope": { "visibility": "shared" },
        "idempotency_key": key,
    }))
}

fn update_request(
    key: &str,
    record_id: &str,
    revision: u64,
) -> schemas::MemoryRecordsUpdateRequest {
    dto(json!({
        "space_id": SPACE,
        "record_id": record_id,
        "expected_revision": revision,
        "content": { "text": UPDATED },
        "idempotency_key": key,
    }))
}

fn list_request(subject: &str) -> schemas::MemoryRecordsListRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": subject,
        "selection": { "mode": "subject_all" },
    }))
}

fn recall_request(subject: &str) -> schemas::MemoryRecallRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": subject,
        "selection": { "mode": "subject_all" },
        "budget": { "max_items": 10 },
    }))
}

fn get_request(record_id: &str) -> schemas::MemoryRecordsGetRequest {
    dto(json!({ "space_id": SPACE, "record_id": record_id }))
}

fn receipt_request(receipt_id: &str) -> schemas::MemoryReceiptsGetRequest {
    dto(json!({ "space_id": SPACE, "receipt_id": receipt_id }))
}

fn candidates_request(subject: &str) -> schemas::MemoryCandidatesListRequest {
    dto(json!({ "space_id": SPACE, "subject_id": subject, "status": "pending" }))
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
    .filter(|path| {
        std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    })
    .collect()
}

fn read(env: &Env, relative: &str) -> String {
    std::fs::read_to_string(
        operational_workspace_dir(&env.user_home, CREATOR, WORKSPACE).join(relative),
    )
    .expect("published projection readable")
}

fn legacy_memory_dir(env: &Env) -> PathBuf {
    env.user_home
        .join(".nexus42")
        .join("creators")
        .join(CREATOR)
        .join("memory")
        .join("long-term")
}

/// One scalar read on the released workspace store, used only as a test oracle
/// for the original pending receipt/record identity (the same observation the
/// existing `memory_reconcile` crash test makes).
async fn store_scalar(db: &Path, sql: &'static str, key: &str) -> String {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db)
        .read_only(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("read-only store oracle");
    let value: (String,) = sqlx::query_as(sql)
        .bind(key)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| panic!("store oracle query failed: {error}"));
    pool.close().await;
    value.0
}

async fn store_scalar_plain(db: &Path, sql: &'static str) -> String {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db)
        .read_only(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("read-only store oracle");
    let value: (String,) = sqlx::query_as(sql)
        .fetch_one(&pool)
        .await
        .expect("store oracle query");
    pool.close().await;
    value.0
}

async fn store_count(db: &Path, sql: &'static str) -> i64 {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db)
        .read_only(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("read-only store oracle");
    let (count,): (i64,) = sqlx::query_as(sql)
        .fetch_one(&pool)
        .await
        .expect("store oracle count");
    pool.close().await;
    count
}

// ---------------------------------------------------------------------------
// C-112: the local layout and `kind` frontmatter survive a normal operation
// ---------------------------------------------------------------------------

/// Seed the author-visible legacy artifacts the preservation contract names:
/// a `SOUL.md` with its required and user/pushed sections, and a long-term
/// memory file carrying the nine-value kind frontmatter, slug, source sessions
/// and user/pushed sections.
fn seed_legacy_artifacts(env: &Env) -> (PathBuf, PathBuf) {
    let soul_path = creator_soul_md_path(&env.user_home, CREATOR);
    std::fs::create_dir_all(soul_path.parent().expect("soul dir")).expect("soul dir");
    std::fs::write(
        &soul_path,
        format!(
            "---\ncreator_id: {CREATOR}\nschema_version: 1\n---\n\
             ## Personality\n\nSteady hands, long memory.\n\n\
             ## Experience\n\nA winter at the northern gate.\n\n\
             ## User Notes\n\nUSERMARKER the author's own note.\n\n\
             ## Pushed\n\nPUSHEDMARKER carried forward by the author.\n"
        ),
    )
    .expect("SOUL.md fixture");

    let memory_path = legacy_memory_dir(env).join(format!("{LEGACY_SLUG}.md"));
    std::fs::create_dir_all(memory_path.parent().expect("memory dir")).expect("memory dir");
    std::fs::write(
        &memory_path,
        format!(
            "---\nnexus_memory_version: 1\nmemory_id: {LEGACY_MEMORY_ID}\n\
             memory_kind: story_summary\nupdated_at: 2026-01-02T03:04:05Z\n\
             source_session_ids:\n- {LEGACY_SESSION}\n---\n\
             {LEGACY_BODY}\n\n## User Notes\n\nUSERMARKER the keeper's note.\n\n\
             ## Pushed\n\nPUSHEDMARKER the pushed paragraph.\n"
        ),
    )
    .expect("legacy long-term memory fixture");
    (soul_path, memory_path)
}

/// The same workspace's capture → review → promote pipeline.
async fn c112_promote_candidate(provider: &LocalMemoryProvider) {
    // The capture → review → promote pipeline on the same workspace.
    let captured = wire(
        &provider
            .capture(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "content": { "text": "C112PROMOTE a candidate through the review pipeline" },
                    "capture_scope": { "visibility": "shared" },
                    "source_context_ids": [],
                    "idempotency_key": "c112-capture",
                })),
            )
            .await,
    );
    let candidate_id = captured["result"]["candidate_ids"][0]
        .as_str()
        .unwrap_or_else(|| panic!("a captured candidate id: {captured}"))
        .to_string();
    let reviewed = wire(
        &provider
            .review_candidate(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "candidate_id": candidate_id,
                    "expected_revision": 1,
                    "decision": { "action": "promote" },
                    "idempotency_key": "c112-review",
                })),
            )
            .await,
    );
    assert_eq!(reviewed["status"], "committed", "{reviewed}");
}

/// The protected author-visible bytes stay untouched by every operation, and the
/// consumer-visible legacy bridge still serves the authored memory.
async fn c112_verify_legacy_untouched(
    provider: &LocalMemoryProvider,
    env: &Env,
    soul_path: &std::path::Path,
    memory_path: &std::path::Path,
    soul_before: &[u8],
    memory_before: &[u8],
) {
    // The protected author-visible bytes are untouched by every operation.
    assert_eq!(
        std::fs::read(soul_path).expect("SOUL.md bytes").as_slice(),
        soul_before,
        "SOUL.md stayed byte-identical"
    );
    assert_eq!(
        std::fs::read(memory_path)
            .expect("long-term bytes")
            .as_slice(),
        memory_before,
        "memory/long-term/{LEGACY_SLUG}.md stayed byte-identical"
    );
    assert_eq!(
        std::fs::read_dir(legacy_memory_dir(env))
            .expect("legacy long-term dir")
            .count(),
        1,
        "no new file was written into the legacy layout"
    );

    // The consumer-visible bridge still serves the authored legacy memory with
    // its kind and body: the bytes above are live, not orphaned.
    let creator_subject = legacy::creator_subject_id(CREATOR);
    let ensured = wire(
        &provider
            .ensure_subject(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": creator_subject,
                    "kind": { "ns": legacy::CREATOR_KIND_NS, "id": CREATOR },
                })),
            )
            .await,
    );
    assert_eq!(ensured["status"], "committed", "{ensured}");
    let bridged = wire(
        &provider
            .list_records(&admin(), list_request(&creator_subject))
            .await,
    );
    let projected = bridged["records"]
        .as_array()
        .expect("records array")
        .iter()
        .find(|record| {
            record["content"]["text"]
                .as_str()
                .is_some_and(|text| text.contains(LEGACY_BODY))
        })
        .unwrap_or_else(|| panic!("the legacy memory is still projected: {bridged}"));
    assert_eq!(projected["kind"], KIND, "{projected}");
    assert!(
        projected["content"]["text"]
            .as_str()
            .is_some_and(|text| text.contains("## User Notes") && text.contains("PUSHEDMARKER")),
        "the projected legacy body keeps its authored sections: {projected}"
    );
}

#[tokio::test]
async fn c_112_local_layout_and_kind_frontmatter_preserved_local() {
    let env = seed_env();
    let (soul_path, memory_path) = seed_legacy_artifacts(&env);
    let soul_before = std::fs::read(&soul_path).expect("SOUL.md bytes");
    let memory_before = std::fs::read(&memory_path).expect("long-term bytes");

    let provider = open_provider(&env).await;
    ensure(&provider, &admin()).await;

    // A normal provider create + update (the current C-112 operation).
    let created = wire(
        &provider
            .create_record(&admin(), create_request("c112-create", KIND, TEXT))
            .await,
    );
    assert_eq!(created["status"], "committed", "{created}");
    let record_id = record_id_of(&created);

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
        format!("space_id: {SPACE}"),
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

    let served = wire(&provider.get_record(&admin(), get_request(&record_id)).await);
    assert_eq!(served["record"]["revision"], 2, "{served}");
    assert_eq!(served["record"]["content"]["text"], UPDATED, "{served}");
    assert_eq!(served["record"]["kind"], KIND, "{served}");

    c112_promote_candidate(&provider).await;
    c112_verify_legacy_untouched(
        &provider,
        &env,
        &soul_path,
        &memory_path,
        &soul_before,
        &memory_before,
    )
    .await;
}

// ---------------------------------------------------------------------------
// C-113 addition: a recovering object is withheld until it is resolved
// ---------------------------------------------------------------------------

/// After the journal commit, the object's SQL effect is durable but its file
/// effect is not published yet. Until an open's bounded recovery settles that
/// intent, ordinary list/recall reads must not expose it and no file may exist.
/// The reopen then settles the ORIGINAL pending receipt/record into exactly one
/// truthful projection.
#[tokio::test]
async fn c_113_recovering_objects_excluded_until_resolved_local() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider, &admin()).await;

    // A crash after the journal COMMIT: the intent committed, no file effect.
    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .create_record(&admin(), create_request("c113-recovering", KIND, TEXT))
            .await,
    );
    assert_eq!(lost["error"]["code"], "OUTCOME_UNCERTAIN", "{lost}");

    // D-10 isolation: the recovering object is excluded from every normal read
    // and nothing was published.
    let listed = wire(&provider.list_records(&admin(), list_request(SUBJECT)).await);
    assert_eq!(listed["total_authorized"], 0, "{listed}");
    let recalled = wire(&provider.recall(&admin(), recall_request(SUBJECT)).await);
    assert_eq!(recalled["total_authorized"], 0, "{recalled}");
    assert_eq!(
        projections(&env),
        Vec::<String>::new(),
        "a recovering intent has no published projection"
    );

    // The original identity at the supported local test/operator boundary:
    // the pending receipt and the intended record this commit created, read
    // from the released store before anything reopens it.
    let db = workspace_state_db_path(&env.user_home, CREATOR, WORKSPACE);
    let original_receipt = store_scalar(
        &db,
        "SELECT receipt_id FROM memory_receipts WHERE idempotency_key = ?1",
        "c113-recovering",
    )
    .await;
    let original_record =
        store_scalar_plain(&db, "SELECT record_id FROM memory_records LIMIT 1").await;
    let pending_status = store_scalar(
        &db,
        "SELECT status FROM memory_receipts WHERE receipt_id = ?1",
        &original_receipt,
    )
    .await;
    assert_eq!(pending_status, "pending", "the original receipt is pending");

    // The reopen runs the bounded recovery: the original object becomes
    // readable once, with its exact body and settlement.
    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(
        projections(&env),
        vec![format!("memory-store/records/{original_record}.md")],
        "recovery publishes exactly the original record's projection"
    );
    let receipt = wire(
        &provider
            .get_receipt(&admin(), receipt_request(&original_receipt))
            .await,
    );
    assert_eq!(receipt["receipt"]["status"], "committed", "{receipt}");
    assert_eq!(
        receipt["receipt"]["result"]["record_id"], original_record,
        "the settled receipt carries the original record"
    );
    let listed = wire(&provider.list_records(&admin(), list_request(SUBJECT)).await);
    assert_eq!(listed["total_authorized"], 1, "{listed}");
    assert_eq!(
        listed["records"][0]["record_id"], original_record,
        "{listed}"
    );
    let served = wire(
        &provider
            .get_record(&admin(), get_request(&original_record))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT, "{served}");
    let published = read(&env, &projections(&env)[0]);
    assert!(
        published.contains(TEXT),
        "the recovered projection carries the body: {published}"
    );

    // One effect: a same-key replay of the settled intent neither re-publishes
    // nor duplicates it, and answers the original identity.
    let replay = wire(
        &provider
            .create_record(&admin(), create_request("c113-recovering", KIND, TEXT))
            .await,
    );
    assert_eq!(replay["status"], "committed", "{replay}");
    assert_eq!(record_id_of(&replay), original_record);
    assert_eq!(
        projections(&env).len(),
        1,
        "settlement left exactly one projection"
    );
    assert_eq!(
        store_count(&db, "SELECT COUNT(*) FROM memory_records").await,
        1,
        "one record effect"
    );
    assert_eq!(
        store_count(
            &db,
            "SELECT COUNT(*) FROM memory_receipts WHERE op = 'memory.records.create'"
        )
        .await,
        1,
        "one create receipt effect"
    );
}

// ---------------------------------------------------------------------------
// C-115: `remember: true` stays refused on both arms
// ---------------------------------------------------------------------------

const PROVIDER: &str = "n3-local-mind-fixture";

/// Deterministic agent adapter: records every execution and completion, so a
/// refusal can prove it never reached this boundary and a plain prompt can
/// prove real execution. No process, model or network is involved.
#[derive(Default)]
struct MindFixtureAdapter {
    executions: AtomicUsize,
    completions: AtomicUsize,
}

impl MindFixtureAdapter {
    fn executions(&self) -> usize {
        self.executions.load(Ordering::SeqCst)
    }

    fn completions(&self) -> usize {
        self.completions.load(Ordering::SeqCst)
    }
}

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
        self.executions.fetch_add(1, Ordering::SeqCst);
        let HostOperation::Prompt { op_id, content, .. } = op else {
            return Err(HostError::internal("the fixture executes prompts only"));
        };
        let text = content
            .into_iter()
            .filter_map(|block| match block {
                HostContentBlock::Text { text } => Some(text),
                HostContentBlock::ResourceLink { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.is_empty(), "the Host assembled a non-empty prompt");
        self.completions.fetch_add(1, Ordering::SeqCst);
        Ok(Box::pin(futures_util::stream::iter(vec![Ok(
            HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session.session_id.clone(),
                op_id,
                reason: FinishReason::EndTurn,
            }),
        )])))
    }

    async fn cancel(
        &self,
        _session: &ManagedSessionHandle,
        _op: HostOperationId,
    ) -> HostResult<()> {
        Ok(())
    }

    async fn shutdown(&self, _session: ManagedSessionHandle) -> HostResult<()> {
        Ok(())
    }
}

/// The authority's composed provider port; the Character path never uses it.
struct QuietPort;

#[async_trait]
impl ProviderPort for QuietPort {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
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

/// Run `remember:true` once and require the typed refusal.
async fn expect_remember_refusal(
    handle: &nexus_core::HostHandle,
    principal: &Principal,
    session_id: String,
    expected: &str,
) {
    let refused = handle
        .execute(principal, session_id, execute_prompt(true))
        .await
        .expect_err("a remember:true prompt is refused");
    match &refused {
        CoreError::Coded { code, .. } => assert_eq!(code, expected),
        CoreError::InvalidInput { field, .. } => {
            assert_eq!(expected, "invalid_input");
            assert_eq!(field, "remember");
        }
        other => panic!("expected the typed remember refusal, got {other:?}"),
    }
}

const CREATOR_BODY: &str = "C115CREATOR the creator memory that must not change";
const CHARACTER_BODY: &str = "C115CHARACTER the character memory that must not change";

async fn ensure_kind(
    provider: &LocalMemoryProvider,
    principal: &schemas::Principal,
    subject: &str,
    namespace: &str,
    kind_id: &str,
) {
    let ensured = wire(
        &provider
            .ensure_subject(
                principal,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": subject,
                    "kind": { "ns": namespace, "id": kind_id },
                })),
            )
            .await,
    );
    assert_eq!(ensured["status"], "committed", "{ensured}");
    assert_eq!(ensured["result"]["subject_id"], subject, "{ensured}");
}

fn create_request_for(
    subject: &str,
    key: &str,
    kind: &str,
    text: &str,
) -> schemas::MemoryRecordsCreateRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": subject,
        "kind": kind,
        "content": { "text": text },
        "scope": { "visibility": "shared" },
        "idempotency_key": key,
    }))
}

/// Observe one admitted subject through the public `MemoryPort`: the read must
/// SUCCEED, carry exactly the seeded record/body/revision, and report no
/// pending candidate. A refusal is a failed observation, never an empty set.
async fn observe_subject(
    provider: &LocalMemoryProvider,
    principal: &schemas::Principal,
    subject: &str,
    expected_body: &str,
) {
    let records = wire(
        &provider
            .list_records(principal, list_request(subject))
            .await,
    );
    assert!(
        records["records"].as_array().is_some(),
        "the record read succeeded for {subject}: {records}"
    );
    assert_eq!(records["total_authorized"], 1, "{subject}: {records}");
    let record = &records["records"][0];
    assert_eq!(record["subject_id"], subject, "{records}");
    assert_eq!(record["revision"], 1, "{records}");
    assert_eq!(record["content"]["text"], expected_body, "{records}");

    let candidates = wire(
        &provider
            .list_candidates(principal, candidates_request(subject))
            .await,
    );
    assert!(
        candidates["candidates"].as_array().is_some(),
        "the candidate read succeeded for {subject}: {candidates}"
    );
    assert_eq!(
        candidates["candidates"]
            .as_array()
            .expect("candidate array")
            .len(),
        0,
        "no capture/candidate effect for {subject}: {candidates}"
    );
}

async fn observe_actual_subjects(
    user_home: &Path,
    creator: &str,
    creator_subject: &str,
    character_subject: &str,
) {
    let provider = LocalMemoryProvider::open_read_only(user_home.to_path_buf(), creator, WORKSPACE)
        .await
        .expect("read-only provider observation");
    let principal = principal(creator);
    observe_subject(&provider, &principal, creator_subject, CREATOR_BODY).await;
    observe_subject(&provider, &principal, character_subject, CHARACTER_BODY).await;
}

/// Seed the real home, Creator, owned World/Character (and their admitted
/// subjects) this scenario's local core and Host then drive.
async fn c115_seed(user_home: &std::path::Path) -> (String, String, String, String) {
    let selector = CoreHomeService::open(user_home.to_path_buf()).expect("home entry opens");
    let creator = selector
        .register_creator(CoreRegisterCreatorRequest {
            display_name: Some("N3 Remember Author".parse().expect("valid display name")),
            platform_creator_id: None,
        })
        .await
        .expect("register the fixture creator");
    let creator_id = creator.creator_id;
    std::fs::create_dir_all(operational_workspace_dir(user_home, &creator_id, WORKSPACE))
        .expect("materialize the workspace");
    selector
        .select_workspace(SetActiveWorkspaceRequest {
            creator_id: Some(creator_id.clone()),
            workspace_slug: WORKSPACE.to_string(),
        })
        .await
        .expect("select the fixture workspace");

    // Create the real World/Character first, then release the writer so the
    // provider fixtures can seed the ACTUAL admitted subjects.
    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.to_path_buf(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("open the local core");
    let first_principal = core.active_principal().await.expect("principal");
    let world = core
        .create_world(
            &first_principal,
            dto(json!({ "title": "N3 remember world" })),
        )
        .await
        .expect("create the owned World")
        .world_id;
    let character = core
        .create_character(
            &first_principal,
            dto(json!({ "display_name": "Ada", "world_id": world })),
        )
        .await
        .expect("create the owned Character with its initial binding");
    let character_id = character.character.character_id.as_str().to_string();
    let binding_id = character.binding.binding_id.as_str().to_string();
    core.close()
        .await
        .expect("the seed core releases its writer");

    // Seed one record under each REAL subject the Core memory paths address.
    let creator_subject = legacy::creator_subject_id(&creator_id);
    let character_subject = legacy::character_subject_id(&character_id);
    let seeded = LocalMemoryProvider::open(user_home.to_path_buf(), &creator_id, WORKSPACE)
        .await
        .expect("provider open for the seed");
    let seed_principal = principal(&creator_id);
    ensure_kind(
        &seeded,
        &seed_principal,
        &creator_subject,
        legacy::CREATOR_KIND_NS,
        &creator_id,
    )
    .await;
    ensure_kind(
        &seeded,
        &seed_principal,
        &character_subject,
        legacy::CHARACTER_KIND_NS,
        &character_id,
    )
    .await;
    for (subject, key, body) in [
        (&creator_subject, "c115-seed-creator", CREATOR_BODY),
        (&character_subject, "c115-seed-character", CHARACTER_BODY),
    ] {
        let created = wire(
            &seeded
                .create_record(
                    &seed_principal,
                    create_request_for(subject, key, "nexus.custom", body),
                )
                .await,
        );
        assert_eq!(created["status"], "committed", "{created}");
    }
    drop(seeded);

    (creator_id, world, character_id, binding_id)
}

/// The admitted state one local refusal scenario observes: the attached Host,
/// both sessions and the subjects whose durable state must not move.
struct C115Admitted<'a> {
    handle: &'a nexus_core::HostHandle,
    principal: &'a Principal,
    character_session: String,
    legacy_session: String,
    user_home: &'a std::path::Path,
    creator_id: &'a str,
    creator_subject: &'a str,
    character_subject: &'a str,
    adapter: &'a MindFixtureAdapter,
}

/// Every refusal is typed, precedes agent execution, leaves the admitted
/// subjects unchanged, and a plain Character prompt still executes.
async fn c115_refuse_and_control(admitted: &C115Admitted<'_>) {
    // BEFORE / AFTER each refusal, the REAL admitted subjects are unchanged.
    observe_actual_subjects(
        admitted.user_home,
        admitted.creator_id,
        admitted.creator_subject,
        admitted.character_subject,
    )
    .await;
    expect_remember_refusal(
        admitted.handle,
        admitted.principal,
        admitted.character_session.clone(),
        "not_supported",
    )
    .await;
    observe_actual_subjects(
        admitted.user_home,
        admitted.creator_id,
        admitted.creator_subject,
        admitted.character_subject,
    )
    .await;
    expect_remember_refusal(
        admitted.handle,
        admitted.principal,
        admitted.legacy_session.clone(),
        "invalid_input",
    )
    .await;
    observe_actual_subjects(
        admitted.user_home,
        admitted.creator_id,
        admitted.creator_subject,
        admitted.character_subject,
    )
    .await;

    assert_eq!(
        admitted.adapter.executions(),
        0,
        "both refusals precede agent execution"
    );
    assert_eq!(
        admitted.adapter.completions(),
        0,
        "no operation completed on a refusal"
    );

    // A plain Character prompt is the positive control: the adapter really
    // executes and completes one operation. The Host's public response is
    // `started`; no per-operation terminal status is claimed beyond the
    // adapter execution/completion this fixture observes.
    let plain = admitted
        .handle
        .execute(
            admitted.principal,
            admitted.character_session.clone(),
            execute_prompt(false),
        )
        .await
        .expect("a plain Character prompt is still served");
    assert_eq!(plain.status, "started", "the plain prompt was admitted");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while admitted.adapter.executions() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the plain prompt reached the agent adapter"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        admitted.adapter.executions(),
        1,
        "exactly one adapter execution"
    );
    assert_eq!(
        admitted.adapter.completions(),
        1,
        "the adapter emitted its completion event"
    );

    observe_actual_subjects(
        admitted.user_home,
        admitted.creator_id,
        admitted.creator_subject,
        admitted.character_subject,
    )
    .await;
}

#[tokio::test]
async fn c_115_remember_true_refusal_unchanged_local() {
    let tmp = TempDir::new().expect("temp home");
    let user_home = tmp.path().to_path_buf();
    let (creator_id, world, character_id, binding_id) = c115_seed(&user_home).await;
    let creator_subject = legacy::creator_subject_id(&creator_id);
    let character_subject = legacy::character_subject_id(&character_id);

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("reopen the local core");
    let principal: Principal = core.active_principal().await.expect("principal");

    // Deterministic local agent host: no process, no model, no network.
    let adapter = Arc::new(MindFixtureAdapter::default());
    let manager = Arc::new(HostManager::new());
    manager
        .register_provider(
            Arc::clone(&adapter) as Arc<dyn ProviderAdapter>,
            LaunchStrategy::NativeCli {
                command: "n3-local-mind-fixture".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
            },
        )
        .await;
    let host_config = nexus_agent_host::config::AgentHostConfig {
        max_ops_per_session: 4,
        ..Default::default()
    };
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
    let handle = core
        .attach_host(manager, Arc::new(QuietPort))
        .expect("the admitted Host authority attaches");

    let character_session = handle
        .create_session(
            &principal,
            dto::<CreateSessionRequest>(json!({
                "provider_id": PROVIDER,
                "actor_ref": { "actor_kind": "character", "character_id": character_id },
                "viewpoint": { "world_id": world, "binding_id": binding_id },
            })),
        )
        .await
        .expect("the admitted Character session is created");

    // A real admitted non-Character Host session (the legacy no-Actor pair).
    let legacy_session = handle
        .create_session(
            &principal,
            dto::<CreateSessionRequest>(json!({ "provider_id": PROVIDER })),
        )
        .await
        .expect("the admitted non-Character session is created");

    let admitted = C115Admitted {
        handle: &handle,
        principal: &principal,
        character_session: character_session.session_id.clone(),
        legacy_session: legacy_session.session_id.clone(),
        user_home: &user_home,
        creator_id: &creator_id,
        creator_subject: &creator_subject,
        character_subject: &character_subject,
        adapter: &adapter,
    };
    c115_refuse_and_control(&admitted).await;
}
