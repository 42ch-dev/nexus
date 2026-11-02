//! NXM-8 (E11) review-pipeline contract on the provider seam.
//!
//! The review drain classifies with the unchanged deterministic classifier and
//! consumes provider candidates (`memory.candidates.review`): a binding-local
//! Character `PromoteToLongTerm` is coerced to one `fact` fragment through
//! `review_candidate`, only the explicit fragment promotion calls
//! `memory.records.promote` with `target_scope: shared`, the DTO keeps the
//! legacy 0→1 revision sequence, an already-shared fragment keeps its stable
//! conflict, and the legacy `*_pending_review` tables stay write-idle
//! (compass D6).
#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::daemon_api::memory::review_request::ReviewRequest;
use nexus_core::memory_provider::legacy;
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{
    create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams,
};
use sqlx::SqlitePool;
use std::path::PathBuf;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD: &str = "wld_worldA";
/// Classifies as `PromoteToLongTerm` for a `brainstorm` task (high-signal,
/// above the creative minimum length).
const PROMOTE_DIGEST: &str =
    "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    character_id: String,
    binding_id: String,
}

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
    let (character_id, binding_id) = {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        ensure_creator_row(&pool, CREATOR, "Owner").await.unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'wrk', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(WORLD)
        .bind(CREATOR)
        .bind(WORLD)
        .bind(WORLD)
        .execute(&pool)
        .await
        .unwrap();
        let created = create_character_with_initial_binding(
            &pool,
            CreateCharacterParams {
                owner_creator_id: CREATOR,
                display_name: "Ada",
                image_uri: None,
                persona_json: "{}",
                world_id: WORLD,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        (created.character.character_id, created.binding.binding_id)
    };

    Env {
        _tmp: tmp,
        user_home,
        character_id,
        binding_id,
    }
}

async fn open_core(env: &Env) -> (CoreService, nexus_core::Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (core, principal)
}

async fn db_pool(env: &Env) -> SqlitePool {
    let db_path = nexus_home_layout::workspace_state_db_path(&env.user_home, CREATOR, "default");
    nexus_local_db::open_pool(&db_path).await.unwrap()
}

async fn count_rows(pool: &SqlitePool, sql: &str) -> i64 {
    let (count,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .unwrap();
    count
}

/// One provider candidate row as the store holds it: `(status, capture
/// context)`.
async fn provider_candidate(
    pool: &SqlitePool,
    candidate_id: &str,
) -> Option<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT status, capture_context_id FROM memory_candidates WHERE candidate_id = ?",
    )
    .bind(candidate_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// `(kind, content)` of one provider record.
async fn provider_record(
    pool: &SqlitePool,
    record_id: &str,
) -> Option<(String, serde_json::Value)> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind, content_json FROM memory_records WHERE record_id = ?")
            .bind(record_id)
            .fetch_optional(pool)
            .await
            .unwrap();
    row.map(|(kind, content)| (kind, serde_json::from_str(&content).unwrap()))
}

fn capture_request(pending_id: &str, binding_id: &str) -> serde_json::Value {
    serde_json::json!({
        "pending_id": pending_id,
        "session_id": format!("sess_{pending_id}"),
        "binding_id": binding_id,
        "task_kind": "brainstorm",
        "raw_digest": PROMOTE_DIGEST,
        "created_at": "2026-01-01T00:00:01Z",
    })
}

fn dto<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("wire-valid request DTO")
}

/// The E11 coercion: a binding-local Character candidate classified
/// `PromoteToLongTerm` is consumed as one `fact` fragment, never as a record in
/// shared scope — the capture's placement decides, the decision cannot.
#[tokio::test]
async fn binding_local_promote_is_coerced_to_a_fact_fragment() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let character_id = env.character_id.clone();
    let binding_id = env.binding_id.clone();

    core.capture_character_pending_review(
        &principal,
        character_id.clone(),
        dto(capture_request("pend_coerce", &binding_id)),
    )
    .await
    .expect("capture");

    let reviewed = core
        .review_character_memory(
            &principal,
            character_id.clone(),
            dto::<ReviewCharacterMemoryRequest>(serde_json::json!({ "binding_id": binding_id })),
        )
        .await
        .expect("review drains the binding scope");
    assert_eq!(
        reviewed.promoted, 0,
        "a binding-local Promote never promotes"
    );
    assert_eq!(reviewed.fragmented, 1, "it is coerced to the fragment path");
    assert_eq!(reviewed.dropped, 0);
    assert_eq!(reviewed.processed, Some(1));
    assert_eq!(reviewed.has_more, Some(false));

    // The candidate was consumed under the bridge's deterministic id, and the
    // legacy queue row the bridge projected it from was never written.
    let pool = db_pool(&env).await;
    let candidate_id = legacy::character_pending_review_candidate_id("pend_coerce");
    assert_eq!(
        provider_candidate(&pool, &candidate_id).await,
        Some(("consumed".to_string(), Some(binding_id.clone()))),
        "the drain adopts and consumes the projected candidate"
    );
    assert_eq!(
        count_rows(
            &pool,
            "SELECT COUNT(*) FROM character_memory_pending_review"
        )
        .await,
        1,
        "the legacy pending table stays write-idle (D6)"
    );
    assert_eq!(
        count_rows(&pool, "SELECT COUNT(*) FROM character_memory_fragments").await,
        0,
        "the review writes provider records, not legacy fragments"
    );
    pool.close().await;

    // The fragment is the retained `fact` DTO: binding scope, legacy 0→1
    // numbering, existing summary/keywords/TTL/session/pending provenance.
    let scoped = core
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            50,
            0,
        )
        .await
        .expect("list binding fragments");
    assert_eq!(scoped.fragments.len(), 1);
    let fragment = &scoped.fragments[0];
    assert_eq!(
        fragment.revision, 0u64,
        "a new provider fragment projects as DTO 0"
    );
    assert_eq!(
        fragment.binding_id.as_ref().map(|id| id.as_str()),
        Some(binding_id.as_str())
    );
    assert_eq!(fragment.summary.as_str(), PROMOTE_DIGEST);
    assert_eq!(fragment.session_id.as_str(), "sess_pend_coerce");
    assert_eq!(
        fragment.ttl.as_deref(),
        Some("180d"),
        "brainstorm fragment TTL"
    );
    assert!(
        !fragment.keywords.is_empty(),
        "keywords survive the fragment mapping"
    );

    let shared = core
        .list_character_memory_fragments(&principal, character_id, None, 50, 0)
        .await
        .expect("list shared fragments");
    assert!(shared.fragments.is_empty(), "nothing was shared implicitly");

    // The record carries the pending provenance the reroute names.
    let pool = db_pool(&env).await;
    let (kind, content) = provider_record(&pool, fragment.fragment_id.as_str())
        .await
        .expect("the fragment is a provider record");
    assert_eq!(kind, "fact");
    assert_eq!(content["text"], PROMOTE_DIGEST);
    assert_eq!(
        content["structured"]["nexus_legacy"]["pending_id"], "pend_coerce",
        "pending provenance rides the fragment"
    );
    pool.close().await;
}

/// Explicit promotion is `memory.records.promote` with `target_scope: shared`:
/// the DTO's `expected_revision` is the provider CAS value minus one, a stale
/// revision writes nothing, and an already-shared fragment keeps its stable
/// conflict.
#[tokio::test]
async fn explicit_promotion_uses_provider_cas_and_keeps_the_0_to_1_sequence() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let character_id = env.character_id.clone();
    let binding_id = env.binding_id.clone();

    core.capture_character_pending_review(
        &principal,
        character_id.clone(),
        dto(capture_request("pend_promote", &binding_id)),
    )
    .await
    .expect("capture");
    core.review_character_memory(
        &principal,
        character_id.clone(),
        dto::<ReviewCharacterMemoryRequest>(serde_json::json!({ "binding_id": binding_id })),
    )
    .await
    .expect("binding-local review");

    let fragment_id = core
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            50,
            0,
        )
        .await
        .expect("list binding fragments")
        .fragments[0]
        .fragment_id
        .as_str()
        .to_string();

    // Stale DTO revision → the retained `version_mismatch`, zero mutation.
    let stale = core
        .promote_character_fragment(&principal, character_id.clone(), fragment_id.clone(), 7)
        .await
        .expect_err("stale promotion must conflict");
    assert!(
        matches!(&stale, CoreError::ActorConflict { code, .. } if code == "version_mismatch"),
        "got {stale:?}"
    );
    let untouched = core
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            50,
            0,
        )
        .await
        .unwrap();
    assert_eq!(
        untouched.fragments[0].revision, 0u64,
        "no mutation on conflict"
    );

    // DTO 0 → provider revision 1 → promote → provider revision 2 → DTO 1.
    let promoted = core
        .promote_character_fragment(&principal, character_id.clone(), fragment_id.clone(), 0)
        .await
        .expect("promotion on the observed revision");
    assert_eq!(promoted.fragment.fragment_id.as_str(), fragment_id);
    assert_eq!(promoted.fragment.revision, 1u64, "the retained 0→1 step");
    assert!(
        promoted.fragment.binding_id.is_none(),
        "promotion clears the binding placement"
    );

    let pool = db_pool(&env).await;
    let candidate_id = legacy::character_pending_review_candidate_id("pend_promote");
    assert_eq!(
        provider_candidate(&pool, &candidate_id)
            .await
            .map(|(status, _)| status),
        Some("consumed".to_string())
    );
    pool.close().await;

    // Already shared → the retained stable conflict.
    let again = core
        .promote_character_fragment(&principal, character_id.clone(), fragment_id, 1)
        .await
        .expect_err("re-promotion must conflict");
    assert!(
        matches!(&again, CoreError::ActorConflict { code, .. } if code == "character_fragment_already_shared"),
        "got {again:?}"
    );

    // The fragment now lives in the shared scope only.
    let shared = core
        .list_character_memory_fragments(&principal, character_id.clone(), None, 50, 0)
        .await
        .expect("list shared fragments");
    assert_eq!(shared.fragments.len(), 1);
    assert_eq!(shared.fragments[0].revision, 1u64);
    let scoped = core
        .list_character_memory_fragments(&principal, character_id, Some(binding_id), 50, 0)
        .await
        .expect("list binding fragments");
    assert!(scoped.fragments.is_empty());
}

/// The shared/whole-bearer Promote path stays a promotion: a Creator candidate
/// classified `PromoteToLongTerm` becomes a provider record under the
/// task-kind-derived `nexus.<MemoryKind>`, while the legacy pending table keeps
/// its row.
#[tokio::test]
async fn creator_promote_writes_a_provider_record_and_leaves_the_legacy_queue_alone() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    let pool = db_pool(&env).await;
    nexus_local_db::create_pending_review(
        &pool,
        &nexus_local_db::PendingReviewRecord {
            pending_id: "pend_creator".to_string(),
            session_id: "sess_pend_creator".to_string(),
            creator_id: CREATOR.to_string(),
            world_id: Some(WORLD.to_string()),
            task_kind: "brainstorm".to_string(),
            raw_digest: PROMOTE_DIGEST.to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        },
    )
    .await
    .unwrap();
    pool.close().await;

    let reviewed = core
        .review_memory(
            &principal,
            dto::<ReviewRequest>(serde_json::json!({ "creator_id": CREATOR })),
        )
        .await
        .expect("creator review batch");
    assert_eq!(reviewed.promoted, 1);
    assert_eq!(reviewed.fragmented, 0);
    assert_eq!(reviewed.dropped, 0);
    assert_eq!(reviewed.has_more, Some(false));

    let pool = db_pool(&env).await;
    let candidate_id = legacy::pending_review_candidate_id("pend_creator");
    assert_eq!(
        provider_candidate(&pool, &candidate_id).await,
        Some(("consumed".to_string(), None)),
        "the Creator candidate is consumed in shared placement"
    );
    assert_eq!(
        count_rows(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1,
        "the legacy pending table stays write-idle (D6)"
    );
    assert_eq!(
        count_rows(&pool, "SELECT COUNT(*) FROM memory_fragments").await,
        0,
        "a promotion is not a fragment"
    );
    let (kind, content): (String, String) = sqlx::query_as(
        "SELECT kind, content_json FROM memory_records WHERE kind = 'nexus.story_summary'",
    )
    .fetch_one(&pool)
    .await
    .expect("the promoted record");
    assert_eq!(kind, "nexus.story_summary", "task-kind-derived record kind");
    let content: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        content["text"]
            .as_str()
            .is_some_and(|text| text.starts_with("# UNTRUSTED:")),
        "the promoted body keeps the passthrough UNTRUSTED header: {content}"
    );
    pool.close().await;

    // The promoted record is not listed as a fragment (same retained split).
    let fragments = core
        .list_memory_fragments(&principal, None, None, 50)
        .await
        .expect("creator fragment list");
    assert!(fragments.fragments.is_empty());
}
