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

use nexus_contracts::daemon_api::characters::create_character_request::CreateCharacterRequest;
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
/// A second owned world: the cross-Character regression gives another Character
/// its own binding here.
const OTHER_CHARACTER_WORLD: &str = "wld_worldOther";
/// A third owned world: the second binding of the primary Character, an
/// unrelated SOUL-narrative cache scope.
const UNRELATED_WORLD: &str = "wld_worldUnrelated";

async fn seed_world_row(pool: &SqlitePool, world_id: &str) {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'wrk', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(world_id)
    .bind(CREATOR)
    .bind(world_id)
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}
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
        for world in [WORLD, OTHER_CHARACTER_WORLD, UNRELATED_WORLD] {
            seed_world_row(&pool, world).await;
        }
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

// ═══ Fix round 1: R4-F1, R4-F2, R4-F3 ════════════════════════════════════
//
// The explicit promotion is addressed by the id the fragment DTO served, so it
// resolves a legacy-projected id through the bridge and adopts the projection
// inside the promotion's transaction (R4-F1); it mutates only the leasing
// Character's own `fact` fragment (R4-F2); and its retained cache invalidation
// survives a retry after a failed attempt (R4-F3).

/// One unmigrated legacy Character fragment: the row the bridge projects and the
/// review reroute may never write (D6).
async fn seed_legacy_fragment(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: &str,
    fragment_id: &str,
    summary: &str,
) {
    sqlx::query(
        "INSERT INTO character_memory_fragments \
         (fragment_id, session_id, character_id, actor_world_binding_id, keywords, summary, \
          created_at, ttl, revision) \
         VALUES (?, ?, ?, ?, ?, ?, '2026-01-01T00:00:00Z', '30d', 0)",
    )
    .bind(fragment_id)
    .bind(format!("sess_{fragment_id}"))
    .bind(character_id)
    .bind(binding_id)
    .bind(r#"["legacy_keyword"]"#)
    .bind(summary)
    .execute(pool)
    .await
    .unwrap();
}

fn soul_narrative_cache_record(
    character_id: &str,
    binding_id: Option<&str>,
) -> nexus_local_db::CharacterSoulNarrativeRecord {
    nexus_local_db::CharacterSoulNarrativeRecord {
        character_id: character_id.to_string(),
        actor_world_binding_id: binding_id.map(str::to_string),
        narrative: Some("cached".to_string()),
        generated_at: Some("2026-01-01T00:00:00Z".to_string()),
        fragment_count_at_generation: 0,
        max_fragment_created_at_at_generation: None,
        distinct_keyword_count_cache: 0,
        stats_fingerprint: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

/// The Character's cached SOUL narrative scopes, ordered so one assertion can
/// name the whole set.
async fn soul_narrative_scopes(env: &Env, character_id: &str) -> Vec<Option<String>> {
    let pool = db_pool(env).await;
    let mut scopes: Vec<Option<String>> = sqlx::query_as(
        "SELECT actor_world_binding_id FROM character_soul_narratives WHERE character_id = ?",
    )
    .bind(character_id)
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row: (Option<String>,)| row.0)
    .collect();
    pool.close().await;
    scopes.sort();
    scopes
}

/// `(kind, scope_visibility, scope_context_id)` of one provider record.
async fn record_placement(
    pool: &SqlitePool,
    record_id: &str,
) -> Option<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT kind, scope_visibility, scope_context_id FROM memory_records WHERE record_id = ?",
    )
    .bind(record_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// R4-F1: a fragment the list served promotes. The DTO serves the legacy
/// `fragment_id` while the bridge names that row by its deterministic external
/// id, so the promotion resolves the listed id through the bridge and adopts
/// the projection inside its own transaction — and the legacy row stays
/// untouched (D6).
#[tokio::test]
async fn a_listed_legacy_fragment_promotes_through_its_bridge_id() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let character_id = env.character_id.clone();
    let binding_id = env.binding_id.clone();

    let pool = db_pool(&env).await;
    seed_legacy_fragment(
        &pool,
        &character_id,
        &binding_id,
        "hfrag_legacy",
        "A legacy fragment the bridge projects for the list.",
    )
    .await;
    pool.close().await;

    let listed = core
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            50,
            0,
        )
        .await
        .expect("list projected legacy fragments");
    assert_eq!(listed.fragments.len(), 1);
    let listed_id = listed.fragments[0].fragment_id.as_str().to_string();
    assert_eq!(listed_id, "hfrag_legacy", "the DTO serves the legacy id");
    assert_eq!(listed.fragments[0].revision, 0u64);

    let promoted = core
        .promote_character_fragment(&principal, character_id.clone(), listed_id, 0)
        .await
        .expect("a listed legacy fragment promotes");
    assert_eq!(
        promoted.fragment.fragment_id.as_str(),
        "hfrag_legacy",
        "the fragment id is preserved"
    );
    assert_eq!(promoted.fragment.revision, 1u64, "the retained 0→1 step");
    assert!(
        promoted.fragment.binding_id.is_none(),
        "promotion clears the binding placement"
    );

    let pool = db_pool(&env).await;
    assert_eq!(
        record_placement(&pool, &legacy::character_fragment_record_id("hfrag_legacy")).await,
        Some(("fact".to_string(), "shared".to_string(), None)),
        "the projection was adopted under the bridge id and shared"
    );
    let legacy_row: Option<(Option<String>, i64)> = sqlx::query_as(
        "SELECT actor_world_binding_id, revision FROM character_memory_fragments \
         WHERE fragment_id = 'hfrag_legacy'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        legacy_row,
        Some((Some(binding_id.clone()), 0)),
        "the legacy row is untouched (D6)"
    );
    pool.close().await;

    // The shared scope serves it once; the binding scope no longer does (the
    // adopted record wins over the projection).
    let shared = core
        .list_character_memory_fragments(&principal, character_id.clone(), None, 50, 0)
        .await
        .expect("list shared fragments");
    assert_eq!(shared.fragments.len(), 1);
    assert_eq!(shared.fragments[0].fragment_id.as_str(), "hfrag_legacy");
    assert_eq!(shared.fragments[0].revision, 1u64);
    let scoped = core
        .list_character_memory_fragments(&principal, character_id, Some(binding_id), 50, 0)
        .await
        .expect("list binding fragments");
    assert!(scoped.fragments.is_empty());
}

/// R4-F2: the promotion names one Character's lease, so it mutates only that
/// Character's own `fact` fragment. Another Character's binding-local record
/// and a non-fragment record are each refused before any write, and an
/// owner-exact promotion still succeeds.
#[tokio::test]
async fn promotion_refuses_another_characters_record_and_a_non_fragment() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let character_id = env.character_id.clone();
    let binding_id = env.binding_id.clone();

    // Character B owns a binding-local fragment of its own.
    let other = core
        .create_character(
            &principal,
            dto::<CreateCharacterRequest>(serde_json::json!({
                "display_name": "Bea",
                "persona": {},
                "world_id": OTHER_CHARACTER_WORLD,
            })),
        )
        .await
        .expect("second Character");
    let other_character = other.character.character_id.as_str().to_string();
    let other_binding = other.binding.binding_id.as_str().to_string();

    core.capture_character_pending_review(
        &principal,
        other_character.clone(),
        dto(capture_request("pend_other", &other_binding)),
    )
    .await
    .expect("capture for the other Character");
    core.review_character_memory(
        &principal,
        other_character.clone(),
        dto::<ReviewCharacterMemoryRequest>(
            serde_json::json!({ "binding_id": other_binding.clone() }),
        ),
    )
    .await
    .expect("review for the other Character");
    let other_fragment = core
        .list_character_memory_fragments(&principal, other_character, Some(other_binding), 50, 0)
        .await
        .expect("list the other Character's fragments")
        .fragments[0]
        .fragment_id
        .as_str()
        .to_string();

    // A's lease must not reach B's record.
    let crossed = core
        .promote_character_fragment(&principal, character_id.clone(), other_fragment.clone(), 0)
        .await
        .expect_err("another Character's record must be refused");
    assert!(
        matches!(crossed, CoreError::NotFound { .. }),
        "got {crossed:?}"
    );
    let pool = db_pool(&env).await;
    let other_record = record_placement(&pool, &other_fragment).await;
    assert_eq!(
        other_record
            .as_ref()
            .map(|(kind, visibility, _)| (kind.as_str(), visibility.as_str())),
        Some(("fact", "context")),
        "the other Character's record keeps its binding placement"
    );
    pool.close().await;

    // A provider-native non-fragment record under A's own subject is refused by
    // kind, not promoted.
    let pool = db_pool(&env).await;
    sqlx::query(
        "INSERT INTO memory_records \
         (record_id, space_id, subject_id, kind, content_json, scope_visibility, \
          scope_context_id, provenance_json, import_ref_json, occurred_at, tombstone_json, \
          revision, created_at, updated_at) \
         VALUES ('rec_not_a_fragment', 'spc_local_default', ?, 'nexus.story_summary', \
                 '{\"text\":\"summary\"}', 'context', ?, ?, NULL, NULL, NULL, 1, \
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(legacy::character_subject_id(&character_id))
    .bind(&binding_id)
    .bind(format!(
        "{{\"source_context_ids\":[\"{binding_id}\"],\"anchors\":[],\
         \"captured_by\":\"test\",\"captured_at\":\"2026-01-01T00:00:00Z\"}}"
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let wrong_kind = core
        .promote_character_fragment(
            &principal,
            character_id.clone(),
            "rec_not_a_fragment".to_string(),
            0,
        )
        .await
        .expect_err("a non-fragment record must be refused");
    assert!(
        matches!(wrong_kind, CoreError::ActorInput(_)),
        "got {wrong_kind:?}"
    );

    // The owner-exact path still promotes what the lease names.
    core.capture_character_pending_review(
        &principal,
        character_id.clone(),
        dto(capture_request("pend_owner", &binding_id)),
    )
    .await
    .expect("capture for the leasing Character");
    core.review_character_memory(
        &principal,
        character_id.clone(),
        dto::<ReviewCharacterMemoryRequest>(
            serde_json::json!({ "binding_id": binding_id.clone() }),
        ),
    )
    .await
    .expect("review for the leasing Character");
    let owned_fragment = core
        .list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            50,
            0,
        )
        .await
        .expect("list the leasing Character's fragments")
        .fragments[0]
        .fragment_id
        .as_str()
        .to_string();
    let promoted = core
        .promote_character_fragment(&principal, character_id, owned_fragment, 0)
        .await
        .expect("the owner-exact promotion succeeds");
    assert_eq!(promoted.fragment.revision, 1u64);

    let pool = db_pool(&env).await;
    assert_eq!(
        record_placement(&pool, "rec_not_a_fragment").await,
        Some((
            "nexus.story_summary".to_string(),
            "context".to_string(),
            Some(binding_id)
        )),
        "the refused non-fragment record was not written"
    );
    pool.close().await;
}

/// R4-F3: the promotion commits before the retained cache invalidation runs, so
/// a failure there leaves a shared fragment with a stale cache. The retry must
/// finish the invalidation instead of exiting early on `already_shared`.
#[tokio::test]
async fn promotion_retry_finishes_an_interrupted_cache_invalidation() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let character_id = env.character_id.clone();
    let binding_id = env.binding_id.clone();

    // An unrelated real binding of the same Character: a scope the promotion
    // must leave alone.
    let unrelated_binding = core
        .add_binding(
            &principal,
            character_id.clone(),
            UNRELATED_WORLD.to_string(),
            None,
        )
        .await
        .expect("second binding")
        .binding
        .binding_id
        .as_str()
        .to_string();

    core.capture_character_pending_review(
        &principal,
        character_id.clone(),
        dto(capture_request("pend_retry", &binding_id)),
    )
    .await
    .expect("capture");
    core.review_character_memory(
        &principal,
        character_id.clone(),
        dto::<ReviewCharacterMemoryRequest>(
            serde_json::json!({ "binding_id": binding_id.clone() }),
        ),
    )
    .await
    .expect("review");
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

    // Pre-existing cache rows: the two scopes the promotion invalidates and one
    // it must leave alone.
    let pool = db_pool(&env).await;
    for binding in [
        None,
        Some(binding_id.as_str()),
        Some(unrelated_binding.as_str()),
    ] {
        nexus_local_db::upsert_character_soul_narrative(
            &pool,
            CREATOR,
            &soul_narrative_cache_record(&character_id, binding),
        )
        .await
        .unwrap();
    }
    pool.close().await;
    let mut stale = vec![
        None,
        Some(binding_id.clone()),
        Some(unrelated_binding.clone()),
    ];
    stale.sort();
    assert_eq!(soul_narrative_scopes(&env, &character_id).await, stale);

    // The invalidation fails after the promotion already committed.
    core.fail_next_soul_narrative_invalidation();
    let failed = core
        .promote_character_fragment(&principal, character_id.clone(), fragment_id.clone(), 0)
        .await
        .expect_err("the injected invalidation failure must surface");
    assert!(
        matches!(failed, CoreError::Internal { .. }),
        "got {failed:?}"
    );

    // The promotion itself committed; only the cache is stale.
    let shared = core
        .list_character_memory_fragments(&principal, character_id.clone(), None, 50, 0)
        .await
        .expect("list shared fragments");
    assert_eq!(
        shared.fragments.len(),
        1,
        "the promotion committed before the invalidation"
    );
    assert_eq!(shared.fragments[0].revision, 1u64);
    assert_eq!(
        soul_narrative_scopes(&env, &character_id).await,
        stale,
        "the failed invalidation left the cache stale"
    );

    // The retry reaches the retained conflict AND finishes the invalidation.
    let retried = core
        .promote_character_fragment(&principal, character_id.clone(), fragment_id, 1)
        .await
        .expect_err("re-promotion conflicts");
    assert!(
        matches!(&retried, CoreError::ActorConflict { code, .. } if code == "character_fragment_already_shared"),
        "got {retried:?}"
    );
    assert_eq!(
        soul_narrative_scopes(&env, &character_id).await,
        vec![Some(unrelated_binding)],
        "the retry invalidated the shared and old-binding scopes"
    );
    let shared = core
        .list_character_memory_fragments(&principal, character_id, None, 50, 0)
        .await
        .expect("list shared fragments");
    assert_eq!(
        shared.fragments.len(),
        1,
        "one consistent state after the retry"
    );
}
