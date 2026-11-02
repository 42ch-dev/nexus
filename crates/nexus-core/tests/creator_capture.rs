//! Creator explicit-capture parity (NXM-16): capture → review → promote through
//! the typed API.
//!
//! The capture leg is `CoreService::create_pending_review` — the typed Creator
//! capability the napi binding, the service route and `nexus42 creator memory
//! capture` wrap. It writes the provider candidate the review family consumes;
//! the review/promotion legs below drive that candidate through the pinned
//! `MemoryPort` (the same port the deployment review drain uses), because the
//! drain reroute itself is NXM-8. The promoted record is read back through the
//! typed Creator long-term-memory read.
//!
//! Explicit capture only: Host auto-`remember` stays refused (D-4 / C-115), and
//! this target never sets it.

use nexus_contracts::generated::daemon_api::memory::create_pending_review_request::CreatePendingReviewRequest;
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, Principal};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::ensure_creator_row;
use serde_json::{json, Value};
use spoke_operations::{spoke_schemas, MemoryPort, SpokeResult};
use std::path::PathBuf;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PENDING: &str = "pend_capture_1";
const SESSION: &str = "sess_capture_1";
const WORLD: &str = "wld_capture";
const CREATED_AT: &str = "2026-11-02T10:00:00Z";
const DIGEST: &str =
    "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
}

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid payload")
}

/// The wire body of the one capture this file performs.
fn capture_body(pending_id: &str, creator_id: &str) -> CreatePendingReviewRequest {
    dto(json!({
        "pending_id": pending_id,
        "session_id": SESSION,
        "creator_id": creator_id,
        "world_id": WORLD,
        "task_kind": "chapter",
        "raw_digest": DIGEST,
        "created_at": CREATED_AT,
    }))
}

/// Materialize the workspace shell and seed the creator row through one engine
/// pool, released before the cores open (mirroring the daemon boot).
async fn seed_env() -> Env {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home, CREATOR, "default",
    ))
    .unwrap();
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"default\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, "default");
    {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        ensure_creator_row(&pool, CREATOR, "Owner").await.unwrap();
        pool.close().await;
    }
    Env {
        _tmp: tmp,
        user_home,
    }
}

async fn open_core(env: &Env) -> (CoreService, Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (core, principal)
}

fn spoke_principal() -> spoke_schemas::Principal {
    dto(json!({ "principal_id": CREATOR, "extensions": {} }))
}

fn wire<T: serde::Serialize>(result: &SpokeResult<T>) -> Value {
    match result {
        SpokeResult::Ok(value) => serde_json::to_value(value).expect("serializable response"),
        SpokeResult::Reject(reject) => json!({
            "error": { "code": reject.code.as_str(), "message": reject.message },
        }),
    }
}

/// The single pending candidate the capture wrote, as the provider serves it.
async fn pending_candidates(core: &CoreService) -> Vec<Value> {
    let provider = core.memory_provider_for_test().expect("pinned provider");
    let body = wire(
        &provider
            .list_candidates(
                &spoke_principal(),
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": nexus_core::memory_provider::legacy::creator_subject_id(CREATOR),
                    "status": "pending",
                })),
            )
            .await,
    );
    body["candidates"].as_array().cloned().unwrap_or_default()
}

/// Capture → review → promote: the captured digest becomes a promoted Creator
/// record, and the pending row is consumed exactly once.
#[tokio::test]
async fn creator_capture_review_promote_round_trip() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // 1. capture: the typed Creator capability answers the retained DTO.
    let response = core
        .create_pending_review(&principal, capture_body(PENDING, CREATOR))
        .await
        .expect("creator capture");
    assert!(response.success);
    assert_eq!(response.pending_id, PENDING);

    // 2. the captured row is the Creator pending-review DTO's row, with every
    // captured field preserved.
    let pending = core
        .list_pending_reviews(&principal, None, 50)
        .await
        .expect("list pending reviews");
    assert_eq!(pending.items.len(), 1, "one captured row");
    let item = &pending.items[0];
    assert_eq!(item.pending_id, PENDING);
    assert_eq!(item.session_id, SESSION);
    assert_eq!(item.creator_id, CREATOR);
    assert_eq!(item.world_id.as_deref(), Some(WORLD));
    assert_eq!(item.task_kind, "chapter");
    assert_eq!(item.raw_digest, DIGEST);
    assert_eq!(item.created_at, CREATED_AT);
    assert_eq!(core.count_pending_reviews(&principal).await.unwrap().count, 1);

    // 3. an exact retry replays the capture instead of queueing a second row.
    let retry = core
        .create_pending_review(&principal, capture_body(PENDING, CREATOR))
        .await
        .expect("capture retry replays");
    assert_eq!(retry.pending_id, PENDING);
    assert_eq!(core.count_pending_reviews(&principal).await.unwrap().count, 1);

    // 4. the capture is a provider candidate carrying the task-kind mapping
    // (`chapter` → the `story_summary` authored kind) and the world context.
    let candidates = pending_candidates(&core).await;
    assert_eq!(candidates.len(), 1, "one pending candidate");
    let candidate = &candidates[0];
    assert_eq!(candidate["status"], "pending");
    assert_eq!(candidate["proposed"]["kind"], "nexus.story_summary");
    assert_eq!(
        candidate["proposed"]["content"]["structured"]["nexus_legacy"]["pending_id"],
        PENDING
    );
    assert_eq!(
        candidate["provenance"]["source_context_ids"],
        json!([WORLD]),
        "world_id is the provenance context"
    );
    assert_eq!(
        candidate["provenance"]["anchors"][0]["source_id"], SESSION,
        "session_id is the source anchor"
    );
    let candidate_id = candidate["candidate_id"]
        .as_str()
        .expect("a provider candidate names itself")
        .to_string();

    // 5. review + promote through the pinned port.
    let provider = core.memory_provider_for_test().expect("pinned provider");
    let reviewed = wire(
        &provider
            .review_candidate(
                &spoke_principal(),
                dto(json!({
                    "space_id": provider.space_id(),
                    "candidate_id": candidate_id,
                    "expected_revision": 1,
                    "decision": { "action": "promote" },
                    "idempotency_key": "k-creator-capture-review",
                })),
            )
            .await,
    );
    let reviewed = &reviewed["result"];
    assert_eq!(reviewed["outcome"], "promoted");
    assert_eq!(
        reviewed["record_ids"].as_array().map(Vec::len),
        Some(1),
        "the promotion created exactly one record"
    );

    // 6. the promoted record is the Creator's long-term memory now, and the
    // queue is consumed.
    let records = core
        .list_creator_memories(&principal)
        .await
        .expect("list creator memories");
    assert_eq!(records.len(), 1, "the promoted record is listed");
    let promoted = serde_json::to_value(&records[0]).expect("serializable record");
    assert_eq!(promoted["kind"], "nexus.story_summary");
    assert_eq!(promoted["content"]["text"], DIGEST);
    assert_eq!(core.count_pending_reviews(&principal).await.unwrap().count, 0);
    assert_eq!(pending_candidates(&core).await.len(), 0);
}

/// The capture refuses another creator's identity with the retained ownership
/// code and writes nothing.
#[tokio::test]
async fn creator_capture_refuses_a_foreign_creator() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    let error = core
        .create_pending_review(&principal, capture_body(PENDING, "ctr_foreign"))
        .await
        .expect_err("a foreign creator_id must be refused");
    match error {
        CoreError::ForbiddenReason { resource, .. } => assert_eq!(resource, "pending_review"),
        other => panic!("expected the pending_review refusal, got {other:?}"),
    }
    assert_eq!(core.count_pending_reviews(&principal).await.unwrap().count, 0);
}

/// The retained capture-field limits stay handler-owned: an empty digest and a
/// reserved `run_` pending id are `invalid_input`, and nothing is queued.
#[tokio::test]
async fn creator_capture_validates_its_fields() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    let mut empty_digest = capture_body(PENDING, CREATOR);
    empty_digest.raw_digest = String::new();
    let error = core
        .create_pending_review(&principal, empty_digest)
        .await
        .expect_err("an empty digest must be refused");
    assert!(
        matches!(error, CoreError::ActorInput(_)),
        "expected invalid_input, got {error:?}"
    );

    let reserved = capture_body("run_reserved_1", CREATOR);
    let error = core
        .create_pending_review(&principal, reserved)
        .await
        .expect_err("the run_ prefix stays reserved");
    assert!(
        matches!(error, CoreError::ActorInput(_)),
        "expected invalid_input, got {error:?}"
    );

    assert_eq!(core.count_pending_reviews(&principal).await.unwrap().count, 0);
}
