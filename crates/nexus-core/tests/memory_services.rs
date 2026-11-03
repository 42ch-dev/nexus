//! P2-T2/P2-T7 bearer memory services contract tests (core service semantics):
//! revision-checked fragment promotion is atomic and cache-scoped (ported
//! from the daemon `character_memory_api.rs` promotion anchor), both memory
//! arms share the classification while their storage stays bearer-isolated
//! (ported from the daemon dual-arm semantic suite), and the SOUL reflect
//! state machine follows the fragment gate and the per-bearer cache without
//! ever synthesizing in the background.
//!
//! P2-T7 additions (ported from the daemon `memory_pagination_bounded.rs`,
//! `memory_review_fragments_api.rs`, `memory_dto_roundtrip.rs` and the
//! remaining `character_memory_api.rs` lifecycle cases): the Creator
//! pending-review keyset walk pages the whole queue once with no gap or
//! overlap (what `creator memory pending-show` walks past page one), the
//! Creator and Character review drains stay bounded at the batch limit and
//! report remaining rows truthfully when a row's action fails, the Creator
//! fragment list is bounded and projects only its public fields with an exact
//! `world_id` filter, and the Character pending lifecycle keeps its shared /
//! binding scopes separate across capture, list, count, delete and the offset
//! page walk.
#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::daemon_api::memory::review_request::ReviewRequest;
use nexus_contracts::generated::core::{
    CoreCharacterTransitionRequest, CoreCharacterTransitionRequestTargetStatus,
};
use nexus_core::{
    AdmittedActor, CoreAccess, CoreCharacterMind, CoreError, CoreOpenOptions, CoreService,
    MemoryReviewPause, MemoryReviewRendezvous,
};
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::soul_narrative::{
    SoulNarrativeDraft, SoulNarrativeSynthesisInput, SoulNarrativeSynthesizer,
};
use nexus_local_db::writer_protocol::{init_engine_pool, init_guarded_pool, GuardedPoolOptions};
use nexus_local_db::{
    create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams,
};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";

const PROMOTE_DIGEST: &str =
    "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";
const FRAGMENT_DIGEST: &str = "Research summary long enough to classify as a fragment rather than being dropped for shortness.";
const DROP_DIGEST: &str = "Too short.";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    db_path: PathBuf,
    character_id: String,
    binding_id: String,
}

fn assert_conflict(err: CoreError, code: &str) {
    match err {
        CoreError::ActorConflict { code: got, .. } => assert_eq!(got, code, "conflict code"),
        other => panic!("expected ActorConflict {code}, got {other:?}"),
    }
}

async fn seed_world(pool: &SqlitePool, world_id: &str, owner: &str) {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'wrk', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(world_id)
    .bind(owner)
    .bind(world_id)
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Materialize the workspace shell and seed stored rows through one engine
/// pool (released before the cores open, mirroring the daemon boot).
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
        ensure_creator_row(&pool, OTHER, "Other").await.unwrap();
        seed_world(&pool, WORLD, CREATOR).await;
        seed_world(&pool, WORLD_B, CREATOR).await;
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
        db_path,
        character_id,
        binding_id,
    }
}

/// Materialize only the workspace shell and an **empty** state database: the
/// pre-provider lifecycle a read-only core can open before any writer has
/// migrated or provisioned it (fix round 3, finding F2).
fn seed_unprovisioned_env() -> Env {
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
    std::fs::write(&db_path, b"").unwrap();
    Env {
        _tmp: tmp,
        user_home,
        db_path,
        character_id: String::new(),
        binding_id: String::new(),
    }
}

/// Open one engine-owner core against the seeded workspace.
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

async fn plain_pool(env: &Env) -> SqlitePool {
    nexus_local_db::open_pool(&env.db_path).await.unwrap()
}

async fn count_where(pool: &SqlitePool, sql: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .unwrap();
    n
}

/// The provider `fact` record bodies of one subject, in insertion order.
async fn provider_fact_bodies(pool: &SqlitePool, subject_id: &str) -> Vec<String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT content_json FROM memory_records WHERE kind = 'fact' AND subject_id = ? \
         ORDER BY created_at, record_id",
    )
    .bind(subject_id)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(content,)| {
            serde_json::from_str::<serde_json::Value>(&content).unwrap()["text"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

fn dto<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("wire-valid request DTO")
}

struct MockSynth;
// Trait-required `async`; the test double performs no awaits.
#[allow(clippy::unused_async_trait_impl)]
impl SoulNarrativeSynthesizer for MockSynth {
    async fn synthesize(
        &self,
        _: MemoryBearerRef<'_>,
        _: SoulNarrativeSynthesisInput,
        _: Option<&str>,
    ) -> Result<SoulNarrativeDraft, MemoryError> {
        Ok(SoulNarrativeDraft {
            narrative: "A reflective turn toward kw_1 and kw_2. What will happen next?".to_string(),
        })
    }
}

/// Ported from the daemon `character_memory_api.rs` promotion anchor: a
/// binding-local fragment promotes only on the exact revision — a stale
/// revision writes nothing, success clears the binding provenance, bumps the
/// revision and invalidates exactly the affected cache scopes (shared + old
/// binding), and re-promotion of an already-shared fragment is a stable
/// conflict. Creator memory remains untouched throughout.
#[tokio::test]
async fn promotion_is_revision_checked_atomic_and_cache_scoped() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let bind1 = env.binding_id.clone();
    // Unrelated cache scope: a real second binding of the same Character (the
    // cache upsert admits binding provenance, so a fabricated id is refused).
    let bind2 = core
        .add_binding(&principal, chr.clone(), WORLD_B.to_string(), None)
        .await
        .expect("second binding")
        .binding
        .binding_id
        .as_str()
        .to_string();

    core.capture_character_pending_review(
        &principal,
        chr.clone(),
        dto(json_req(&bind1, "pend_local")),
    )
    .await
    .expect("capture");

    core.review_character_memory(
        &principal,
        chr.clone(),
        dto::<ReviewCharacterMemoryRequest>(serde_json::json!({ "binding_id": bind1 })),
    )
    .await
    .expect("review drains the binding scope");

    let fragments = core
        .list_character_memory_fragments(&principal, chr.clone(), Some(bind1.clone()), 50, 0)
        .await
        .expect("list binding fragments");
    assert_eq!(fragments.fragments.len(), 1);
    let fragment = &fragments.fragments[0];
    let fragment_id = fragment.fragment_id.as_str().to_string();
    assert_eq!(fragment.revision, 0u64);
    assert_eq!(
        fragment.binding_id.as_ref().map(|b| b.as_str()),
        Some(bind1.as_str())
    );

    // Pre-existing cache rows: the promoted scope (shared + old binding) and
    // an unrelated scope that must survive the invalidation.
    let pool = plain_pool(&env).await;
    let cache_seed = |binding: Option<&str>| nexus_local_db::CharacterSoulNarrativeRecord {
        character_id: chr.clone(),
        actor_world_binding_id: binding.map(str::to_string),
        narrative: Some("cached".to_string()),
        generated_at: Some("2026-01-01T00:00:00Z".to_string()),
        fragment_count_at_generation: 0,
        max_fragment_created_at_at_generation: None,
        distinct_keyword_count_cache: 0,
        stats_fingerprint: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    nexus_local_db::upsert_character_soul_narrative(&pool, CREATOR, &cache_seed(Some(&bind1)))
        .await
        .unwrap();
    nexus_local_db::upsert_character_soul_narrative(&pool, CREATOR, &cache_seed(Some(&bind2)))
        .await
        .unwrap();
    pool.close().await;

    // Stale revision → stable conflict, zero mutation.
    let err = core
        .promote_character_fragment(&principal, chr.clone(), fragment_id.clone(), 7)
        .await
        .expect_err("stale promotion must conflict");
    assert_conflict(err, "version_mismatch");
    let after = core
        .list_character_memory_fragments(&principal, chr.clone(), Some(bind1.clone()), 50, 0)
        .await
        .unwrap();
    assert_eq!(after.fragments.len(), 1, "stale promotion must not mutate");
    assert_eq!(after.fragments[0].revision, 0u64);

    // Correct revision → same id, provenance cleared, revision bumped.
    let promoted = core
        .promote_character_fragment(&principal, chr.clone(), fragment_id.clone(), 0)
        .await
        .expect("promotion on the observed revision");
    let promoted_fragment = &promoted.fragment;
    assert_eq!(promoted_fragment.fragment_id.as_str(), fragment_id);
    assert_eq!(promoted_fragment.revision, 1u64);
    assert!(
        promoted_fragment.binding_id.is_none(),
        "promotion clears binding provenance"
    );

    // The fragment now lives in the shared scope only.
    let shared = core
        .list_character_memory_fragments(&principal, chr.clone(), None, 50, 0)
        .await
        .unwrap();
    assert_eq!(shared.fragments.len(), 1);
    let scoped = core
        .list_character_memory_fragments(&principal, chr.clone(), Some(bind1.clone()), 50, 0)
        .await
        .unwrap();
    assert_eq!(scoped.fragments.len(), 0);

    // Cache invalidation is scope-exact: shared + old binding rows are gone,
    // the unrelated scope survives.
    let pool = plain_pool(&env).await;
    let caches: Vec<Option<String>> = sqlx::query_as(
        "SELECT actor_world_binding_id FROM character_soul_narratives WHERE character_id = ?",
    )
    .bind(&chr)
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row: (std::option::Option<String>,)| row.0)
    .collect();
    pool.close().await;
    assert_eq!(caches, vec![Some(bind2)]);

    // Re-promotion of an already-shared fragment → stable conflict.
    let err = core
        .promote_character_fragment(&principal, chr.clone(), fragment_id, 1)
        .await
        .expect_err("re-promotion must conflict");
    assert_conflict(err, "character_fragment_already_shared");

    // Creator memory remains untouched by the Character journey.
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_fragments").await,
        0
    );
    pool.close().await;
    let nexus_root = nexus_home_layout::nexus_root_from_home(&env.user_home);
    let creator_dir = MemoryBearerRef::Creator(CREATOR).long_term_memory_dir(&nexus_root);
    assert!(
        !creator_dir.exists(),
        "Creator memory dir must not be created by Character promotion"
    );
}

/// Ported from the daemon dual-arm semantic suite: both memory arms share one
/// classification (promote / fragment / drop) while their storage stays
/// bearer-isolated — distinct tables, distinct long-term-memory directories,
/// and no row ever crossing to another creator.
#[tokio::test]
async fn review_both_arms_share_classification_and_isolate_storage() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();

    // Seed both queues with the same three digests.
    for (id, digest, kind) in [
        ("c_p", PROMOTE_DIGEST, "brainstorm"),
        ("c_f", FRAGMENT_DIGEST, "research"),
        ("c_d", DROP_DIGEST, "unknown"),
    ] {
        let pool = plain_pool(&env).await;
        nexus_local_db::create_pending_review(
            &pool,
            &nexus_local_db::PendingReviewRecord {
                pending_id: id.to_string(),
                session_id: format!("sess_{id}"),
                creator_id: CREATOR.to_string(),
                world_id: None,
                task_kind: kind.to_string(),
                raw_digest: digest.to_string(),
                created_at: "2026-01-01T00:00:01Z".to_string(),
            },
        )
        .await
        .unwrap();
        nexus_local_db::create_character_pending_review(
            &pool,
            CREATOR,
            &nexus_local_db::CharacterPendingReviewRecord {
                pending_id: id.to_string(),
                session_id: format!("sess_{id}"),
                character_id: chr.clone(),
                actor_world_binding_id: None,
                task_kind: kind.to_string(),
                raw_digest: digest.to_string(),
                created_at: "2026-01-01T00:00:01Z".to_string(),
                source_operation_id: None,
            },
        )
        .await
        .unwrap();
        pool.close().await;
    }

    let creator_out = core
        .review_memory(
            &principal,
            dto::<ReviewRequest>(serde_json::json!({ "creator_id": CREATOR })),
        )
        .await
        .expect("creator review batch");
    assert_eq!(creator_out.promoted, 1);
    assert_eq!(creator_out.fragmented, 1);
    assert_eq!(creator_out.dropped, 1);

    let char_out = core
        .review_character_memory(
            &principal,
            chr.clone(),
            dto::<ReviewCharacterMemoryRequest>(serde_json::json!({})),
        )
        .await
        .expect("character review batch");
    assert_eq!(char_out.promoted, 1);
    assert_eq!(char_out.fragmented, 1);
    assert_eq!(char_out.dropped, 1);

    // Both queues drain through the provider: every candidate is adopted under
    // the bridge's deterministic id and consumed in place, and the review writes
    // provider records only — the legacy pending and fragment tables stay
    // write-idle (compass D6).
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        3,
        "the Creator legacy queue is write-idle"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM character_memory_pending_review"
        )
        .await,
        3,
        "the Character legacy queue is write-idle"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_fragments").await,
        0,
        "a review writes provider records, not legacy fragments"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_memory_fragments").await,
        0
    );
    let creator_subject = legacy::creator_subject_id(CREATOR);
    let character_subject = legacy::character_subject_id(&chr);
    for subject in [creator_subject.as_str(), character_subject.as_str()] {
        let facts: Vec<(String,)> = sqlx::query_as(
            "SELECT content_json FROM memory_records WHERE kind = 'fact' AND subject_id = ?",
        )
        .bind(subject)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            facts.len(),
            1,
            "one fragment record per subject ({subject})"
        );
        let promoted: Vec<(String,)> = sqlx::query_as(
            "SELECT content_json FROM memory_records WHERE kind = 'nexus.story_summary' \
             AND subject_id = ?",
        )
        .bind(subject)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            promoted.len(),
            1,
            "one promoted record per subject ({subject})"
        );
    }
    pool.close().await;

    // Every queue entry — promoted, fragmented and dropped alike — was consumed
    // under the bridge's deterministic id on both arms.
    for pending_id in ["c_p", "c_f", "c_d"] {
        for candidate_id in [
            legacy::pending_review_candidate_id(pending_id),
            legacy::character_pending_review_candidate_id(pending_id),
        ] {
            assert_eq!(
                adopted_candidate(&env, &candidate_id)
                    .await
                    .map(|(status, _)| status),
                Some("consumed".to_string()),
                "{candidate_id} is consumed by the drain"
            );
        }
    }
}

fn json_req(binding_id: &str, pending_id: &str) -> serde_json::Value {
    serde_json::json!({
        "pending_id": pending_id,
        "session_id": format!("sess_{pending_id}"),
        "binding_id": binding_id,
        "task_kind": "research",
        "raw_digest": FRAGMENT_DIGEST,
        "created_at": "2026-01-01T00:00:01Z",
    })
}

/// Ported reflect states: below the fragment gate both arms report
/// `insufficient_data` without calling a synthesizer; above the gate but
/// never synthesized is `ungenerated`; an explicitly forced reflect with a
/// host-supplied provider synthesizes and caches per bearer; a missing
/// provider on a forced reflect is the retained truthful error — never
/// background synthesis.
#[tokio::test]
async fn reflect_states_follow_the_gate_cache_and_provider_presence() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();

    // Empty scope → insufficient_data on both arms.
    let read = core
        .reflect_character_soul::<MockSynth>(
            &principal,
            chr.clone(),
            dto(serde_json::json!({ "force_regenerate": false })),
            || None::<MockSynth>,
        )
        .await
        .unwrap();
    assert_eq!(read.state.to_string(), "insufficient_data");
    assert_eq!(read.current_fragment_count, 0);
    assert_eq!(read.current_distinct_keyword_count, 0);
    assert_eq!(
        read.min_fragment_count, 10,
        "documented insufficient-data fragment gate"
    );
    assert_eq!(
        read.min_distinct_keyword_count, 20,
        "documented insufficient-data distinct-keyword gate"
    );
    assert!(!read.stale);
    let creator_read = core
        .reflect_creator_soul::<MockSynth>(
            &principal,
            dto(serde_json::json!({ "creator_id": CREATOR, "force_regenerate": false })),
            || None::<MockSynth>,
        )
        .await
        .unwrap();
    assert_eq!(creator_read.state.to_string(), "insufficient_data");

    // Zero-write read contract: even with the gate unmet, a non-forced
    // reflect must leave both cache tables completely untouched (no
    // stats-only rows, no fingerprint rows) — before any synthesis exists.
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_soul_narratives").await,
        0
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_soul_narratives").await,
        0
    );
    pool.close().await;

    // Above the data gate but never synthesized → ungenerated.
    seed_fragments(&env, &chr).await;
    let ungenerated = core
        .reflect_character_soul::<MockSynth>(
            &principal,
            chr.clone(),
            dto(serde_json::json!({ "force_regenerate": false })),
            || None::<MockSynth>,
        )
        .await
        .unwrap();
    assert_eq!(ungenerated.state.to_string(), "ungenerated");
    assert_eq!(ungenerated.current_fragment_count, 12);

    // The fingerprint-mismatch read (fragments present, no cache row) is
    // still zero-write on both bearers.
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_soul_narratives").await,
        0
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_soul_narratives").await,
        0
    );
    pool.close().await;

    // Forced reflect without a provider → retained truthful 503 carrier.
    let err = core
        .reflect_character_soul::<MockSynth>(
            &principal,
            chr.clone(),
            dto(serde_json::json!({ "force_regenerate": true })),
            || None::<MockSynth>,
        )
        .await
        .expect_err("forced reflect without a provider is a truthful error");
    assert!(
        matches!(&err, CoreError::ServiceUnavailable(m) if m.contains("capability registry not available")),
        "got {err:?}"
    );
    // …and nothing was synthesized in the background.
    let still = core
        .reflect_character_soul::<MockSynth>(
            &principal,
            chr.clone(),
            dto(serde_json::json!({ "force_regenerate": false })),
            || None::<MockSynth>,
        )
        .await
        .unwrap();
    assert_eq!(still.state.to_string(), "ungenerated");
    assert!(still.narrative.is_none());

    // Authorization precedes provider resolution: a forced reflect on a
    // foreign Character must return the retained 404 with the provider
    // factory NEVER invoked — the panicking factory is the proof.
    let err = core
        .reflect_character_soul::<MockSynth>(
            &principal,
            "chr_0000000000000000000000000000ffff".to_string(),
            dto(serde_json::json!({ "force_regenerate": true })),
            || panic!("provider factory must not run before authorization"),
        )
        .await
        .expect_err("foreign character must 404 before provider handling");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");

    // Explicit synthesis through the host-supplied provider caches per bearer.
    let current = core
        .reflect_character_soul(
            &principal,
            chr.clone(),
            dto(serde_json::json!({ "force_regenerate": true })),
            || Some(MockSynth),
        )
        .await
        .unwrap();
    assert_eq!(current.state.to_string(), "current");
    assert!(current.narrative.is_some());
    let creator_current = core
        .reflect_creator_soul(
            &principal,
            dto(serde_json::json!({ "creator_id": CREATOR, "force_regenerate": true })),
            || Some(MockSynth),
        )
        .await
        .unwrap();
    assert_eq!(creator_current.state.to_string(), "current");

    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_soul_narratives").await,
        1
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_soul_narratives").await,
        1,
        "the Creator cache stays separate from the Character cache"
    );
    pool.close().await;

    // A fragment arriving after the persisted synthesis diverges the stats
    // fingerprint → the retained `stale` state: the cached narrative is
    // returned flagged stale, with no provider consulted and no write.
    seed_character_fragment(&env, &chr, 12).await;
    let stale = core
        .reflect_character_soul::<MockSynth>(
            &principal,
            chr.clone(),
            dto(serde_json::json!({ "force_regenerate": false })),
            || None::<MockSynth>,
        )
        .await
        .unwrap();
    assert_eq!(stale.state.to_string(), "stale");
    assert!(
        stale.stale,
        "the stale flag follows the divergent fingerprint"
    );
    assert_eq!(stale.current_fragment_count, 13);
    assert!(
        stale.narrative.is_some(),
        "the cached narrative is retained by a stale read"
    );
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_soul_narratives").await,
        1,
        "a stale read is zero-write"
    );
    pool.close().await;
}

/// Seed one shared-scope Character fragment (index `i`) with distinct
/// keywords, used to move the SOUL stats fingerprint off a persisted
/// narrative.
async fn seed_character_fragment(env: &Env, chr: &str, i: usize) {
    let pool = plain_pool(env).await;
    nexus_local_db::create_character_fragment(
        &pool,
        CREATOR,
        &nexus_local_db::NewCharacterMemoryFragment {
            fragment_id: format!("hfrag_{i:02}"),
            session_id: format!("hsess_{i:02}"),
            character_id: chr.to_string(),
            actor_world_binding_id: None,
            keywords: serde_json::json!([format!("kw_{i}"), format!("kx_{i}")]).to_string(),
            summary: format!("Character summary {i} about kw_{i} and kx_{i}."),
            created_at: format!("2026-01-02T00:00:{i:02}Z"),
            ttl: None,
        },
    )
    .await
    .unwrap();
    pool.close().await;
}

/// Seed 12 fragments with 24 distinct keywords on each bearer: above the
/// insufficient-data gate (10 fragments / 20 distinct keywords).
async fn seed_fragments(env: &Env, chr: &str) {
    let pool = plain_pool(env).await;
    for i in 0..12 {
        nexus_local_db::memory_fragment::create_fragment(
            &pool,
            &nexus_local_db::memory_fragment::MemoryFragmentRecord {
                fragment_id: format!("cfrag_{i:02}"),
                session_id: format!("csess_{i:02}"),
                creator_id: CREATOR.to_string(),
                keywords: serde_json::json!([format!("kw_{i}"), format!("kx_{i}")]).to_string(),
                summary: format!("Creator summary {i} about kw_{i} and kx_{i}."),
                created_at: format!("2026-01-02T00:00:{i:02}Z"),
                ttl: None,
                world_id: None,
            },
        )
        .await
        .unwrap();
        nexus_local_db::create_character_fragment(
            &pool,
            CREATOR,
            &nexus_local_db::NewCharacterMemoryFragment {
                fragment_id: format!("hfrag_{i:02}"),
                session_id: format!("hsess_{i:02}"),
                character_id: chr.to_string(),
                actor_world_binding_id: None,
                keywords: serde_json::json!([format!("kw_{i}"), format!("kx_{i}")]).to_string(),
                summary: format!("Character summary {i} about kw_{i} and kx_{i}."),
                created_at: format!("2026-01-02T00:00:{i:02}Z"),
                ttl: None,
            },
        )
        .await
        .unwrap();
    }
    pool.close().await;
}

// ── Creator pending-review keyset pagination (P2-T7) ──────────────────────

/// Rows seeded for the Creator cursor walks — far above every page size used
/// below, so the server-side bound is exercised on every page.
const SEED_COUNT: usize = 60;
/// Page size across the walks (divides `SEED_COUNT` evenly).
const PAGE_SIZE: usize = 10;

/// Seed `count` Creator pending rows with strictly increasing `created_at`
/// (row `i` is older than row `i + 1`), so `created_at DESC, pending_id DESC`
/// is a total order and page N is exactly the N-th window of the seed.
async fn seed_creator_pending(env: &Env, prefix: &str, count: usize) {
    let pool = plain_pool(env).await;
    for i in 0..count {
        nexus_local_db::create_pending_review(
            &pool,
            &nexus_local_db::PendingReviewRecord {
                pending_id: format!("{prefix}_{i:03}"),
                session_id: format!("sess_{prefix}_{i:03}"),
                creator_id: CREATOR.to_string(),
                world_id: None,
                task_kind: "research".to_string(),
                raw_digest: FRAGMENT_DIGEST.to_string(),
                created_at: format!("2026-01-01T00:{i:02}:00Z"),
            },
        )
        .await
        .unwrap();
    }
    pool.close().await;
}

/// Seed one Creator pending row with an explicit world provenance.
async fn seed_creator_pending_row(
    env: &Env,
    pending_id: &str,
    world_id: Option<&str>,
    created_at: &str,
) {
    let pool = plain_pool(env).await;
    nexus_local_db::create_pending_review(
        &pool,
        &nexus_local_db::PendingReviewRecord {
            pending_id: pending_id.to_string(),
            session_id: format!("sess_{pending_id}"),
            creator_id: CREATOR.to_string(),
            world_id: world_id.map(str::to_string),
            task_kind: "research".to_string(),
            raw_digest: FRAGMENT_DIGEST.to_string(),
            created_at: created_at.to_string(),
        },
    )
    .await
    .unwrap();
    pool.close().await;
}

/// Seed `count` Creator fragments with distinct `created_at` (row `i` older
/// than row `i + 1`, so the newest seeded fragment is returned first).
async fn seed_creator_fragments(env: &Env, count: usize) {
    let pool = plain_pool(env).await;
    for i in 0..count {
        nexus_local_db::memory_fragment::create_fragment(
            &pool,
            &nexus_local_db::memory_fragment::MemoryFragmentRecord {
                fragment_id: format!("frag_seeded_{i:03}"),
                session_id: format!("sess_seeded_{i:03}"),
                creator_id: CREATOR.to_string(),
                keywords: serde_json::json!([format!("kw_{i}")]).to_string(),
                summary: format!("Seeded creator fragment {i} about kw_{i}."),
                created_at: format!("2026-02-01T00:{i:02}:00Z"),
                ttl: None,
                world_id: None,
            },
        )
        .await
        .unwrap();
    }
    pool.close().await;
}

fn creator_review_request() -> ReviewRequest {
    dto::<ReviewRequest>(serde_json::json!({ "creator_id": CREATOR }))
}

/// Ported from the daemon `memory_pagination_bounded.rs` (60 seeded rows,
/// page size 10): one full keyset cursor walk returns every seeded row exactly
/// once, in `created_at DESC` order — no gap, no overlap, no duplicate — which
/// is the behavioral proof that the page bound is pushed into SQL instead of
/// being a fetch-all-then-truncate. It is also what lets `creator memory
/// pending-show` find an id past the first page: the oldest row is absent from
/// page one and only reachable by continuing the walk, and a cursor whose own
/// row was deleted restarts from the top instead of erroring.
#[tokio::test]
async fn retained_pending_cursor_walk_has_no_gaps_or_overlap() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    seed_creator_pending(&env, "pending_bounded", SEED_COUNT).await;

    // Page one is bounded by the requested page size and advertises more.
    let first = core
        .list_pending_reviews(&principal, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(
        first.items.len(),
        PAGE_SIZE,
        "page one must return exactly the page size, not the whole dataset"
    );
    assert_eq!(first.pagination.limit, i64::try_from(PAGE_SIZE).unwrap());
    assert!(
        first.pagination.has_more,
        "has_more must be true while rows remain"
    );
    assert!(
        first.pagination.next_cursor.is_some(),
        "has_more without next_cursor"
    );
    assert_eq!(
        first.items[0].pending_id, "pending_bounded_059",
        "newest first"
    );
    // A NULL `world_id` is omitted from the item projection (optional field).
    let item_wire = serde_json::to_value(&first.items[0]).unwrap();
    assert!(!item_wire.as_object().unwrap().contains_key("world_id"));

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut walked: Vec<String> = first
        .items
        .iter()
        .map(|item| item.pending_id.clone())
        .collect();
    for id in &walked {
        assert!(seen.insert(id.clone()), "duplicate pending_id: {id}");
    }

    let mut cursor = first.pagination.next_cursor.clone();
    let mut pages = 1usize;
    while let Some(c) = cursor.take() {
        let page = core
            .list_pending_reviews(&principal, Some(c.clone()), PAGE_SIZE)
            .await
            .unwrap();
        assert!(
            !page.items.is_empty(),
            "page {pages} returned 0 items mid-walk (gap or premature truncation)"
        );
        for item in &page.items {
            assert!(
                seen.insert(item.pending_id.clone()),
                "duplicate pending_id across pages: {}",
                item.pending_id
            );
        }
        walked.extend(page.items.iter().map(|item| item.pending_id.clone()));
        pages += 1;
        assert!(
            pages <= SEED_COUNT,
            "the walk paginated past the dataset without terminating"
        );
        if page.pagination.has_more {
            let next = page
                .pagination
                .next_cursor
                .clone()
                .expect("has_more without next_cursor");
            assert_ne!(next, c, "the cursor must advance");
            cursor = Some(next);
        } else {
            assert!(
                page.pagination.next_cursor.is_none(),
                "the terminal page carries no cursor"
            );
        }
    }

    assert_eq!(seen.len(), SEED_COUNT, "every seeded row exactly once");
    assert_eq!(pages, SEED_COUNT / PAGE_SIZE, "page count");
    let expected: Vec<String> = (0..SEED_COUNT)
        .rev()
        .map(|i| format!("pending_bounded_{i:03}"))
        .collect();
    assert_eq!(walked, expected, "the walk is one total DESC ordering");

    // Pending-show beyond page one: the oldest row is not on page one, and the
    // walk only reaches it on the final page.
    assert!(
        !first
            .items
            .iter()
            .any(|item| item.pending_id == "pending_bounded_000"),
        "the oldest row must not be on page one"
    );
    assert_eq!(
        walked[walked.len() - 1],
        "pending_bounded_000",
        "the walk must reach the row buried past page one"
    );

    // A cursor whose own row was deleted (or that is otherwise unknown)
    // restarts from the top instead of erroring or skipping the head.
    let dangling = first
        .pagination
        .next_cursor
        .clone()
        .expect("page-one cursor");
    let pool = plain_pool(&env).await;
    sqlx::query("DELETE FROM memory_pending_review WHERE pending_id = ?")
        .bind(&dangling)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let fallback = core
        .list_pending_reviews(&principal, Some(dangling), PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(fallback.items.len(), PAGE_SIZE, "fallback page size");
    assert_eq!(
        fallback.items[0].pending_id, "pending_bounded_059",
        "a dangling cursor restarts from the first page"
    );
    assert!(fallback.pagination.has_more);
}

/// Ported from the daemon Creator review drain (`memory_review_fragments_api.rs`
/// V1.80 REL-01 + the empty-queue case): one call inspects at most
/// `REVIEW_BATCH_LIMIT` (50) rows and reports `has_more`, repeated calls drain
/// the remainder with no row lost and no fragment inserted twice, and the
/// request's `creator_id` is authorized against the active principal before
/// any queue mutation.
#[tokio::test]
async fn retained_creator_review_drain_is_bounded_and_keeps_remaining_rows() {
    /// Seeded rows — above the 50-row batch bound.
    const DRAIN_TOTAL: usize = 55;
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    seed_creator_pending(&env, "pending_bulk", DRAIN_TOTAL).await;

    // Authorization precedes the drain: a request naming another creator is
    // refused and mutates nothing.
    let err = core
        .review_memory(
            &principal,
            dto::<ReviewRequest>(serde_json::json!({ "creator_id": OTHER })),
        )
        .await
        .expect_err("a foreign creator_id must be refused");
    assert!(
        matches!(err, CoreError::ForbiddenReason { .. }),
        "got {err:?}"
    );
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        i64::try_from(DRAIN_TOTAL).unwrap(),
        "a refused review must not advance the queue"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_fragments").await,
        0,
        "a refused review must not fragment"
    );
    pool.close().await;

    // One bounded call: never more than the batch limit, and more rows
    // signalled. The provider commits a durable record set per row, so a call
    // can also stop at its own deadline; partial progress is the retained
    // contract and repeated calls drain the queue.
    let first = core
        .review_memory(&principal, creator_review_request())
        .await
        .unwrap();
    let first_processed = first.processed.expect("processed count");
    assert!(
        first_processed > 0 && first_processed <= 50,
        "one call is bounded by the batch bound, got {first_processed}"
    );
    assert_eq!(
        first.has_more,
        Some(true),
        "remaining rows must be signalled"
    );
    assert_eq!(first.promoted + first.dropped, 0);
    assert!(
        first.fragmented <= first_processed,
        "an attempt the deadline stopped counts as processed but not completed"
    );

    // Repeated calls drain the remainder; every call stays bounded and the
    // drain reports completion once the queue is empty. `processed` counts
    // attempts, so a deadline-stopped row is retried by the next call and
    // completes exactly once.
    let mut fragmented = first.fragmented;
    let mut completed = false;
    for _ in 0..=DRAIN_TOTAL {
        let next = core
            .review_memory(&principal, creator_review_request())
            .await
            .unwrap();
        assert!(
            next.processed.expect("processed count") <= 50,
            "every call is bounded"
        );
        assert_eq!(next.promoted + next.dropped, 0);
        fragmented += next.fragmented;
        if next.has_more == Some(false) {
            completed = true;
            break;
        }
    }
    assert!(completed, "the drain must report completion");
    assert_eq!(
        fragmented,
        i64::try_from(DRAIN_TOTAL).unwrap(),
        "every seeded row completes exactly once"
    );

    // A call on the empty queue reports zero work and completion.
    let third = core
        .review_memory(&principal, creator_review_request())
        .await
        .unwrap();
    assert_eq!(third.processed, Some(0));
    assert_eq!(third.has_more, Some(false));
    assert_eq!(third.promoted + third.fragmented + third.dropped, 0);

    // No row was lost and none was processed twice: the provider queue is empty,
    // exactly one fragment record exists per seeded candidate, and the legacy
    // table never changed (compass D6).
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        i64::try_from(DRAIN_TOTAL).unwrap(),
        "the legacy queue stays write-idle"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_candidates WHERE status = 'pending'"
        )
        .await,
        0,
        "every provider candidate was consumed"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_records WHERE kind = 'fact'"
        )
        .await,
        i64::try_from(DRAIN_TOTAL).unwrap(),
        "one fragment per seeded row, no duplicate insert"
    );
    pool.close().await;
}

/// Ported from the daemon W-QC3-001 drain-completion regressions
/// (`memory_review_fragments_api.rs`): `has_more` reflects whether candidates
/// REMAIN, not whether rows were attempted. A row whose decision fails at its
/// commit boundary stays pending, so `has_more` stays true (never a false
/// "Review complete"), the other rows still complete, and the retried row is
/// consumed exactly once.
#[tokio::test]
async fn retained_creator_review_row_failure_keeps_has_more_true() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    seed_creator_pending(&env, "pending_blocked", 2).await;
    let pinned = core.memory_provider_for_test().expect("pinned provider");

    // Park the drain after its re-check (the adapter is open, the slice is
    // selected) and arm the §14.3 commit-boundary fault: the next row's decision
    // fails before its COMMIT and leaves no effect.
    let (entered, release) = arm_review_pause(&core, MemoryReviewPause::AfterRecheck);
    let drain = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        async move {
            core.review_memory(&principal, creator_review_request())
                .await
        }
    });
    entered.notified().await;
    pinned.inject_commit_fault(false);
    release.notify_one();
    let out = drain
        .await
        .unwrap()
        .expect("the batch survives one failed row");
    assert_eq!(out.processed, Some(2), "both rows were inspected");
    assert_eq!(out.fragmented, 1, "the other row completed");
    assert_eq!(
        out.has_more,
        Some(true),
        "has_more must stay true while a row remains"
    );
    assert_eq!(
        core.count_pending_reviews(&principal).await.unwrap().count,
        1,
        "the failed row is still queued"
    );
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_candidates").await,
        1,
        "only the completed row was adopted and consumed"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_records WHERE kind = 'fact'"
        )
        .await,
        1,
        "only the completed row recorded a fragment"
    );
    pool.close().await;

    // The retry classifies the remaining row and drains the queue.
    let retried = core
        .review_memory(&principal, creator_review_request())
        .await
        .expect("the retry drains the remaining row");
    assert_eq!(retried.processed, Some(1));
    assert_eq!(
        retried.fragmented, 1,
        "the retried row completes exactly once"
    );
    assert_eq!(retried.has_more, Some(false));
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_records WHERE kind = 'fact'"
        )
        .await,
        2,
        "no fragment is duplicated and none is lost"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        2,
        "the legacy queue was never written"
    );
    pool.close().await;
}

/// Ported from the daemon `memory_pagination_bounded.rs` fragment bound plus
/// `memory_dto_roundtrip.rs`: the Creator fragment list is bounded in SQL (a
/// large dataset returns exactly the page, a limit above it returns all), and
/// the item projection carries the public keyword/created-at metadata only —
/// the internal write-only columns never reach a client, and a NULL
/// `world_id` is omitted.
#[tokio::test]
async fn retained_creator_fragment_list_is_bounded_and_projects_public_fields() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // Nothing stored yet → an empty list, not an error.
    let empty = core
        .list_memory_fragments(&principal, None, None, PAGE_SIZE)
        .await
        .unwrap();
    assert!(empty.fragments.is_empty());

    seed_creator_fragments(&env, SEED_COUNT).await;
    let page = core
        .list_memory_fragments(&principal, None, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(
        page.fragments.len(),
        PAGE_SIZE,
        "the dataset is {SEED_COUNT} rows but the page bound is {PAGE_SIZE}"
    );
    assert_eq!(
        page.fragments[0].fragment_id, "frag_seeded_059",
        "newest first"
    );
    assert_eq!(page.fragments[0].keywords, vec!["kw_59".to_string()]);
    assert_eq!(
        page.fragments[0].created_at.as_deref(),
        Some("2026-02-01T00:59:00Z")
    );
    let wire = serde_json::to_value(&page.fragments[0]).unwrap();
    let fields = wire.as_object().unwrap();
    assert!(fields.contains_key("keywords"));
    assert!(fields.contains_key("created_at"));
    for internal in ["ttl", "session_id", "creator_id", "world_id"] {
        assert!(
            !fields.contains_key(internal),
            "internal/absent field `{internal}` must stay off the fragment projection: {wire}"
        );
    }

    // The bound is a cap, not a requirement: a limit above the dataset lists all.
    let all = core
        .list_memory_fragments(&principal, None, None, SEED_COUNT + 100)
        .await
        .unwrap();
    assert_eq!(all.fragments.len(), SEED_COUNT);
}

/// Ported from the daemon `memory_review_fragments_api.rs` world propagation
/// pair (R-V181P0-QC1-W001): a pending row's `world_id` survives into the
/// fragment the review mints, and the fragment list filters on it exactly —
/// the matching world includes the row, any other world excludes it, and the
/// unscoped read lists both the scoped and the unscoped fragment.
#[tokio::test]
async fn retained_creator_review_propagates_world_id_and_filters_exactly() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    seed_creator_pending_row(&env, "pend_world_x", Some("wld_x"), "2026-01-01T00:00:01Z").await;
    seed_creator_pending_row(&env, "pend_world_none", None, "2026-01-01T00:00:02Z").await;

    // The pending read already carries the provenance (and omits NULL).
    let listed = core
        .list_pending_reviews(&principal, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 2);
    assert_eq!(listed.items[0].pending_id, "pend_world_none");
    assert!(listed.items[0].world_id.is_none());
    assert_eq!(listed.items[1].world_id.as_deref(), Some("wld_x"));

    let out = core
        .review_memory(&principal, creator_review_request())
        .await
        .unwrap();
    assert_eq!(out.fragmented, 2, "both rows are FragmentOnly");

    let scoped = core
        .list_memory_fragments(&principal, None, Some("wld_x".to_string()), 50)
        .await
        .unwrap();
    assert_eq!(scoped.fragments.len(), 1, "the world filter is exact");
    assert_eq!(scoped.fragments[0].world_id.as_deref(), Some("wld_x"));

    let other = core
        .list_memory_fragments(&principal, None, Some("wld_y".to_string()), 50)
        .await
        .unwrap();
    assert!(
        other.fragments.is_empty(),
        "a different world must exclude the row"
    );

    let unscoped = core
        .list_memory_fragments(&principal, None, None, 50)
        .await
        .unwrap();
    assert_eq!(unscoped.fragments.len(), 2);
    assert_eq!(
        unscoped
            .fragments
            .iter()
            .filter(|f| f.world_id.is_none())
            .count(),
        1,
        "the unscoped read keeps the world-less fragment"
    );
}

// ── Character pending-review lifecycle and drains (P2-T7) ─────────────────

/// Capture request body for one Character scope (`binding_id: None` = shared).
fn json_capture(pending_id: &str, binding_id: Option<&str>, created_at: &str) -> serde_json::Value {
    let mut body = serde_json::json!({
        "pending_id": pending_id,
        "session_id": format!("sess_{pending_id}"),
        "task_kind": "research",
        "raw_digest": FRAGMENT_DIGEST,
        "created_at": created_at,
    });
    if let Some(binding) = binding_id {
        body["binding_id"] = serde_json::json!(binding);
    }
    body
}

/// Ported from the daemon `character_memory_api.rs` lifecycle and pagination
/// cases: capture → count → list → delete stay exact per scope (shared rows
/// never appear in a binding read and vice versa, a missing row is not-found),
/// and the retained offset page walk hands out `v1:{offset}` cursors until the
/// terminal page, which carries none.
#[tokio::test]
async fn retained_character_pending_lifecycle_scope_delete_and_offset_pages() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let bind1 = env.binding_id.clone();
    // Unrelated second binding of the same Character: it must stay empty.
    let bind2 = core
        .add_binding(&principal, chr.clone(), WORLD_B.to_string(), None)
        .await
        .expect("second binding")
        .binding
        .binding_id
        .as_str()
        .to_string();

    // Capture one shared-scope row and one binding-local row.
    let shared = core
        .capture_character_pending_review(
            &principal,
            chr.clone(),
            dto(json_capture("pend_shared_1", None, "2026-01-01T00:00:01Z")),
        )
        .await
        .expect("shared capture");
    assert!(shared.success);
    assert_eq!(shared.pending_id.as_str(), "pend_shared_1");
    let local = core
        .capture_character_pending_review(
            &principal,
            chr.clone(),
            dto(json_capture(
                "pend_local_1",
                Some(&bind1),
                "2026-01-01T00:00:02Z",
            )),
        )
        .await
        .expect("binding-local capture");
    assert_eq!(local.pending_id.as_str(), "pend_local_1");

    // Counts and lists are scope-exact.
    assert_eq!(
        core.count_character_pending_reviews(&principal, chr.clone(), None)
            .await
            .unwrap()
            .count,
        1
    );
    assert_eq!(
        core.count_character_pending_reviews(&principal, chr.clone(), Some(bind1.clone()))
            .await
            .unwrap()
            .count,
        1
    );
    assert_eq!(
        core.count_character_pending_reviews(&principal, chr.clone(), Some(bind2.clone()))
            .await
            .unwrap()
            .count,
        0
    );

    let shared_page = core
        .list_character_pending_reviews(&principal, chr.clone(), None, 50, 0)
        .await
        .unwrap();
    assert_eq!(shared_page.items.len(), 1);
    assert_eq!(shared_page.items[0].pending_id.as_str(), "pend_shared_1");
    assert!(
        shared_page.items[0].binding_id.is_none(),
        "a shared row carries no binding provenance"
    );
    assert!(!shared_page.pagination.has_more);
    assert!(shared_page.pagination.next_cursor.is_none());

    let local_page = core
        .list_character_pending_reviews(&principal, chr.clone(), Some(bind1.clone()), 50, 0)
        .await
        .unwrap();
    assert_eq!(local_page.items.len(), 1);
    assert_eq!(local_page.items[0].pending_id.as_str(), "pend_local_1");
    assert_eq!(
        local_page.items[0].binding_id.as_ref().map(|b| b.as_str()),
        Some(bind1.as_str())
    );

    // Delete removes exactly the addressed row; a missing row is not-found.
    let deleted = core
        .delete_character_pending_review(&principal, chr.clone(), "pend_shared_1".to_string())
        .await
        .expect("delete the shared row");
    assert!(deleted.success);
    assert_eq!(deleted.pending_id.as_str(), "pend_shared_1");
    assert_eq!(
        core.count_character_pending_reviews(&principal, chr.clone(), None)
            .await
            .unwrap()
            .count,
        0
    );
    assert_eq!(
        core.count_character_pending_reviews(&principal, chr.clone(), Some(bind1.clone()))
            .await
            .unwrap()
            .count,
        1,
        "the binding-local row survives"
    );
    let err = core
        .delete_character_pending_review(&principal, chr.clone(), "pend_missing".to_string())
        .await
        .expect_err("a missing row must be not-found");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");

    // Offset page walk: five more shared rows, pages of two, `v1:{offset}`
    // cursors, newest first, and a terminal page without a cursor.
    for i in 0..5 {
        core.capture_character_pending_review(
            &principal,
            chr.clone(),
            dto(json_capture(
                &format!("pend_page_{i}"),
                None,
                &format!("2026-01-02T00:00:0{i}Z"),
            )),
        )
        .await
        .expect("paged capture");
    }
    let mut walked: Vec<String> = Vec::new();
    let mut offset = 0u32;
    let mut pages = 0usize;
    loop {
        let page = core
            .list_character_pending_reviews(&principal, chr.clone(), None, 2, offset)
            .await
            .unwrap();
        pages += 1;
        assert_eq!(page.pagination.limit, 2_i64);
        walked.extend(
            page.items
                .iter()
                .map(|item| item.pending_id.as_str().to_string()),
        );
        if !page.pagination.has_more {
            assert!(
                page.pagination.next_cursor.is_none(),
                "the terminal page carries no cursor"
            );
            break;
        }
        let next = page
            .pagination
            .next_cursor
            .clone()
            .expect("has_more without next_cursor");
        assert_eq!(
            next,
            format!("v1:{}", offset + 2),
            "the offset cursor advances by the page size"
        );
        offset += 2;
        assert!(pages <= 5, "the offset walk must terminate");
    }
    assert_eq!(pages, 3, "five rows at page size two");
    assert_eq!(
        walked,
        vec![
            "pend_page_4",
            "pend_page_3",
            "pend_page_2",
            "pend_page_1",
            "pend_page_0"
        ]
    );
}

/// Ported from the daemon `character_memory_api.rs`
/// `review_is_bounded_at_batch_limit_with_correct_has_more`: the Character arm
/// shares the bounded-drain contract — one call inspects at most
/// `REVIEW_BATCH_LIMIT` (50) rows and signals more, the tail drains, and the
/// empty queue reports zero work.
#[tokio::test]
async fn retained_character_review_batch_is_bounded_and_reports_remaining() {
    /// Seeded rows — above the 50-row batch bound.
    const TOTAL: usize = 53;
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();

    // Short non-creative digests classify as Drop, so the drain writes no
    // fragments and the counters are the entire observable.
    let pool = plain_pool(&env).await;
    for i in 0..TOTAL {
        nexus_local_db::create_character_pending_review(
            &pool,
            CREATOR,
            &nexus_local_db::CharacterPendingReviewRecord {
                pending_id: format!("cpending_{i:03}"),
                session_id: format!("csess_{i:03}"),
                character_id: chr.clone(),
                actor_world_binding_id: None,
                task_kind: "research".to_string(),
                raw_digest: "tiny".to_string(),
                created_at: format!("2026-01-01T00:{i:02}:00Z"),
                source_operation_id: None,
            },
        )
        .await
        .unwrap();
    }
    pool.close().await;

    let first = core
        .review_character_memory(
            &principal,
            chr.clone(),
            dto::<ReviewCharacterMemoryRequest>(serde_json::json!({})),
        )
        .await
        .unwrap();
    let first_processed = first.processed.expect("processed count");
    assert!(
        first_processed > 0 && first_processed <= 50,
        "one call is bounded by the batch bound, got {first_processed}"
    );
    assert_eq!(
        first.has_more,
        Some(true),
        "remaining rows must be signalled"
    );
    assert!(
        first.dropped <= first_processed,
        "an attempt the deadline stopped counts as processed but not completed"
    );

    // Repeated calls drain the remainder; every call stays bounded and the
    // drain reports completion once the queue is empty. `processed` counts
    // attempts, so a deadline-stopped row is retried by the next call and
    // completes exactly once.
    let mut dropped = first.dropped;
    let mut completed = false;
    for _ in 0..=TOTAL {
        let next = core
            .review_character_memory(
                &principal,
                chr.clone(),
                dto::<ReviewCharacterMemoryRequest>(serde_json::json!({})),
            )
            .await
            .unwrap();
        assert!(
            next.processed.expect("processed count") <= 50,
            "every call is bounded"
        );
        dropped += next.dropped;
        if next.has_more == Some(false) {
            completed = true;
            break;
        }
    }
    assert!(completed, "the drain must report completion");
    assert_eq!(
        dropped,
        i64::try_from(TOTAL).unwrap(),
        "every seeded row completes exactly once"
    );

    let third = core
        .review_character_memory(
            &principal,
            chr.clone(),
            dto::<ReviewCharacterMemoryRequest>(serde_json::json!({})),
        )
        .await
        .unwrap();
    assert_eq!(third.processed, Some(0));
    assert_eq!(third.has_more, Some(false));

    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM character_memory_pending_review"
        )
        .await,
        i64::try_from(TOTAL).unwrap(),
        "the legacy queue stays write-idle"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_memory_fragments").await,
        0,
        "dropped digests create no fragments"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_candidates WHERE status = 'pending'"
        )
        .await,
        0,
        "every provider candidate was consumed"
    );
    pool.close().await;
}

/// Ported from the daemon `character_memory_api.rs` activity-fence pair: both
/// Character memory writes (pending delete, fragment promotion) take the
/// per-Character activity lease across their DB effect, so an archive
/// transition refuses `character_busy` while the write is outstanding and
/// succeeds — with the write intact — once the lease drains.
#[tokio::test]
async fn retained_character_memory_writes_hold_the_activity_fence() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let bind1 = env.binding_id.clone();

    // Delete path.
    core.capture_character_pending_review(
        &principal,
        chr.clone(),
        dto(json_capture(
            "pend_busy",
            Some(&bind1),
            "2026-01-01T00:00:01Z",
        )),
    )
    .await
    .expect("binding-local capture");
    let activity = core
        .acquire_actor_activity(
            &principal,
            &AdmittedActor::Character {
                character_id: chr.clone(),
            },
        )
        .await
        .expect("admit memory activity");
    let busy = core
        .transition_character(
            &principal,
            transition_request(
                &chr,
                0,
                CoreCharacterTransitionRequestTargetStatus::Archived,
            ),
        )
        .await
        .expect_err("archive must refuse while memory activity is outstanding");
    assert_conflict(busy, "character_busy");
    drop(activity);
    let deleted = core
        .delete_character_pending_review(&principal, chr.clone(), "pend_busy".to_string())
        .await
        .expect("the write succeeds once the lease drains");
    assert!(deleted.success);

    // Promotion path: same lease, so the fence refusal repeats.
    core.capture_character_pending_review(
        &principal,
        chr.clone(),
        dto(json_capture(
            "pend_promote",
            Some(&bind1),
            "2026-01-01T00:00:02Z",
        )),
    )
    .await
    .expect("binding-local capture");
    core.review_character_memory(
        &principal,
        chr.clone(),
        dto::<ReviewCharacterMemoryRequest>(serde_json::json!({ "binding_id": bind1 })),
    )
    .await
    .expect("binding-local review");
    let fragment_id = core
        .list_character_memory_fragments(&principal, chr.clone(), Some(bind1.clone()), 50, 0)
        .await
        .expect("list binding fragments")
        .fragments[0]
        .fragment_id
        .as_str()
        .to_string();

    let activity = core
        .acquire_actor_activity(
            &principal,
            &AdmittedActor::Character {
                character_id: chr.clone(),
            },
        )
        .await
        .expect("admit promotion activity");
    let busy = core
        .transition_character(
            &principal,
            transition_request(
                &chr,
                0,
                CoreCharacterTransitionRequestTargetStatus::Archived,
            ),
        )
        .await
        .expect_err("archive must refuse while promotion activity is outstanding");
    assert_conflict(busy, "character_busy");
    drop(activity);
    let promoted = core
        .promote_character_fragment(&principal, chr.clone(), fragment_id, 0)
        .await
        .expect("promotion succeeds once the lease drains");
    assert_eq!(promoted.fragment.revision, 1u64);
}

fn transition_request(
    character_id: &str,
    expected_revision: i64,
    target: CoreCharacterTransitionRequestTargetStatus,
) -> CoreCharacterTransitionRequest {
    CoreCharacterTransitionRequest::builder()
        .character_id(character_id.to_string())
        .expected_revision(expected_revision)
        .target_status(target)
        .try_into()
        .expect("transition request is wire-valid")
}

// ═══ NXM-5 Task 1: provider-seam legacy read bridge ═══════════════════════
//
// The bridge projects the legacy bearer surfaces (tables and files) as
// provider-native rows, so one `MemoryPort` read path serves both the
// provider's own content and unmigrated legacy content. These tests pin the
// merged view: provider-native and projected rows appear through the same
// read, the projected ids are deterministic, and nothing is written to the
// legacy surfaces by a read.

use nexus_core::memory_provider::legacy;
use nexus_core::memory_provider::LocalMemoryProvider;
use spoke_operations::spoke_schemas as bridge_schemas;
use spoke_operations::{MemoryPort, SpokeResult};

const BRIDGE_SPACE: &str = "spc_local_default";
const BRIDGE_CREATOR_SUBJECT: &str = "sbj_bridge_creator";
const BRIDGE_CHARACTER_SUBJECT: &str = "sbj_bridge_character";
const BRIDGE_TEXT: &str = "Provider-native record written by the current provider.";
const BRIDGE_SOUL: &str = "# Soul\n\nA bridge-authored soul document.\n";

fn bridge_principal() -> bridge_schemas::Principal {
    dto(serde_json::json!({ "principal_id": CREATOR, "extensions": {} }))
}

/// The success payload of a typed port result; a refusal panics with its code.
fn bridge_ok<T: serde::Serialize>(result: &SpokeResult<T>) -> serde_json::Value {
    match result {
        SpokeResult::Ok(value) => serde_json::to_value(value).expect("serializable response"),
        SpokeResult::Reject(reject) => panic!(
            "expected success, got {}: {}",
            reject.code.as_str(),
            reject.message
        ),
    }
}

async fn open_bridge_provider(env: &Env) -> LocalMemoryProvider {
    LocalMemoryProvider::open(env.user_home.clone(), CREATOR, "default")
        .await
        .expect("provider open")
}

/// Opt a provider subject into the legacy projection of one bearer.
fn bridge_ensure_request(
    subject: &str,
    namespace: &str,
    kind_id: &str,
) -> bridge_schemas::MemorySubjectsEnsureRequest {
    bridge_ensure_request_in(BRIDGE_SPACE, subject, namespace, kind_id)
}

fn bridge_ensure_request_in(
    space_id: &str,
    subject: &str,
    namespace: &str,
    kind_id: &str,
) -> bridge_schemas::MemorySubjectsEnsureRequest {
    dto(serde_json::json!({
        "space_id": space_id,
        "subject_id": subject,
        "kind": { "ns": namespace, "id": kind_id },
    }))
}

#[allow(clippy::needless_pass_by_value)] // one request builder per test read
fn bridge_records_request(
    subject: &str,
    selection: serde_json::Value,
    kinds: serde_json::Value,
) -> bridge_schemas::MemoryRecordsListRequest {
    bridge_records_request_in(BRIDGE_SPACE, subject, selection, kinds)
}

#[allow(clippy::needless_pass_by_value)] // one request builder per test read
fn bridge_records_request_in(
    space_id: &str,
    subject: &str,
    selection: serde_json::Value,
    kinds: serde_json::Value,
) -> bridge_schemas::MemoryRecordsListRequest {
    dto(serde_json::json!({
        "space_id": space_id,
        "subject_id": subject,
        "selection": selection,
        "kinds": kinds,
    }))
}

fn bridge_recall_request(subject: &str) -> bridge_schemas::MemoryRecallRequest {
    dto(serde_json::json!({
        "space_id": BRIDGE_SPACE,
        "subject_id": subject,
        "selection": { "mode": "subject_all" },
        "budget": { "max_items": 50 },
    }))
}

fn bridge_record_get_request(record_id: &str) -> bridge_schemas::MemoryRecordsGetRequest {
    dto(serde_json::json!({ "space_id": BRIDGE_SPACE, "record_id": record_id }))
}

fn bridge_report_ids(body: &serde_json::Value, field: &str) -> Vec<String> {
    body[field]
        .as_array()
        .unwrap_or_else(|| panic!("expected `{field}` to be an array: {body}"))
        .iter()
        .map(|row| {
            row["record_id"]
                .as_str()
                .unwrap_or_else(|| panic!("expected a record id in {row}"))
                .to_string()
        })
        .collect()
}

/// The wire reject code of a refused port result; a success panics.
fn bridge_reject_code<T>(result: &SpokeResult<T>) -> String {
    match result {
        SpokeResult::Ok(_) => panic!("expected a refusal"),
        SpokeResult::Reject(reject) => reject.code.as_str().to_string(),
    }
}

/// One records page under an explicit limit and cursor.
fn bridge_records_page_request(
    subject: &str,
    limit: u64,
    cursor: Option<&str>,
) -> bridge_schemas::MemoryRecordsListRequest {
    dto(serde_json::json!({
        "space_id": BRIDGE_SPACE,
        "subject_id": subject,
        "selection": { "mode": "subject_all" },
        "kinds": [],
        "limit": limit,
        "cursor": cursor,
    }))
}

/// Publish P5's completion marker for one space the way the migration pass
/// does: a `memory_provider_meta` row written by a registered core writer.
async fn publish_migration_marker(env: &Env, space_id: &str) {
    let guarded = init_guarded_pool(&env.db_path, CREATOR).await.unwrap();
    let pool = guarded.clone_pool();
    sqlx::query("INSERT INTO memory_provider_meta (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(legacy::legacy_migration_key(space_id))
        .bind("2026-11-02T00:00:00Z")
        .bind("2026-11-02T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    drop(guarded);
}

/// Capture one provider-native pending candidate under `subject` and return its
/// id, so a marker scenario can prove the provider's own state stays readable.
async fn native_pending_candidate(
    provider: &LocalMemoryProvider,
    admin: &bridge_schemas::Principal,
    subject: &str,
    idempotency_key: &str,
) -> String {
    let captured = bridge_ok(
        &provider
            .capture(
                admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": subject,
                    "content": { "text": FRAGMENT_DIGEST },
                    "capture_scope": { "visibility": "shared" },
                    "source_context_ids": [],
                    "idempotency_key": idempotency_key,
                })),
            )
            .await,
    );
    captured["result"]["candidate_ids"][0]
        .as_str()
        .expect("a committed capture names its candidate")
        .to_string()
}

/// Seed every Creator legacy surface the bridge projects: one fragment, one
/// soul narrative, one long-term memory file, `SOUL.md` and one pending-review
/// row.
async fn seed_legacy_creator_content(env: &Env) {
    let pool = plain_pool(env).await;
    nexus_local_db::memory_fragment::create_fragment(
        &pool,
        &nexus_local_db::memory_fragment::MemoryFragmentRecord {
            fragment_id: "frag_bridge".to_string(),
            session_id: "sess_bridge".to_string(),
            creator_id: CREATOR.to_string(),
            keywords: serde_json::json!(["kw_bridge"]).to_string(),
            summary: "Legacy creator fragment text.".to_string(),
            created_at: "2026-03-01T00:00:00Z".to_string(),
            ttl: None,
            world_id: None,
        },
    )
    .await
    .unwrap();
    nexus_local_db::upsert_soul_narrative(
        &pool,
        &nexus_local_db::SoulNarrativeRecord {
            creator_id: CREATOR.to_string(),
            world_id: None,
            narrative: Some("Legacy soul narrative.".to_string()),
            generated_at: Some("2026-03-01T00:00:01Z".to_string()),
            fragment_count_at_generation: 1,
            max_fragment_created_at_at_generation: None,
            distinct_keyword_count_cache: 1,
            stats_fingerprint: None,
            created_at: "2026-03-01T00:00:01Z".to_string(),
            updated_at: "2026-03-01T00:00:01Z".to_string(),
        },
    )
    .await
    .unwrap();
    nexus_local_db::create_pending_review(
        &pool,
        &nexus_local_db::PendingReviewRecord {
            pending_id: "pend_bridge".to_string(),
            session_id: "sess_pend_bridge".to_string(),
            creator_id: CREATOR.to_string(),
            world_id: Some(WORLD.to_string()),
            task_kind: "research".to_string(),
            raw_digest: FRAGMENT_DIGEST.to_string(),
            created_at: "2026-03-01T00:00:02Z".to_string(),
        },
    )
    .await
    .unwrap();
    pool.close().await;

    let home = env.user_home.as_path();
    let bearer = MemoryBearerRef::Creator(CREATOR);
    let mut memory = nexus_creator_memory::LongTermMemory::new("story_summary");
    memory.set_body("Legacy long-term body.");
    nexus_creator_memory::memory_io::save_memory(home, bearer, "bridge-memory", &memory)
        .expect("long-term memory fixture");
    std::fs::write(
        nexus_home_layout::creator_soul_md_path(home, CREATOR),
        BRIDGE_SOUL,
    )
    .expect("SOUL.md fixture");
}

/// Existing content that still lives only in a bearer file is editable and
/// deletable before P5's migration (NXM-6 fix round 1, finding F2).
///
/// The mutation adopts the projected row under the bridge's deterministic id
/// inside its own guarded transaction, so the user's write lands on the provider
/// and the legacy file keeps every byte it had (D6).
#[tokio::test]
async fn creator_mutations_adopt_projected_legacy_memories() {
    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    let bearer = MemoryBearerRef::Creator(CREATOR);
    let memory_path = bearer.long_term_memory_path(&env.user_home, "bridge-memory");
    let projected_bytes = std::fs::read(&memory_path).expect("legacy memory bytes");
    let mut personality = nexus_creator_memory::LongTermMemory::new("personality_core");
    personality.set_body("Projected personality body.");
    nexus_creator_memory::memory_io::save_memory(
        &env.user_home,
        bearer,
        "personality-core",
        &personality,
    )
    .expect("personality memory fixture");
    let personality_path = bearer.long_term_memory_path(&env.user_home, "personality-core");
    let personality_bytes = std::fs::read(&personality_path).expect("legacy personality bytes");

    let (core, principal) = open_core(&env).await;

    // The read serves the projection …
    let listed = serde_json::to_value(
        core.get_creator_memory(&principal, "bridge-memory")
            .await
            .unwrap()
            .expect("the projected memory is listed"),
    )
    .unwrap();
    assert_eq!(listed["content"]["text"], "Legacy long-term body.");

    // … and the edit replaces it on the provider.
    assert!(core
        .update_creator_memory(&principal, "bridge-memory", "Edited body.", None)
        .await
        .expect("the edit adopts the projection"));
    let edited = serde_json::to_value(
        core.get_creator_memory(&principal, "bridge-memory")
            .await
            .unwrap()
            .expect("the edited memory is listed"),
    )
    .unwrap();
    assert_eq!(edited["content"]["text"], "Edited body.");
    assert_eq!(
        std::fs::read(&memory_path).unwrap(),
        projected_bytes,
        "the edit never writes the legacy file"
    );

    // The SOUL push targets the same kind of projected row.
    let pushed = serde_json::to_value(
        core.push_creator_personality(&principal, "Pushed personality section.")
            .await
            .expect("the push adopts the projection"),
    )
    .unwrap();
    assert_eq!(pushed["content"]["text"], "Pushed personality section.");
    assert_eq!(
        std::fs::read(&personality_path).unwrap(),
        personality_bytes,
        "the push never writes the legacy file"
    );

    // The delete tombstones the adopted row while the file stays as it was.
    assert!(core
        .delete_creator_memory(&principal, "bridge-memory")
        .await
        .expect("the delete adopts the projection"));
    assert!(core
        .get_creator_memory(&principal, "bridge-memory")
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        std::fs::read(&memory_path).unwrap(),
        projected_bytes,
        "the delete never writes the legacy file"
    );
}

/// An interactive edit's read is the write's precondition, and two different
/// mutations never share an idempotency key (NXM-6 fix round 1, F3/F4).
#[tokio::test]
async fn creator_memory_writes_guard_the_read_revision_and_name_their_mutation() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    assert!(core
        .create_creator_memory(&principal, "edit-guard", "story_summary", "First body.")
        .await
        .unwrap());

    // The editor's read: the identity and revision it carries into the write.
    let read = serde_json::to_value(
        core.get_creator_memory(&principal, "edit-guard")
            .await
            .unwrap()
            .expect("the created memory is listed"),
    )
    .unwrap();
    let expected = nexus_core::MemoryRevision {
        record_id: read["record_id"]
            .as_str()
            .expect("the record names itself")
            .to_string(),
        revision: read["revision"]
            .as_u64()
            .expect("the record carries a revision"),
    };

    // A concurrent writer commits while the editor is open.
    assert!(core
        .update_creator_memory(&principal, "edit-guard", "Concurrent body.", None)
        .await
        .unwrap());
    let error = core
        .update_creator_memory(
            &principal,
            "edit-guard",
            "Stale editor body.",
            Some(&expected),
        )
        .await
        .expect_err("a stale editor revision is refused");
    assert_conflict(error, "version_mismatch");
    let current = serde_json::to_value(
        core.get_creator_memory(&principal, "edit-guard")
            .await
            .unwrap()
            .expect("the memory still exists"),
    )
    .unwrap();
    assert_eq!(
        current["content"]["text"], "Concurrent body.",
        "the concurrent change survives the stale write"
    );

    // Repeated edits of one slug are distinct mutations: X, Y, X all land.
    assert!(core
        .update_creator_memory(&principal, "edit-guard", "Edited again.", None)
        .await
        .unwrap());
    assert!(core
        .update_creator_memory(&principal, "edit-guard", "Concurrent body.", None)
        .await
        .unwrap());
    let current = serde_json::to_value(
        core.get_creator_memory(&principal, "edit-guard")
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(current["content"]["text"], "Concurrent body.");

    // delete → recreate → delete is three mutations of the same slug.
    assert!(core
        .create_creator_memory(&principal, "recycled", "story_summary", "Recycled body.")
        .await
        .unwrap());
    assert!(core
        .delete_creator_memory(&principal, "recycled")
        .await
        .unwrap());
    assert!(core
        .create_creator_memory(&principal, "recycled", "story_summary", "Recycled body.")
        .await
        .unwrap());
    assert!(core
        .delete_creator_memory(&principal, "recycled")
        .await
        .unwrap());
    assert!(core
        .get_creator_memory(&principal, "recycled")
        .await
        .unwrap()
        .is_none());
}

/// Each SOUL refresh is a distinct mutation of the profile revision, so a
/// second refresh cannot collide with the first one's receipt (NXM-6 fix round
/// 1, finding F4).
#[tokio::test]
async fn creator_experience_refresh_repeats_without_a_key_collision() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    core.set_creator_soul(&principal, "# Soul\n\nAuthored body.\n", None)
        .await
        .expect("create the SOUL document");
    assert!(core
        .create_creator_memory(
            &principal,
            "experience-source",
            "story_summary",
            "The alliance holds the harbor."
        )
        .await
        .unwrap());

    let (first, processed) = core
        .refresh_creator_experience(&principal)
        .await
        .expect("first refresh");
    assert_eq!(processed, 1, "one memory feeds the Experience section");
    let (second, _) = core
        .refresh_creator_experience(&principal)
        .await
        .expect("a repeated refresh is a new mutation, not a receipt collision");
    assert_eq!(first, second, "the rendered section is deterministic");
    let soul = core
        .get_creator_soul(&principal)
        .await
        .unwrap()
        .expect("the SOUL document exists");
    assert!(
        soul.document
            .body
            .contains("### Story Summary: experience-source"),
        "the SOUL carries the rendered Experience section: {}",
        soul.document.body
    );
}

/// Seed the Character legacy surfaces: one binding-local fragment and one run
/// capture receipt (which also enqueues its own pending row).
async fn seed_legacy_character_content(env: &Env) {
    // `character_run_captures` is engine-owned (§5.2 capture lane): its writer
    // guard admits only an engine-mode writer, unlike the direct-mode surfaces
    // the other seeds use.
    let guarded = init_engine_pool(&env.db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .unwrap();
    let pool = guarded.clone_pool();
    let lifecycle_epoch: i64 =
        sqlx::query_scalar("SELECT lifecycle_epoch FROM characters WHERE character_id = ?")
            .bind(&env.character_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    nexus_local_db::create_character_fragment(
        &pool,
        CREATOR,
        &nexus_local_db::NewCharacterMemoryFragment {
            fragment_id: "hfrag_bridge".to_string(),
            session_id: "hsess_bridge".to_string(),
            character_id: env.character_id.clone(),
            actor_world_binding_id: Some(env.binding_id.clone()),
            keywords: serde_json::json!(["kw_binding"]).to_string(),
            summary: "Binding-local character fragment.".to_string(),
            created_at: "2026-03-02T00:00:00Z".to_string(),
            ttl: None,
        },
    )
    .await
    .unwrap();
    nexus_local_db::capture_character_run(
        &pool,
        CREATOR,
        nexus_local_db::RunCaptureInput {
            operation_id: "op_bridge",
            session_id: "sess_run_bridge",
            character_id: &env.character_id,
            binding_id: &env.binding_id,
            pending_id: "pend_char_bridge",
            raw_digest: FRAGMENT_DIGEST,
            captured_at: "2026-03-02T00:00:01Z",
            lifecycle_epoch,
        },
    )
    .await
    .unwrap();
    pool.close().await;
    drop(guarded);
}

#[tokio::test]
async fn merged_reads_serve_provider_native_and_projected_legacy_records() {
    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    // The read bridge is read-only: the bearer files are the input, never an
    // output, so their bytes must survive every merged read below.
    let long_term_path =
        MemoryBearerRef::Creator(CREATOR).long_term_memory_path(&env.user_home, "bridge-memory");
    let soul_path = nexus_home_layout::creator_soul_md_path(&env.user_home, CREATOR);
    let long_term_before = std::fs::read(&long_term_path).expect("long-term file bytes");
    let soul_before = std::fs::read(&soul_path).expect("SOUL.md bytes");
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(BRIDGE_CREATOR_SUBJECT, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );
    let created = bridge_ok(
        &provider
            .create_record(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                    "kind": "note",
                    "content": { "text": BRIDGE_TEXT },
                    "scope": { "visibility": "shared" },
                    "idempotency_key": "k-bridge-native",
                })),
            )
            .await,
    );
    let native_id = created["result"]["record_id"]
        .as_str()
        .expect("a created record id")
        .to_string();

    let listed = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CREATOR_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    let ids = bridge_report_ids(&listed, "records");
    assert_eq!(
        listed["total_authorized"], 4,
        "the provider's own record and the three projected legacy records share one authorized set: {listed}"
    );
    assert!(
        ids.contains(&native_id),
        "provider-native row missing: {ids:?}"
    );
    assert!(
        ids.contains(&"lgc:memory_fragments:frag_bridge".to_string()),
        "projected fragment missing: {ids:?}"
    );
    assert!(
        ids.contains(&format!("lgc:memory_long_term:{CREATOR}:bridge-memory")),
        "projected long-term memory file missing: {ids:?}"
    );
    assert!(
        ids.contains(&"lgc:memory_soul_narratives:%00".to_string()),
        "projected Creator-level soul narrative missing: {ids:?}"
    );

    let kinds: Vec<&str> = listed["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"fact"),
        "legacy fragment projects as a fact: {kinds:?}"
    );
    assert!(
        kinds.contains(&"nexus.story_summary"),
        "a long-term memory keeps its authored kind: {kinds:?}"
    );
    assert!(
        kinds.contains(&"nexus.soul_narrative"),
        "a cached soul narrative keeps its own kind: {kinds:?}"
    );

    // The projected rows are readable by their external id through the same
    // by-id route, and carry the legacy content they project.
    let fragment = bridge_ok(
        &provider
            .get_record(
                &admin,
                bridge_record_get_request("lgc:memory_fragments:frag_bridge"),
            )
            .await,
    );
    assert_eq!(
        fragment["record"]["content"]["text"],
        "Legacy creator fragment text."
    );
    assert_eq!(fragment["record"]["revision"], 1);
    assert_eq!(fragment["record"]["subject_id"], BRIDGE_CREATOR_SUBJECT);

    // Deterministic external ids: the same read twice returns the same ids in
    // the same order.
    let again = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CREATOR_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        bridge_report_ids(&again, "records"),
        ids,
        "projected ids are stable across repeated reads"
    );

    // Recall ranks the merged set: every authorized record is a candidate.
    let recalled = bridge_ok(
        &provider
            .recall(&admin, bridge_recall_request(BRIDGE_CREATOR_SUBJECT))
            .await,
    );
    assert_eq!(recalled["total_authorized"], 4, "{recalled}");
    assert_eq!(
        bridge_report_ids(&recalled, "records").len(),
        4,
        "recall serves the merged set: {recalled}"
    );

    // A kind filter is applied to the projected rows by the same predicate.
    let only_narrative = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CREATOR_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!(["nexus.soul_narrative"]),
                ),
            )
            .await,
    );
    assert_eq!(
        bridge_report_ids(&only_narrative, "records"),
        vec!["lgc:memory_soul_narratives:%00".to_string()],
        "{only_narrative}"
    );

    // A subject that does not name a legacy bearer projects nothing.
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request("sbj_bridge_plain", "nexus", "plain"),
            )
            .await,
    );
    let plain = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    "sbj_bridge_plain",
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(plain["total_authorized"], 0, "{plain}");

    assert_eq!(
        std::fs::read(&long_term_path).expect("long-term file bytes"),
        long_term_before,
        "a merged read never rewrites a legacy bearer file"
    );
    assert_eq!(
        std::fs::read(&soul_path).expect("SOUL.md bytes"),
        soul_before,
        "a merged read never rewrites SOUL.md"
    );
}

#[tokio::test]
async fn merged_profile_read_serves_the_legacy_soul_document() {
    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(BRIDGE_CREATOR_SUBJECT, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );

    let profile = bridge_ok(
        &provider
            .get_profile(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                })),
            )
            .await,
    );
    let personality = profile["profiles"]
        .as_array()
        .expect("profiles array")
        .iter()
        .find(|row| row["profile_kind"] == "personality")
        .unwrap_or_else(|| panic!("SOUL.md projects as the personality profile: {profile}"));
    assert_eq!(personality["document"]["format"], "markdown");
    assert_eq!(personality["document"]["body"], BRIDGE_SOUL);
    assert_eq!(personality["revision"], 1);
}

#[tokio::test]
async fn merged_candidate_reads_serve_the_legacy_pending_queue() {
    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(BRIDGE_CREATOR_SUBJECT, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );

    let listed = bridge_ok(
        &provider
            .list_candidates(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                })),
            )
            .await,
    );
    let candidate_ids: Vec<&str> = listed["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["candidate_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        candidate_ids,
        vec!["lgc:memory_pending_review:pend_bridge"],
        "the legacy queue entry is projected as a pending candidate: {listed}"
    );
    let projected = &listed["candidates"][0];
    assert_eq!(projected["status"], "pending");
    assert_eq!(projected["capture_scope"]["visibility"], "shared");
    assert_eq!(
        projected["proposed"]["content"]["structured"]["nexus_legacy"]["raw_digest"],
        FRAGMENT_DIGEST
    );

    let fetched = bridge_ok(
        &provider
            .get_candidate(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "candidate_id": "lgc:memory_pending_review:pend_bridge",
                })),
            )
            .await,
    );
    assert_eq!(fetched["candidate"]["candidate_id"], candidate_ids[0]);

    // A status-filtered audit query never admits a projected pending entry.
    let consumed = bridge_ok(
        &provider
            .list_candidates(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                    "status": "consumed",
                })),
            )
            .await,
    );
    assert_eq!(consumed["candidates"], serde_json::json!([]), "{consumed}");
}

#[tokio::test]
async fn merged_character_reads_respect_binding_placement_and_ownership() {
    let env = seed_env().await;
    seed_legacy_character_content(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(
                    BRIDGE_CHARACTER_SUBJECT,
                    legacy::CHARACTER_KIND_NS,
                    &env.character_id,
                ),
            )
            .await,
    );

    let all = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CHARACTER_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    let ids = bridge_report_ids(&all, "records");
    assert!(
        ids.contains(&"lgc:character_memory_fragments:hfrag_bridge".to_string()),
        "binding-local fragment missing: {ids:?}"
    );
    assert!(
        ids.contains(&"lgc:character_run_captures:op_bridge".to_string()),
        "run-capture provenance record missing: {ids:?}"
    );
    for row in all["records"].as_array().unwrap() {
        assert_eq!(
            row["scope"]["visibility"], "context",
            "a binding-local legacy row keeps its binding placement: {row}"
        );
        assert_eq!(row["scope"]["context_id"], env.binding_id.as_str());
    }

    // A shared-only selection excludes both binding-local projections.
    let shared = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CHARACTER_SUBJECT,
                    serde_json::json!({ "mode": "shared_only" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(shared["total_authorized"], 0, "{shared}");

    // Naming the binding context selects them back in.
    let context = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CHARACTER_SUBJECT,
                    serde_json::json!({
                        "mode": "selected_contexts",
                        "context_ids": [env.binding_id.clone()],
                    }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(context["total_authorized"], 2, "{context}");

    // Ownership guard: a Character subject naming a character this workspace
    // does not own projects nothing.
    let foreign = "chr_00000000000000000000000000000000";
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request("sbj_bridge_foreign", legacy::CHARACTER_KIND_NS, foreign),
            )
            .await,
    );
    let foreign_records = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    "sbj_bridge_foreign",
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(foreign_records["total_authorized"], 0, "{foreign_records}");
}

/// F1: once P5 publishes the completion marker, the bridge serves nothing —
/// list, by-id, candidate and profile reads are provider-native only, and the
/// next read of the already-open provider sees that without a reopen.
#[tokio::test]
async fn completion_marker_closes_the_legacy_projection() {
    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(BRIDGE_CREATOR_SUBJECT, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );
    let before = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CREATOR_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        before["total_authorized"], 3,
        "pre-migration the bridge projects the fragment, narrative and file: {before}"
    );

    // A provider-native record alongside the legacy surface: the marker must
    // close the projection, not the provider.
    let created = bridge_ok(
        &provider
            .create_record(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                    "kind": "note",
                    "content": { "text": BRIDGE_TEXT },
                    "scope": { "visibility": "shared" },
                    "idempotency_key": "k-marker-native",
                })),
            )
            .await,
    );
    let native_id = created["result"]["record_id"]
        .as_str()
        .expect("a created record id")
        .to_string();

    publish_migration_marker(&env, BRIDGE_SPACE).await;

    let after = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CREATOR_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        bridge_report_ids(&after, "records"),
        vec![native_id.clone()],
        "the marker withdraws the legacy projection, never the provider's own row: {after}"
    );

    let record = provider
        .get_record(
            &admin,
            bridge_record_get_request("lgc:memory_fragments:frag_bridge"),
        )
        .await;
    assert_eq!(bridge_reject_code(&record), "NOT_FOUND");
    let native = bridge_ok(
        &provider
            .get_record(&admin, bridge_record_get_request(&native_id))
            .await,
    );
    assert_eq!(
        native["record"]["content"]["text"], BRIDGE_TEXT,
        "the provider-native row is still readable by id: {native}"
    );
    let candidate = provider
        .get_candidate(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "candidate_id": "lgc:memory_pending_review:pend_bridge",
            })),
        )
        .await;
    assert_eq!(bridge_reject_code(&candidate), "NOT_FOUND");
    let candidates = bridge_ok(
        &provider
            .list_candidates(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                })),
            )
            .await,
    );
    assert_eq!(
        candidates["candidates"],
        serde_json::json!([]),
        "{candidates}"
    );
    let profile = bridge_ok(
        &provider
            .get_profile(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                })),
            )
            .await,
    );
    assert_eq!(
        profile["profiles"],
        serde_json::json!([]),
        "an unmigrated SOUL.md stops projecting after the marker: {profile}"
    );
}

/// F2: one bearer projects under exactly one canonical subject, so a subject
/// that lists nothing is a subject that can read nothing by id.
#[tokio::test]
async fn legacy_bearer_projects_under_one_canonical_subject() {
    const CANONICAL: &str = "sbj_bridge_aaa";
    const SHADOW: &str = "sbj_bridge_zzz";
    const SHADOW_PRINCIPAL: &str = "prn_bridge_shadow";

    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    for subject in [CANONICAL, SHADOW] {
        bridge_ok(
            &provider
                .ensure_subject(
                    &admin,
                    bridge_ensure_request(subject, legacy::CREATOR_KIND_NS, CREATOR),
                )
                .await,
        );
    }
    bridge_ok(
        &provider
            .grant(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": SHADOW,
                    "principal_id": SHADOW_PRINCIPAL,
                    "capabilities": ["records_read", "recall"],
                    "idempotency_key": "k-bridge-shadow-grant",
                })),
            )
            .await,
    );
    let shadow: bridge_schemas::Principal = dto(serde_json::json!({
        "principal_id": SHADOW_PRINCIPAL,
        "extensions": {},
    }));

    let canonical = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    CANONICAL,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(canonical["total_authorized"], 3, "{canonical}");

    // The second subject naming the same bearer projects nothing even though it
    // is granted: its listing and its by-id read agree instead of the listing
    // revealing an object that the by-id read reports absent.
    let shadow_page = bridge_ok(
        &provider
            .list_records(
                &shadow,
                bridge_records_request(
                    SHADOW,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(shadow_page["total_authorized"], 0, "{shadow_page}");
    assert!(bridge_report_ids(&shadow_page, "records").is_empty());
    let absent = provider
        .get_record(
            &shadow,
            bridge_record_get_request("lgc:memory_fragments:frag_bridge"),
        )
        .await;
    assert_eq!(bridge_reject_code(&absent), "NOT_FOUND");

    // The canonical subject serves the same id under its own subject.
    let served = bridge_ok(
        &provider
            .get_record(
                &admin,
                bridge_record_get_request("lgc:memory_fragments:frag_bridge"),
            )
            .await,
    );
    assert_eq!(served["record"]["subject_id"], CANONICAL);
}

/// F3: a projected record keeps revision 1, so the cursor marker carries a
/// digest of the projection: rewriting a bearer file between two pages
/// invalidates the cursor instead of shifting its offset.
#[tokio::test]
async fn legacy_file_change_between_pages_invalidates_the_cursor() {
    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(BRIDGE_CREATOR_SUBJECT, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );

    let first = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_page_request(BRIDGE_CREATOR_SUBJECT, 1, None),
            )
            .await,
    );
    assert_eq!(
        first["records"].as_array().map(Vec::len),
        Some(1),
        "{first}"
    );
    let cursor = first["next_cursor"]
        .as_str()
        .expect("a first page over three projected records has a next cursor")
        .to_string();

    // Same slug, same projected id and revision — a new body and timestamp.
    let bearer = MemoryBearerRef::Creator(CREATOR);
    let mut memory = nexus_creator_memory::LongTermMemory::new("story_summary");
    memory.set_body("Rewritten long-term body.");
    nexus_creator_memory::memory_io::save_memory(
        env.user_home.as_path(),
        bearer,
        "bridge-memory",
        &memory,
    )
    .expect("long-term memory rewrite");

    let stale = provider
        .list_records(
            &admin,
            bridge_records_page_request(BRIDGE_CREATOR_SUBJECT, 1, Some(&cursor)),
        )
        .await;
    assert_eq!(bridge_reject_code(&stale), "CURSOR_INVALID");
}

// ═══ NXM-5 Task 2: E4 reroute + just-in-time adoption ═════════════════════
//
// The E4 read and dismissal handlers serve the memory family through the
// pinned `MemoryPort`. A dismissal of a legacy-derived row is the architect's
// just-in-time adoption contract: the row exists only as a projected view, so
// the handler adopts it under the bridge's deterministic id and then consumes
// it through the provider — the legacy table stays byte-identical, adoption is
// idempotent, and nothing is adopted once P5's completion marker is present.

/// One provider candidate row as the store holds it: `(status, capture
/// context)`.
async fn adopted_candidate(env: &Env, candidate_id: &str) -> Option<(String, Option<String>)> {
    let pool = plain_pool(env).await;
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT status, capture_context_id FROM memory_candidates WHERE candidate_id = ?",
    )
    .bind(candidate_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    pool.close().await;
    row
}

#[tokio::test]
async fn dismissal_adopts_the_projected_row_before_consuming_it() {
    let env = seed_env().await;
    seed_creator_pending_row(&env, "pend_adopt", Some(WORLD), "2026-01-01T00:00:01Z").await;
    let (core, principal) = open_core(&env).await;
    let candidate_id = legacy::pending_review_candidate_id("pend_adopt");

    // The row is served from the bridge's projection, and a read adopts
    // nothing: adoption belongs to the mutation it precedes.
    assert_eq!(
        core.count_pending_reviews(&principal).await.unwrap().count,
        1
    );
    assert_eq!(
        adopted_candidate(&env, &candidate_id).await,
        None,
        "a read must not adopt"
    );

    let deleted = core
        .delete_pending_review(&principal, "pend_adopt".to_string())
        .await
        .expect("the projected row is adopted, then dismissed");
    assert!(deleted.success);
    assert_eq!(deleted.pending_id, "pend_adopt");

    // The provider now owns the same deterministic id, consumed in place.
    assert_eq!(
        adopted_candidate(&env, &candidate_id).await,
        Some(("consumed".to_string(), None)),
        "the dismissal consumed the adopted row under the bridge's id"
    );
    // …and the legacy table was never written (compass D6: write-idle).
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1
    );
    pool.close().await;

    // The consumed row is gone from the retained DTO surface, and a repeat
    // dismissal finds nothing to dismiss.
    assert_eq!(
        core.count_pending_reviews(&principal).await.unwrap().count,
        0
    );
    assert!(core
        .list_pending_reviews(&principal, None, PAGE_SIZE)
        .await
        .unwrap()
        .items
        .is_empty());
    let err = core
        .delete_pending_review(&principal, "pend_adopt".to_string())
        .await
        .expect_err("a consumed row is gone");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");

    // Idempotency and no standalone adoption (fix round 3, finding F1): the
    // repeat dismissal above neither duplicated nor resurrected the row, and
    // adoption exists only inside the dismissal that consumes it.
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_candidates WHERE candidate_id = 'lgc:memory_pending_review:pend_adopt'"
        )
        .await,
        1,
        "no adoption path can duplicate the row"
    );
    pool.close().await;
}

#[tokio::test]
async fn character_dismissal_adopts_the_projected_row_before_consuming_it() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let bind1 = env.binding_id.clone();
    core.capture_character_pending_review(
        &principal,
        chr.clone(),
        dto(json_capture(
            "pend_char_adopt",
            Some(&bind1),
            "2026-01-01T00:00:01Z",
        )),
    )
    .await
    .expect("binding-local capture");

    core.delete_character_pending_review(&principal, chr.clone(), "pend_char_adopt".to_string())
        .await
        .expect("the projected row is adopted, then dismissed");

    assert_eq!(
        adopted_candidate(
            &env,
            &legacy::character_pending_review_candidate_id("pend_char_adopt")
        )
        .await,
        Some(("consumed".to_string(), Some(bind1.clone()))),
        "the adopted row keeps its binding placement"
    );
    assert_eq!(
        core.count_character_pending_reviews(&principal, chr, Some(bind1))
            .await
            .unwrap()
            .count,
        0
    );
}

#[tokio::test]
async fn just_in_time_adoption_is_inert_after_the_completion_marker() {
    let env = seed_env().await;
    seed_creator_pending_row(&env, "pend_post_marker", None, "2026-01-01T00:00:01Z").await;
    // A provider-native candidate alongside the legacy row: the marker closes
    // the legacy projection only, so this one stays readable.
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let subject = legacy::creator_subject_id(CREATOR);
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(&subject, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );
    let native_candidate =
        native_pending_candidate(&provider, &admin, &subject, "k-marker-native-candidate").await;
    publish_migration_marker(&env, BRIDGE_SPACE).await;
    let (core, principal) = open_core(&env).await;

    // Post-migration reads serve provider-native rows only: the bridge closes.
    assert_eq!(
        core.count_pending_reviews(&principal).await.unwrap().count,
        0
    );
    assert!(core
        .list_pending_reviews(&principal, None, PAGE_SIZE)
        .await
        .unwrap()
        .items
        .is_empty());

    let err = core
        .delete_pending_review(&principal, "pend_post_marker".to_string())
        .await
        .expect_err("nothing is projected to dismiss");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");
    assert_eq!(
        adopted_candidate(
            &env,
            &legacy::pending_review_candidate_id("pend_post_marker")
        )
        .await,
        None,
        "no adoption path runs after the marker"
    );
    assert_eq!(
        provider_candidate_status(&provider, &admin, &native_candidate).await,
        "pending",
        "the marker leaves the provider's own candidate alone"
    );
    let legacy_candidate = provider
        .get_candidate(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "candidate_id": legacy::pending_review_candidate_id("pend_post_marker"),
            })),
        )
        .await;
    assert_eq!(
        bridge_reject_code(&legacy_candidate),
        "NOT_FOUND",
        "the legacy candidate is no longer projected"
    );
}

// ═══ NXM-5 Task 2 fix round 1 ═════════════════════════════════════════════
//
// F1: a row a provider dismissal consumed keeps its legacy row (a provider
// write never touches the legacy surface), so the legacy review drain has to
// check the provider's state before it classifies a digest the caller rejected.
// F2: the retained cross-Creator `pending_review` 403. F4: adoption and
// dismissal are one guarded transaction. F5: no blanket scan cap in the retained
// list/count/search semantics. F6: a read-only core keeps serving memory reads
// and still refuses writes.

const REJECTED_DIGEST: &str =
    "Rejected research summary, long enough to classify as a fragment if a drain ever saw it.";

async fn seed_creator_pending_digest(env: &Env, pending_id: &str, digest: &str, created_at: &str) {
    let pool = plain_pool(env).await;
    nexus_local_db::create_pending_review(
        &pool,
        &nexus_local_db::PendingReviewRecord {
            pending_id: pending_id.to_string(),
            session_id: format!("sess_{pending_id}"),
            creator_id: CREATOR.to_string(),
            world_id: None,
            task_kind: "research".to_string(),
            raw_digest: digest.to_string(),
            created_at: created_at.to_string(),
        },
    )
    .await
    .unwrap();
    pool.close().await;
}

async fn seed_foreign_creator_pending_row(env: &Env, pending_id: &str) {
    let pool = plain_pool(env).await;
    nexus_local_db::create_pending_review(
        &pool,
        &nexus_local_db::PendingReviewRecord {
            pending_id: pending_id.to_string(),
            session_id: format!("sess_{pending_id}"),
            creator_id: OTHER.to_string(),
            world_id: None,
            task_kind: "research".to_string(),
            raw_digest: FRAGMENT_DIGEST.to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        },
    )
    .await
    .unwrap();
    pool.close().await;
}

/// Seed `count` Creator pending rows whose `created_at` sorts by row index, in
/// one statement (a fixture past the former 10_000-row scan cap).
async fn seed_creator_pending_past_former_cap(env: &Env, count: i64) {
    let pool = plain_pool(env).await;
    sqlx::query(
        "WITH RECURSIVE seq(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM seq WHERE i + 1 < ?)
         INSERT INTO memory_pending_review
             (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
         SELECT printf('pending_cap_%06d', i), printf('sess_cap_%06d', i), ?, NULL, 'research', ?,
                printf('2026-01-01T00:00:00.%06dZ', i)
         FROM seq",
    )
    .bind(count)
    .bind(CREATOR)
    .bind(FRAGMENT_DIGEST)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// Seed `count` Character-shared pending rows in one statement.
async fn seed_character_pending_past_former_cap(env: &Env, count: i64) {
    let pool = plain_pool(env).await;
    sqlx::query(
        "WITH RECURSIVE seq(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM seq WHERE i + 1 < ?)
         INSERT INTO character_memory_pending_review
             (pending_id, session_id, character_id, actor_world_binding_id, task_kind, raw_digest,
              created_at)
         SELECT printf('pending_chr_%06d', i), printf('sess_chr_%06d', i), ?, NULL, 'research', ?,
                printf('2026-01-01T00:00:00.%06dZ', i)
         FROM seq",
    )
    .bind(count)
    .bind(&env.character_id)
    .bind(FRAGMENT_DIGEST)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// Seed `count` Creator fragments; only the last one carries `kw_past_cap`.
async fn seed_creator_fragments_past_former_cap(env: &Env, count: i64) {
    let pool = plain_pool(env).await;
    sqlx::query(
        "WITH RECURSIVE seq(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM seq WHERE i + 1 < ?)
         INSERT INTO memory_fragments
             (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id)
         SELECT printf('frag_cap_%06d', i), printf('sess_fcap_%06d', i), ?,
                CASE WHEN i + 1 = ? THEN '[\"kw_past_cap\"]' ELSE '[\"kw_other\"]' END,
                printf('Seeded fragment %06d.', i),
                printf('2026-01-01T00:00:00.%06dZ', i), NULL, NULL
         FROM seq",
    )
    .bind(count)
    .bind(CREATOR)
    .bind(count)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// The provider's merged view of one candidate's status.
async fn provider_candidate_status(
    provider: &LocalMemoryProvider,
    admin: &bridge_schemas::Principal,
    candidate_id: &str,
) -> String {
    let fetched = bridge_ok(
        &provider
            .get_candidate(
                admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "candidate_id": candidate_id,
                })),
            )
            .await,
    );
    fetched["candidate"]["status"]
        .as_str()
        .expect("candidate status")
        .to_string()
}

#[tokio::test]
async fn dismissed_creator_digest_is_never_classified_by_the_review_drain() {
    let env = seed_env().await;
    seed_creator_pending_digest(
        &env,
        "pending_keep",
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    seed_creator_pending_digest(
        &env,
        "pending_rejected",
        REJECTED_DIGEST,
        "2026-01-01T00:00:02Z",
    )
    .await;
    let (core, principal) = open_core(&env).await;

    core.delete_pending_review(&principal, "pending_rejected".to_string())
        .await
        .expect("the rejected row is dismissed");

    let reviewed = core
        .review_memory(&principal, creator_review_request())
        .await
        .expect("review drains what is still queued");
    assert_eq!(
        reviewed.processed,
        Some(1),
        "the consumed row is not a classification input"
    );
    assert_eq!(reviewed.fragmented, 1, "only the kept digest is classified");
    assert_eq!(
        reviewed.has_more,
        Some(false),
        "a consumed row must not keep the drain reporting work"
    );

    let pool = plain_pool(&env).await;
    assert_eq!(
        provider_fact_bodies(&pool, &legacy::creator_subject_id(CREATOR)).await,
        vec![FRAGMENT_DIGEST.to_string()],
        "only the kept digest is classified"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        2,
        "the drain writes no legacy row; both queued rows stay untouched"
    );
    pool.close().await;
}

#[tokio::test]
async fn dismissed_character_digest_is_never_classified_by_the_review_drain() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let bind1 = env.binding_id.clone();

    core.capture_character_pending_review(
        &principal,
        chr.clone(),
        dto(json_capture(
            "pend_char_keep",
            Some(&bind1),
            "2026-01-01T00:00:01Z",
        )),
    )
    .await
    .expect("kept capture");
    let mut rejected = json_capture("pend_char_rejected", Some(&bind1), "2026-01-01T00:00:02Z");
    rejected["raw_digest"] = serde_json::json!(REJECTED_DIGEST);
    core.capture_character_pending_review(&principal, chr.clone(), dto(rejected))
        .await
        .expect("rejected capture");

    core.delete_character_pending_review(&principal, chr.clone(), "pend_char_rejected".to_string())
        .await
        .expect("dismissal");

    let reviewed = core
        .review_character_memory(
            &principal,
            chr.clone(),
            dto::<ReviewCharacterMemoryRequest>(serde_json::json!({ "binding_id": bind1 })),
        )
        .await
        .expect("review drains what is still queued");
    assert_eq!(reviewed.processed, Some(1), "the consumed row is skipped");
    assert_eq!(reviewed.fragmented, 1);
    assert_eq!(reviewed.has_more, Some(false));

    let pool = plain_pool(&env).await;
    assert_eq!(
        provider_fact_bodies(&pool, &legacy::character_subject_id(&chr)).await,
        vec![FRAGMENT_DIGEST.to_string()],
        "only the kept digest is classified"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM character_memory_pending_review"
        )
        .await,
        2,
        "the drain writes no legacy row; both queued rows stay untouched"
    );
    pool.close().await;
}

#[tokio::test]
async fn cross_creator_dismissal_keeps_the_pending_review_forbidden_reason() {
    let env = seed_env().await;
    seed_foreign_creator_pending_row(&env, "pending_foreign").await;
    let (core, principal) = open_core(&env).await;

    match core
        .delete_pending_review(&principal, "pending_foreign".to_string())
        .await
        .expect_err("another Creator's row must be refused")
    {
        CoreError::ForbiddenReason { resource, reason } => {
            assert_eq!(resource, "pending_review");
            assert!(
                reason.contains("does not belong to creator"),
                "retained reason text, got {reason}"
            );
        }
        other => panic!("expected ForbiddenReason, got {other:?}"),
    }

    // An id no row carries is still the retained 404.
    let err = core
        .delete_pending_review(&principal, "pending_absent".to_string())
        .await
        .expect_err("an absent row is not found");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");

    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_pending_review WHERE pending_id = 'pending_foreign'"
        )
        .await,
        1,
        "the refused dismissal mutates nothing"
    );
    pool.close().await;
}

/// QC1-F3: a failed ownership re-read is the retained `database_error`
/// internal, never the 404 an absence answers.
#[tokio::test]
async fn dismissal_ownership_lookup_failure_is_a_database_error() {
    let env = seed_env().await;
    // Another Creator's row: the provider cannot see it, so the dismissal falls
    // through to the legacy ownership re-read that distinguishes 403 from 404.
    seed_foreign_creator_pending_row(&env, "pending_foreign").await;
    let (core, principal) = open_core(&env).await;
    // Closing the core pool fails exactly that re-read; the provider keeps its
    // own pool, so the reads ahead of it still answer.
    core.pool().close().await;

    let err = core
        .delete_pending_review(&principal, "pending_foreign".to_string())
        .await
        .expect_err("a failed ownership lookup is not an absence");
    match err {
        CoreError::Internal { category } => assert!(
            category.starts_with("database_error"),
            "the pre-reroute database-error class, got {category}"
        ),
        other => panic!("expected the database_error internal, got {other:?}"),
    }
}

#[tokio::test]
async fn legacy_dismissal_and_adoption_commit_together() {
    let env = seed_env().await;
    seed_creator_pending_row(&env, "pend_atomic", None, "2026-01-01T00:00:01Z").await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let subject = legacy::creator_subject_id(CREATOR);
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(&subject, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );
    let candidate_id = legacy::pending_review_candidate_id("pend_atomic");

    // A commit that never lands leaves neither half: no adopted row, and the
    // projected row is still pending for the next reader.
    provider.inject_commit_fault(false);
    let failed = provider
        .dismiss_legacy_candidate(&admin, &subject, &candidate_id, None, "atomic-1")
        .await;
    assert!(
        matches!(failed, SpokeResult::Reject(_)),
        "a failed commit refuses"
    );
    assert_eq!(
        adopted_candidate(&env, &candidate_id).await,
        None,
        "the adoption rolled back with the dismissal"
    );
    assert_eq!(
        provider_candidate_status(&provider, &admin, &candidate_id).await,
        "pending"
    );

    // A CAS that cannot land rolls the adoption back too: the two effects are
    // one transaction, so a rejected dismissal leaves no adopted pending row.
    let stale = provider
        .dismiss_legacy_candidate(&admin, &subject, &candidate_id, Some(99), "atomic-2")
        .await;
    assert_eq!(bridge_reject_code(&stale), "REVISION_CONFLICT");
    assert_eq!(
        adopted_candidate(&env, &candidate_id).await,
        None,
        "a rejected dismissal leaves no adopted row behind"
    );
    assert_eq!(
        provider_candidate_status(&provider, &admin, &candidate_id).await,
        "pending"
    );

    // The same call without a caller CAS adopts and consumes atomically.
    bridge_ok(
        &provider
            .dismiss_legacy_candidate(&admin, &subject, &candidate_id, None, "atomic-3")
            .await,
    );
    assert_eq!(
        adopted_candidate(&env, &candidate_id).await,
        Some(("consumed".to_string(), None))
    );
}

#[tokio::test]
async fn pending_listing_serves_rows_past_the_former_scan_cap() {
    const TOTAL: i64 = 10_001;
    let env = seed_env().await;
    seed_creator_pending_past_former_cap(&env, TOTAL).await;
    let (core, principal) = open_core(&env).await;

    assert_eq!(
        core.count_pending_reviews(&principal).await.unwrap().count,
        TOTAL,
        "a queue past the former 10_000-row cap is counted, not refused"
    );
    let first = core
        .list_pending_reviews(&principal, None, 2)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 2);
    assert!(
        first.pagination.has_more,
        "a first page of a large queue must not report completion"
    );
    assert!(first.pagination.next_cursor.is_some());
}

/// QC1-F1: two rows inside one second whose ids order opposite to their
/// fractional timestamps keep the retained `created_at DESC` order and serve
/// their original timestamp values, across a keyset page boundary.
#[tokio::test]
async fn pending_pages_keep_sub_second_order_opposite_to_ids() {
    let env = seed_env().await;
    // `pending_a` sorts before `pending_z` by id but is the newer row.
    seed_creator_pending_row(&env, "pending_a", None, "2026-01-01T00:00:00.900000Z").await;
    seed_creator_pending_row(&env, "pending_z", None, "2026-01-01T00:00:00.100000Z").await;
    let (core, principal) = open_core(&env).await;

    let first = core
        .list_pending_reviews(&principal, None, 1)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(
        first.items[0].pending_id, "pending_a",
        "the newer fractional timestamp orders first, not the larger id"
    );
    assert_eq!(
        first.items[0].created_at, "2026-01-01T00:00:00.900000Z",
        "the legacy timestamp is served verbatim, sub-second precision included"
    );
    assert!(first.pagination.has_more, "the older row is still queued");
    let cursor = first
        .pagination
        .next_cursor
        .clone()
        .expect("a partial page carries a cursor");

    let second = core
        .list_pending_reviews(&principal, Some(cursor), 1)
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].pending_id, "pending_z");
    assert_eq!(second.items[0].created_at, "2026-01-01T00:00:00.100000Z");
    assert!(!second.pagination.has_more);
}

/// QC1-F1 fix round 2: one pending queue mixes an RFC 3339 whole second, a
/// fractional RFC 3339 value and a timezone-less bare-`T` legacy value. Ordering
/// by the shared legacy parser keeps the retained `created_at DESC` order and
/// its keyset page boundaries consistent with one total order.
#[tokio::test]
async fn pending_mixed_timestamp_formats_order_as_instants_across_pages() {
    let env = seed_env().await;
    seed_creator_pending_row(&env, "pending_a", None, "2026-01-01T00:00:00Z").await;
    seed_creator_pending_row(&env, "pending_b", None, "2026-01-01T00:00:00.900Z").await;
    seed_creator_pending_row(&env, "pending_c", None, "2026-01-01T00:00:00.950").await;
    let (core, principal) = open_core(&env).await;

    let first = core
        .list_pending_reviews(&principal, None, 1)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(
        first.items[0].pending_id, "pending_c",
        "the timezone-less bare-T value is the newest, not the textually largest"
    );
    assert_eq!(
        first.items[0].created_at, "2026-01-01T00:00:00.950",
        "the legacy value is served verbatim"
    );
    assert!(first.pagination.has_more);
    let cursor = first
        .pagination
        .next_cursor
        .clone()
        .expect("a partial page carries a cursor");

    let second = core
        .list_pending_reviews(&principal, Some(cursor), 1)
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].pending_id, "pending_b");
    assert_eq!(second.items[0].created_at, "2026-01-01T00:00:00.900Z");
    assert!(second.pagination.has_more);
    let cursor = second
        .pagination
        .next_cursor
        .clone()
        .expect("a partial page carries a cursor");

    let third = core
        .list_pending_reviews(&principal, Some(cursor), 1)
        .await
        .unwrap();
    assert_eq!(third.items.len(), 1);
    assert_eq!(third.items[0].pending_id, "pending_a");
    assert_eq!(third.items[0].created_at, "2026-01-01T00:00:00Z");
    assert!(!third.pagination.has_more);
    assert!(third.pagination.next_cursor.is_none());
}

#[tokio::test]
async fn character_offset_window_serves_rows_past_the_former_scan_cap() {
    const TOTAL: i64 = 10_001;
    let env = seed_env().await;
    seed_character_pending_past_former_cap(&env, TOTAL).await;
    let (core, principal) = open_core(&env).await;

    // The window starts inside what the former clamp hid: it used to answer an
    // empty page with `has_more = false` although the row existed.
    let page = core
        .list_character_pending_reviews(&principal, env.character_id.clone(), None, 5, 10_000)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1, "the row past the old window is served");
    assert!(!page.pagination.has_more);
}

#[tokio::test]
async fn keyword_search_finds_a_fragment_past_the_former_scan_cap() {
    const TOTAL: i64 = 10_001;
    let env = seed_env().await;
    seed_creator_fragments_past_former_cap(&env, TOTAL).await;
    let (core, principal) = open_core(&env).await;

    let found = core
        .list_memory_fragments(&principal, Some("kw_past_cap".to_string()), None, 10)
        .await
        .unwrap();
    assert_eq!(
        found.fragments.len(),
        1,
        "a keyword match past the former scan bound is not silently dropped"
    );
    assert_eq!(
        found.fragments[0]
            .keywords
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["kw_past_cap"]
    );
}

#[tokio::test]
async fn read_only_core_serves_memory_reads_and_refuses_the_write() {
    let env = seed_env().await;
    seed_creator_pending_row(&env, "pend_read_only", None, "2026-01-01T00:00:01Z").await;
    // A writer open ensures the provider subject the legacy projection needs;
    // the read-only core below serves the same read without writing anything.
    {
        let (core, principal) = open_core(&env).await;
        assert_eq!(
            core.count_pending_reviews(&principal).await.unwrap().count,
            1
        );
    }

    let read_only = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .expect("read-only open");
    let principal = read_only.active_principal().await.unwrap();
    assert_eq!(
        read_only
            .count_pending_reviews(&principal)
            .await
            .unwrap()
            .count,
        1,
        "a read-only core still serves the pending count"
    );
    let listed = read_only
        .list_pending_reviews(&principal, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].pending_id, "pend_read_only");

    let err = read_only
        .delete_pending_review(&principal, "pend_read_only".to_string())
        .await
        .expect_err("a read-only core must refuse the dismissal");
    assert!(
        matches!(err, CoreError::Forbidden { ref resource } if resource == "memory: read-only core access"),
        "got {err:?}"
    );
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1,
        "the refused dismissal mutates nothing"
    );
    pool.close().await;
}

// ═══ NXM-5 Task 2 fix round 2 ═════════════════════════════════════════════
//
// R2-F1: a provider dismissal and a legacy review drain decide the same digest,
// so the drain holds the bearer's disposition section across its consumed-check,
// classification and persistence, and a dismissal must hold the same section to
// consume. Each regression parks the drain at a test-controlled step-point — no
// sleeps, no timing — and asserts the dismissal cannot enter that window.
// R2-F2: a read-only open projects the legacy rows of a bearer no writer ever
// ensured, and answers the space's absence when nothing was ever provisioned.

/// Seed one Character pending row bound to `binding_id`.
async fn seed_character_pending_binding(env: &Env, pending_id: &str, binding_id: &str) {
    let pool = plain_pool(env).await;
    sqlx::query(
        "INSERT INTO character_memory_pending_review
             (pending_id, session_id, character_id, actor_world_binding_id, task_kind, raw_digest,
              created_at)
         VALUES (?, ?, ?, ?, 'research', ?, '2026-01-01T00:00:01Z')",
    )
    .bind(pending_id)
    .bind(format!("sess_{pending_id}"))
    .bind(&env.character_id)
    .bind(binding_id)
    .bind(FRAGMENT_DIGEST)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// Arm one of the drain's step-points.
fn arm_review_pause(
    core: &CoreService,
    at: MemoryReviewPause,
) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    core.pause_memory_review(MemoryReviewRendezvous {
        at,
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });
    (entered, release)
}

#[tokio::test]
async fn creator_dismissal_cannot_commit_inside_the_review_decision() {
    let env = seed_env().await;
    seed_creator_pending_digest(
        &env,
        "pending_window",
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    let (core, principal) = open_core(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let subject = legacy::creator_subject_id(CREATOR);
    let candidate_id = legacy::pending_review_candidate_id("pending_window");

    let (entered, release) = arm_review_pause(&core, MemoryReviewPause::AfterRecheck);
    let drain_task = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        async move {
            core.review_memory(&principal, creator_review_request())
                .await
        }
    });
    entered.notified().await;

    // The drain's check ran and saw the row pending, and the section it holds is
    // the one a dismissal must take to consume that row: there is no window
    // between the check and the classification for the dismissal to commit in.
    assert_eq!(
        provider_candidate_status(&provider, &admin, &candidate_id).await,
        "pending",
        "the checked row was still queued when the drain decided to classify it"
    );
    assert!(
        !provider.disposition_available(&subject),
        "the review decision is not interleavable with a dismissal"
    );

    // The dismissal-side wait signal (fix round 4, finding R4-F3): it fires
    // from inside the lock attempt only after the non-blocking probe failed, so
    // the test observes a real contended acquisition — not merely that the task
    // had not finished yet. The drain stays held until that signal arrives.
    let pinned = core.memory_provider_for_test().expect("pinned provider");
    let waiting = Arc::new(tokio::sync::Notify::new());
    pinned.report_disposition_waits(Arc::clone(&waiting));

    let dismissal = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        async move {
            core.delete_pending_review(&principal, "pending_window".to_string())
                .await
        }
    });
    waiting.notified().await;
    assert!(
        !provider.disposition_available(&subject),
        "the dismissal found the section held and is blocked on its lock"
    );
    assert!(
        !dismissal.is_finished(),
        "the dismissal cannot complete while the drain holds the section"
    );

    release.notify_one();
    let reviewed = drain_task.await.unwrap().expect("review drains the row");
    assert_eq!(
        reviewed.processed,
        Some(1),
        "the row the drain checked is still classified"
    );
    assert_eq!(reviewed.fragmented, 1);
    // The drain consumed the row first, so the dismissal that was excluded from
    // its window is the retained `NotFound` — it never ran inside the decision.
    let late = dismissal
        .await
        .unwrap()
        .expect_err("the dismissal lost the serialization");
    assert!(
        matches!(late, CoreError::NotFound { .. }),
        "the excluded dismissal finds the row already consumed: {late:?}"
    );
    assert!(
        provider.disposition_available(&subject),
        "the section is released once the row is persisted"
    );

    let pool = plain_pool(&env).await;
    assert_eq!(
        provider_fact_bodies(&pool, &legacy::creator_subject_id(CREATOR)).await,
        vec![FRAGMENT_DIGEST.to_string()]
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1,
        "the drain consumes the provider candidate, never the legacy row"
    );
    pool.close().await;
}

#[tokio::test]
async fn character_dismissal_cannot_commit_inside_the_review_decision() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let bind1 = env.binding_id.clone();
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let subject = legacy::character_subject_id(&chr);
    let candidate_id = legacy::character_pending_review_candidate_id("pend_window");
    core.capture_character_pending_review(
        &principal,
        chr.clone(),
        dto(json_capture(
            "pend_window",
            Some(&bind1),
            "2026-01-01T00:00:01Z",
        )),
    )
    .await
    .expect("capture");

    let (entered, release) = arm_review_pause(&core, MemoryReviewPause::AfterRecheck);
    let drain_task = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        let chr = chr.clone();
        let bind = bind1.clone();
        async move {
            core.review_character_memory(
                &principal,
                chr,
                dto::<ReviewCharacterMemoryRequest>(serde_json::json!({ "binding_id": bind })),
            )
            .await
        }
    });
    entered.notified().await;

    assert_eq!(
        provider_candidate_status(&provider, &admin, &candidate_id).await,
        "pending",
        "the checked row was still queued when the drain decided to classify it"
    );
    assert!(
        !provider.disposition_available(&subject),
        "the review decision is not interleavable with a dismissal"
    );

    // Same dismissal-side wait signal as the Creator arm (fix round 4, finding
    // R4-F3): it fires from inside the contending lock attempt.
    let pinned = core.memory_provider_for_test().expect("pinned provider");
    let waiting = Arc::new(tokio::sync::Notify::new());
    pinned.report_disposition_waits(Arc::clone(&waiting));

    let dismissal = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        let chr = chr.clone();
        async move {
            core.delete_character_pending_review(&principal, chr, "pend_window".to_string())
                .await
        }
    });
    waiting.notified().await;
    assert!(
        !provider.disposition_available(&subject),
        "the dismissal found the section held and is blocked on its lock"
    );
    assert!(
        !dismissal.is_finished(),
        "the dismissal cannot complete while the drain holds the section"
    );

    release.notify_one();
    let reviewed = drain_task.await.unwrap().expect("review drains the row");
    assert_eq!(reviewed.processed, Some(1));
    assert_eq!(reviewed.fragmented, 1);
    // The drain consumed the row first: the excluded dismissal finds nothing.
    let late = dismissal
        .await
        .unwrap()
        .expect_err("the dismissal lost the serialization");
    assert!(
        matches!(late, CoreError::NotFound { .. }),
        "the excluded dismissal finds the row already consumed: {late:?}"
    );
    assert!(provider.disposition_available(&subject));

    let pool = plain_pool(&env).await;
    assert_eq!(
        provider_fact_bodies(&pool, &legacy::character_subject_id(&chr)).await,
        vec![FRAGMENT_DIGEST.to_string()]
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM character_memory_pending_review"
        )
        .await,
        1,
        "the drain consumes the provider candidate, never the legacy row"
    );
    pool.close().await;
}

/// R2-F1: a dismissal that commits between the fetch-time check and the
/// disposition section is re-verified inside the section and its row is dropped
/// — the digest is never classified and no fragment is created.
#[tokio::test]
async fn creator_dismissal_between_check_and_classification_is_reverified() {
    let env = seed_env().await;
    seed_creator_pending_digest(
        &env,
        "pending_window",
        REJECTED_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    let (core, principal) = open_core(&env).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let candidate_id = legacy::pending_review_candidate_id("pending_window");

    let (entered, release) = arm_review_pause(&core, MemoryReviewPause::AfterFetchCheck);
    let drain_task = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        async move {
            core.review_memory(&principal, creator_review_request())
                .await
        }
    });
    entered.notified().await;

    // The fetch-time check saw the row pending, and the dismissal commits in the
    // window before the drain takes the disposition section.
    assert_eq!(
        provider_candidate_status(&provider, &admin, &candidate_id).await,
        "pending"
    );
    core.delete_pending_review(&principal, "pending_window".to_string())
        .await
        .expect("the dismissal wins the window");
    assert_eq!(
        provider_candidate_status(&provider, &admin, &candidate_id).await,
        "consumed"
    );

    release.notify_one();
    let reviewed = drain_task.await.unwrap().expect("review");
    assert_eq!(
        reviewed.processed,
        Some(0),
        "the re-check drops the consumed row instead of classifying it"
    );
    assert_eq!(
        reviewed.fragmented, 0,
        "a dismissed digest is never classified"
    );
    assert_eq!(reviewed.has_more, Some(false));

    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_fragments").await,
        0,
        "no fragment is created from a dismissed digest"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1,
        "the dismissal wrote no legacy row and the drain never classified it"
    );
    pool.close().await;
}

/// The Character arm of
/// [`creator_dismissal_between_check_and_classification_is_reverified`].
#[tokio::test]
async fn character_dismissal_between_check_and_classification_is_reverified() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let bind1 = env.binding_id.clone();
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let candidate_id = legacy::character_pending_review_candidate_id("pend_window");
    let mut rejected = json_capture("pend_window", Some(&bind1), "2026-01-01T00:00:01Z");
    rejected["raw_digest"] = serde_json::json!(REJECTED_DIGEST);
    core.capture_character_pending_review(&principal, chr.clone(), dto(rejected))
        .await
        .expect("capture");

    let (entered, release) = arm_review_pause(&core, MemoryReviewPause::AfterFetchCheck);
    let drain_task = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        let chr = chr.clone();
        let bind = bind1.clone();
        async move {
            core.review_character_memory(
                &principal,
                chr,
                dto::<ReviewCharacterMemoryRequest>(serde_json::json!({ "binding_id": bind })),
            )
            .await
        }
    });
    entered.notified().await;
    assert_eq!(
        provider_candidate_status(&provider, &admin, &candidate_id).await,
        "pending"
    );
    core.delete_character_pending_review(&principal, chr.clone(), "pend_window".to_string())
        .await
        .expect("the dismissal wins the window");

    release.notify_one();
    let reviewed = drain_task.await.unwrap().expect("review");
    assert_eq!(reviewed.processed, Some(0));
    assert_eq!(reviewed.fragmented, 0);
    assert_eq!(reviewed.has_more, Some(false));

    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_memory_fragments").await,
        0
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM character_memory_pending_review"
        )
        .await,
        1
    );
    pool.close().await;
}

#[tokio::test]
async fn read_only_core_projects_bearers_no_writer_ever_ensured() {
    let env = seed_env().await;
    seed_creator_pending_digest(
        &env,
        "pending_unensured",
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;
    seed_creator_fragments(&env, 1).await;
    let binding = env.binding_id.clone();
    seed_character_pending_binding(&env, "pending_chr_unensured", &binding).await;
    // Provision the space only: a writer open registers the space and the
    // bootstrap grant and ensures no subject at all.
    let provider = open_bridge_provider(&env).await;
    drop(provider);

    let read_only = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .expect("read-only open");
    let principal = read_only.active_principal().await.unwrap();

    assert_eq!(
        read_only
            .count_pending_reviews(&principal)
            .await
            .unwrap()
            .count,
        1,
        "an unensured Creator bearer still serves its legacy pending row"
    );
    let listed = read_only
        .list_pending_reviews(&principal, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].pending_id, "pending_unensured");
    let fragments = read_only
        .list_memory_fragments(&principal, None, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(
        fragments.fragments.len(),
        1,
        "an unensured Creator bearer still serves its legacy fragments"
    );

    let character = read_only
        .list_character_pending_reviews(
            &principal,
            env.character_id.clone(),
            Some(binding),
            u32::try_from(PAGE_SIZE).unwrap(),
            0,
        )
        .await
        .unwrap();
    assert_eq!(
        character.items.len(),
        1,
        "an unensured Character bearer still serves its legacy pending row"
    );
    assert_eq!(
        character.items[0].pending_id.as_str(),
        "pending_chr_unensured"
    );

    // The projection is a read: no subject, receipt or candidate was created.
    let pool = plain_pool(&env).await;
    for table in ["memory_subjects", "memory_receipts", "memory_candidates"] {
        assert_eq!(
            count_where(&pool, &format!("SELECT COUNT(*) FROM {table}")).await,
            0,
            "{table} stays write-idle on a read-only projection"
        );
    }
    pool.close().await;
}

/// NXM-5 Task 2 fix round 3 (R3-F2/R3-F3): a read-only core opened before any
/// writer provisioned the workspace answers the space's absence; once a writer
/// provisions that same workspace, the SAME read-only instance serves its reads
/// — no reopen and no persistent write from the read path.
#[tokio::test]
async fn read_only_core_over_a_never_provisioned_workspace_answers_not_found() {
    let env = seed_unprovisioned_env();

    let read_only = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .expect("read-only open over the pre-provider workspace");
    let principal = read_only.active_principal().await.unwrap();

    assert!(
        matches!(
            read_only.count_pending_reviews(&principal).await,
            Err(CoreError::NotFound { .. })
        ),
        "a workspace with no provider schema answers the space's absence"
    );
    assert!(matches!(
        read_only
            .list_pending_reviews(&principal, None, PAGE_SIZE)
            .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        read_only
            .list_memory_fragments(&principal, None, None, PAGE_SIZE)
            .await,
        Err(CoreError::NotFound { .. })
    ));

    // A writer opens the same workspace afterwards: migrations and provisioning
    // commit while this read-only service stays open.
    let writer = open_bridge_provider(&env).await;
    seed_creator_pending_digest(
        &env,
        "pending_no_space",
        FRAGMENT_DIGEST,
        "2026-01-01T00:00:01Z",
    )
    .await;

    assert_eq!(
        read_only
            .count_pending_reviews(&principal)
            .await
            .unwrap()
            .count,
        1,
        "the pinned read-only provider observes the later provisioning"
    );
    let listed = read_only
        .list_pending_reviews(&principal, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].pending_id, "pending_no_space");
    assert_eq!(
        read_only
            .list_memory_fragments(&principal, None, None, PAGE_SIZE)
            .await
            .unwrap()
            .fragments
            .len(),
        0,
        "the read serves the provider/bridge state, not a stale snapshot"
    );

    // The read path wrote nothing: no subject was ensured and no projection was
    // persisted by the read-only instance.
    let pool = plain_pool(&env).await;
    for table in ["memory_subjects", "memory_receipts", "memory_candidates"] {
        assert_eq!(
            count_where(&pool, &format!("SELECT COUNT(*) FROM {table}")).await,
            0,
            "{table} stays write-idle across the read-only open and its reads"
        );
    }
    pool.close().await;
    drop(writer);
}

/// NXM-5 Task 2 fix round 2/3: a provider schema with no space row still
/// answers the space's absence on every read entry point (the static case the
/// open-before-provision regression no longer covers, since it starts from a
/// database with no schema at all).
#[tokio::test]
async fn read_only_core_answers_a_missing_space_row_as_not_found() {
    let env = seed_env().await;

    let read_only = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .expect("read-only open");
    let principal = read_only.active_principal().await.unwrap();

    assert!(
        matches!(
            read_only.count_pending_reviews(&principal).await,
            Err(CoreError::NotFound { .. })
        ),
        "an unprovisioned space answers the space's absence"
    );
    assert!(matches!(
        read_only
            .list_pending_reviews(&principal, None, PAGE_SIZE)
            .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        read_only
            .list_memory_fragments(&principal, None, None, PAGE_SIZE)
            .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        read_only
            .list_character_pending_reviews(
                &principal,
                env.character_id.clone(),
                None,
                u32::try_from(PAGE_SIZE).unwrap(),
                0,
            )
            .await,
        Err(CoreError::NotFound { .. })
    ));
}

// ═══ NXM-5 Task 3: provider-path regression gaps ═══════════════════════════
//
// The four families the reroute must keep — pre-migration merged reads,
// just-in-time adoption, stable external ids, and provider-only reads after
// P5's completion marker — are pinned by the Task 1/2 suites above. These
// regressions close what they leave open: the marker is per space, the wire
// candidate entries are not adoption paths, a record cursor survives an
// adoption that writes only the candidate family, and a marker published while
// a dismissal is in flight withdraws the projection that dismissal would adopt.

/// A second space as P5 leaves it after migrating it: a space row plus its
/// bootstrap admin grant, and no completion marker yet.
async fn seed_space_with_admin_grant(env: &Env, space_id: &str) {
    let guarded = init_guarded_pool(&env.db_path, CREATOR).await.unwrap();
    let pool = guarded.clone_pool();
    sqlx::query(
        "INSERT INTO memory_spaces \
         (space_id, display_name, authority_host_id, authority_generation, status, revision, \
          created_at) \
         VALUES (?, ?, 'nexus-local', 1, 'active', 1, '2026-03-01T00:00:00Z')",
    )
    .bind(space_id)
    .bind(space_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memory_grants \
         (grant_id, space_id, subject_id, principal_id, capabilities_json, context_ids_json, \
          shared_access, expires_at, revoked_at, revision, created_at) \
         VALUES (?, ?, NULL, ?, '[\"admin\"]', '[]', 0, NULL, NULL, 1, '2026-03-01T00:00:00Z')",
    )
    .bind(format!("grant_bootstrap_{space_id}"))
    .bind(space_id)
    .bind(CREATOR)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// P5's completion marker is per space (`legacy_content_migration:<space>`): a
/// marker published for one space closes every legacy projection of that space
/// — both bearers — and leaves another space's unmigrated projection serving
/// the same external ids.
#[tokio::test]
async fn completion_marker_closes_only_the_space_it_names() {
    const SHADOW_SPACE: &str = "spc_bridge_shadow";
    const SHADOW_SUBJECT: &str = "sbj_bridge_shadow";

    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    seed_legacy_character_content(&env).await;
    seed_space_with_admin_grant(&env, SHADOW_SPACE).await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    for (space, subject, namespace, kind_id) in [
        (
            BRIDGE_SPACE,
            BRIDGE_CREATOR_SUBJECT,
            legacy::CREATOR_KIND_NS,
            CREATOR,
        ),
        (
            BRIDGE_SPACE,
            BRIDGE_CHARACTER_SUBJECT,
            legacy::CHARACTER_KIND_NS,
            env.character_id.as_str(),
        ),
        (
            SHADOW_SPACE,
            SHADOW_SUBJECT,
            legacy::CREATOR_KIND_NS,
            CREATOR,
        ),
    ] {
        bridge_ok(
            &provider
                .ensure_subject(
                    &admin,
                    bridge_ensure_request_in(space, subject, namespace, kind_id),
                )
                .await,
        );
    }

    let creator_before = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CREATOR_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(creator_before["total_authorized"], 3, "{creator_before}");
    let character_before = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CHARACTER_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        character_before["total_authorized"], 2,
        "{character_before}"
    );
    let shadow_before = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request_in(
                    SHADOW_SPACE,
                    SHADOW_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        bridge_report_ids(&shadow_before, "records"),
        bridge_report_ids(&creator_before, "records"),
        "the unmigrated space projects the same deterministic external ids: {shadow_before}"
    );
    let shadow_candidates_before = bridge_ok(
        &provider
            .list_candidates(
                &admin,
                dto(serde_json::json!({
                    "space_id": SHADOW_SPACE,
                    "subject_id": SHADOW_SUBJECT,
                    "status": "pending",
                })),
            )
            .await,
    );
    assert_eq!(
        shadow_candidates_before["candidates"][0]["candidate_id"],
        "lgc:memory_pending_review:pend_bridge",
        "{shadow_candidates_before}"
    );

    // A provider-native record in the marked space must outlive its marker.
    let created = bridge_ok(
        &provider
            .create_record(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                    "kind": "note",
                    "content": { "text": BRIDGE_TEXT },
                    "scope": { "visibility": "shared" },
                    "idempotency_key": "k-space-native",
                })),
            )
            .await,
    );
    let native_id = created["result"]["record_id"]
        .as_str()
        .expect("a created record id")
        .to_string();

    // P5 completes the marked space only.
    publish_migration_marker(&env, BRIDGE_SPACE).await;

    // Every legacy projection entry point of that space closes, for both
    // bearers; the provider's own record keeps serving.
    let creator_closed = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CREATOR_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        bridge_report_ids(&creator_closed, "records"),
        vec![native_id.clone()],
        "the marker is not subject-scoped and never hides a provider-native row: {creator_closed}"
    );
    let character_closed = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request(
                    BRIDGE_CHARACTER_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        character_closed["total_authorized"], 0,
        "the marker is not subject-scoped: {character_closed}"
    );
    let closed_candidates = bridge_ok(
        &provider
            .list_candidates(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                    "status": "pending",
                })),
            )
            .await,
    );
    assert_eq!(
        closed_candidates["candidates"],
        serde_json::json!([]),
        "{closed_candidates}"
    );
    let closed_profile = bridge_ok(
        &provider
            .get_profile(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": BRIDGE_CREATOR_SUBJECT,
                })),
            )
            .await,
    );
    assert_eq!(
        closed_profile["profiles"],
        serde_json::json!([]),
        "{closed_profile}"
    );
    let closed_by_id = provider
        .get_record(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "record_id": "lgc:memory_fragments:frag_bridge",
            })),
        )
        .await;
    assert_eq!(bridge_reject_code(&closed_by_id), "NOT_FOUND");
    let native_by_id = bridge_ok(
        &provider
            .get_record(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "record_id": native_id,
                })),
            )
            .await,
    );
    assert_eq!(
        native_by_id["record"]["content"]["text"], BRIDGE_TEXT,
        "the provider-native record is still readable by id: {native_by_id}"
    );

    // The other space is untouched: same external ids, same queue, same SOUL.
    let shadow_after = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request_in(
                    SHADOW_SPACE,
                    SHADOW_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        bridge_report_ids(&shadow_after, "records"),
        bridge_report_ids(&creator_before, "records"),
        "a marker for another space withdraws nothing here: {shadow_after}"
    );
    let shadow_candidates = bridge_ok(
        &provider
            .list_candidates(
                &admin,
                dto(serde_json::json!({
                    "space_id": SHADOW_SPACE,
                    "subject_id": SHADOW_SUBJECT,
                    "status": "pending",
                })),
            )
            .await,
    );
    assert_eq!(
        shadow_candidates["candidates"][0]["candidate_id"], "lgc:memory_pending_review:pend_bridge",
        "{shadow_candidates}"
    );
    let shadow_by_id = bridge_ok(
        &provider
            .get_record(
                &admin,
                dto(serde_json::json!({
                    "space_id": SHADOW_SPACE,
                    "record_id": "lgc:memory_fragments:frag_bridge",
                })),
            )
            .await,
    );
    assert_eq!(
        shadow_by_id["record"]["content"]["text"], "Legacy creator fragment text.",
        "{shadow_by_id}"
    );
    let shadow_profile = bridge_ok(
        &provider
            .get_profile(
                &admin,
                dto(serde_json::json!({
                    "space_id": SHADOW_SPACE,
                    "subject_id": SHADOW_SUBJECT,
                })),
            )
            .await,
    );
    assert_eq!(
        shadow_profile["profiles"][0]["document"]["body"], BRIDGE_SOUL,
        "{shadow_profile}"
    );

    // Once that space completes too, its own marker closes it.
    publish_migration_marker(&env, SHADOW_SPACE).await;
    let shadow_closed = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_request_in(
                    SHADOW_SPACE,
                    SHADOW_SUBJECT,
                    serde_json::json!({ "mode": "subject_all" }),
                    serde_json::json!([]),
                ),
            )
            .await,
    );
    assert_eq!(
        shadow_closed["total_authorized"], 0,
        "each space has its own completion marker: {shadow_closed}"
    );
}

/// Adoption is the E4 dismissal's contract, not the wire's: `dismiss_candidate`
/// and `review_candidate` reach provider-native rows only. A projected legacy
/// row is absent to them and stays unadopted; once the dismissal has adopted and
/// consumed it, the same id is a provider-native consumed row, so the wire
/// entries refuse it as consumed instead of re-adopting or resurrecting it.
#[tokio::test]
async fn wire_candidate_mutations_never_adopt_a_projected_legacy_row() {
    let env = seed_env().await;
    seed_creator_pending_row(&env, "pend_wire", None, "2026-01-01T00:00:01Z").await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let projected = legacy::pending_review_candidate_id("pend_wire");

    // A projected id is not a provider row: both mutation entries refuse the
    // op and, crucially, adopt nothing on the way.
    let dismissed = provider
        .dismiss_candidate(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "candidate_id": projected,
                "expected_revision": 1,
                "reason": "not this entry's to adopt",
                "idempotency_key": "k-wire-dismiss",
            })),
        )
        .await;
    assert_eq!(bridge_reject_code(&dismissed), "NOT_FOUND");
    let reviewed = provider
        .review_candidate(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "candidate_id": projected,
                "expected_revision": 1,
                "decision": { "action": "drop" },
                "idempotency_key": "k-wire-review",
            })),
        )
        .await;
    assert_eq!(bridge_reject_code(&reviewed), "NOT_FOUND");
    assert_eq!(
        adopted_candidate(&env, &projected).await,
        None,
        "neither wire entry has an adoption path"
    );

    // The E4 dismissal adopts under the projected id and consumes in place; it
    // is also the path that ensures the bearer's deterministic subject.
    let (core, principal) = open_core(&env).await;
    core.delete_pending_review(&principal, "pend_wire".to_string())
        .await
        .expect("the E4 dismissal adopts the projected row");
    assert_eq!(
        adopted_candidate(&env, &projected).await,
        Some(("consumed".to_string(), None)),
        "adoption under the bridge's deterministic id"
    );

    // The wire now sees that provider-native row and refuses a second decision.
    assert_eq!(
        provider_candidate_status(&provider, &admin, &projected).await,
        "consumed"
    );
    let dismissed_again = provider
        .dismiss_candidate(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "candidate_id": projected,
                "expected_revision": 2,
                "reason": "already decided",
                "idempotency_key": "k-wire-dismiss-consumed",
            })),
        )
        .await;
    assert_eq!(bridge_reject_code(&dismissed_again), "CANDIDATE_CONSUMED");
    let reviewed_again = provider
        .review_candidate(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "candidate_id": projected,
                "expected_revision": 2,
                "decision": { "action": "drop" },
                "idempotency_key": "k-wire-review-consumed",
            })),
        )
        .await;
    assert_eq!(bridge_reject_code(&reviewed_again), "CANDIDATE_CONSUMED");

    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_candidates WHERE candidate_id = 'lgc:memory_pending_review:pend_wire'"
        )
        .await,
        1,
        "no wire entry duplicated the adopted row"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1,
        "the legacy queue row is never written by any of these paths"
    );
    pool.close().await;
}

/// A cursor minted over the merged record set stays valid across an adoption:
/// the dismissal writes candidates, receipts and the journal only, so the
/// snapshot marker of the record family is unchanged and the walk continues to
/// every projected row. A candidate cursor is different — the dismissal really
/// removed a row from that authorized set, so the old snapshot is refused
/// instead of silently re-filtering (MEM-R026).
#[tokio::test]
async fn record_cursor_survives_a_dismissal_that_adopts_a_projected_row() {
    let env = seed_env().await;
    seed_legacy_creator_content(&env).await;
    seed_creator_pending_row(&env, "pend_cursor", None, "2026-03-02T00:00:00Z").await;
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    // The bearer's deterministic subject is the one E4 addresses, so the pages
    // below are minted against the same subject (and lifecycle status) the
    // dismissal leaves behind.
    let subject = legacy::creator_subject_id(CREATOR);
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(&subject, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );

    let first = bridge_ok(
        &provider
            .list_records(&admin, bridge_records_page_request(&subject, 1, None))
            .await,
    );
    assert_eq!(first["total_authorized"], 3, "{first}");
    let cursor = first["next_cursor"]
        .as_str()
        .expect("three projected records have a second page")
        .to_string();
    let candidates_first = bridge_ok(
        &provider
            .list_candidates(
                &admin,
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": subject,
                    "status": "pending",
                    "limit": 1,
                })),
            )
            .await,
    );
    let candidate_cursor = candidates_first["next_cursor"]
        .as_str()
        .expect("two pending rows have a second page")
        .to_string();

    // Adoption through the E4 dismissal, while this same provider stays open.
    let (core, principal) = open_core(&env).await;
    core.delete_pending_review(&principal, "pend_bridge".to_string())
        .await
        .expect("the projected row is adopted and consumed");

    let second = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_page_request(&subject, 1, Some(&cursor)),
            )
            .await,
    );
    let second_cursor = second["next_cursor"]
        .as_str()
        .expect("the third projected record follows")
        .to_string();
    let third = bridge_ok(
        &provider
            .list_records(
                &admin,
                bridge_records_page_request(&subject, 1, Some(&second_cursor)),
            )
            .await,
    );
    assert!(
        third["next_cursor"].is_null(),
        "the walk ends with the third record: {third}"
    );
    let mut walked = bridge_report_ids(&first, "records");
    walked.extend(bridge_report_ids(&second, "records"));
    walked.extend(bridge_report_ids(&third, "records"));
    walked.sort();
    let mut expected = vec![
        "lgc:memory_fragments:frag_bridge".to_string(),
        format!("lgc:memory_long_term:{CREATOR}:bridge-memory"),
        "lgc:memory_soul_narratives:%00".to_string(),
    ];
    expected.sort();
    assert_eq!(
        walked, expected,
        "the walk is exactly the seeded record set: no gap, no duplicate, no extra id"
    );

    // The candidate snapshot really changed: the adopted row left the queue.
    let stale = provider
        .list_candidates(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "subject_id": subject,
                "status": "pending",
                "limit": 1,
                "cursor": candidate_cursor,
            })),
        )
        .await;
    assert_eq!(bridge_reject_code(&stale), "CURSOR_INVALID");
}

/// The completion marker is re-read inside the dismissal's admission
/// transaction, not cached from the projection read that preceded it: when P5
/// publishes the marker while a dismissal is parked in its publication section,
/// the dismissal adopts nothing, consumes nothing, returns the retained
/// `NotFound` and leaves the legacy row untouched.
#[tokio::test]
async fn dismissal_racing_the_completion_marker_adopts_nothing() {
    let env = seed_env().await;
    seed_creator_pending_row(&env, "pend_race", None, "2026-01-01T00:00:01Z").await;
    // A provider-native candidate alongside the legacy row: the marker must
    // leave the provider's own state readable.
    let provider = open_bridge_provider(&env).await;
    let admin = bridge_principal();
    let subject = legacy::creator_subject_id(CREATOR);
    bridge_ok(
        &provider
            .ensure_subject(
                &admin,
                bridge_ensure_request(&subject, legacy::CREATOR_KIND_NS, CREATOR),
            )
            .await,
    );
    let native_candidate =
        native_pending_candidate(&provider, &admin, &subject, "k-race-native").await;
    let (core, principal) = open_core(&env).await;
    let pinned = core.memory_provider_for_test().expect("pinned provider");
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Barrier::new(2));
    pinned.park_next_publish(Arc::clone(&entered), Arc::clone(&release));

    let dismissal = tokio::spawn({
        let core = core.clone();
        let principal = principal.clone();
        async move {
            core.delete_pending_review(&principal, "pend_race".to_string())
                .await
        }
    });
    // The dismissal has resolved the projection and now holds its publication
    // section; P5 completes the space before the admission transaction opens.
    entered.notified().await;
    publish_migration_marker(&env, BRIDGE_SPACE).await;
    release.wait().await;

    let late = dismissal
        .await
        .unwrap()
        .expect_err("the marker withdrew the row before the dismissal consumed it");
    assert!(matches!(late, CoreError::NotFound { .. }), "got {late:?}");
    assert_eq!(
        adopted_candidate(&env, &legacy::pending_review_candidate_id("pend_race")).await,
        None,
        "the withdrawal is re-read inside the admission transaction"
    );
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1,
        "the legacy row is still there and was never written"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_candidates WHERE candidate_id = \
             'lgc:memory_pending_review:pend_race'"
        )
        .await,
        0,
        "no adoption was committed"
    );
    pool.close().await;
    assert_eq!(
        provider_candidate_status(&provider, &admin, &native_candidate).await,
        "pending",
        "the marker leaves the provider's own candidate alone"
    );
    let legacy_candidate = provider
        .get_candidate(
            &admin,
            dto(serde_json::json!({
                "space_id": BRIDGE_SPACE,
                "candidate_id": legacy::pending_review_candidate_id("pend_race"),
            })),
        )
        .await;
    assert_eq!(
        bridge_reject_code(&legacy_candidate),
        "NOT_FOUND",
        "the legacy candidate closed with the marker"
    );
    assert_eq!(
        core.count_pending_reviews(&principal).await.unwrap().count,
        0,
        "post-marker the legacy-derived queue is empty"
    );
}

// ═══ NXM-7 fix round 1 ═══════════════════════════════════════════════════
//
// P3-F1/P3-F2/P3-F4: the rerouted E6 projection is consumer-visible, so these
// tests assert the final mind-slot content against provider-seeded fixtures —
// the Character SOUL, its shared and binding-local fragments, and the promoted
// memories rendered as the legacy memory files they came from. The fixtures
// flood three recall pages with newer `event` rows, so the projection must
// filter per family while it pages rather than let one global cap serve every
// kind.

const MIND_SOUL: &str = "# Soul\n\nA provider-seeded Character soul.\n";

/// Seed the Character's legacy surfaces below 830 newer `event` records — more
/// than eight recall pages, the scale at which the round-2 review showed a
/// global page budget silently evicting memory — and return the exact bytes of
/// its two promoted memory files.
async fn seed_mind_projection_fixtures(env: &Env, other_binding: &str) -> (String, String) {
    let guarded = init_engine_pool(&env.db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .unwrap();
    let pool = guarded.clone_pool();
    let lifecycle_epoch: i64 =
        sqlx::query_scalar("SELECT lifecycle_epoch FROM characters WHERE character_id = ?")
            .bind(&env.character_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    for (fragment_id, summary, keywords, binding, created_at) in [
        (
            "mfrag_shared",
            "Shared Character fragment.",
            r#"["kw_shared"]"#,
            None,
            "2026-01-02T00:00:00Z",
        ),
        (
            "mfrag_binding",
            "Binding-local Character fragment.",
            r#"["kw_binding"]"#,
            Some(env.binding_id.as_str()),
            "2026-01-03T00:00:00Z",
        ),
        (
            "mfrag_other",
            "Other-binding Character fragment.",
            r#"["kw_other"]"#,
            Some(other_binding),
            "2026-01-04T00:00:00Z",
        ),
    ] {
        nexus_local_db::create_character_fragment(
            &pool,
            CREATOR,
            &nexus_local_db::NewCharacterMemoryFragment {
                fragment_id: fragment_id.to_string(),
                session_id: format!("sess_{fragment_id}"),
                character_id: env.character_id.clone(),
                actor_world_binding_id: binding.map(str::to_string),
                keywords: keywords.to_string(),
                summary: summary.to_string(),
                created_at: created_at.to_string(),
                ttl: None,
            },
        )
        .await
        .unwrap();
    }
    for index in 0..830_u32 {
        let operation_id = format!("op_mind_{index:03}");
        let session_id = format!("sess_mind_{index:03}");
        let pending_id = format!("pend_mind_{index:03}");
        let captured_at = format!("2026-04-01T{:02}:{:02}:00Z", index / 60, index % 60);
        nexus_local_db::capture_character_run(
            &pool,
            CREATOR,
            nexus_local_db::RunCaptureInput {
                operation_id: &operation_id,
                session_id: &session_id,
                character_id: &env.character_id,
                binding_id: &env.binding_id,
                pending_id: &pending_id,
                raw_digest: FRAGMENT_DIGEST,
                captured_at: &captured_at,
                lifecycle_epoch,
            },
        )
        .await
        .unwrap();
    }
    pool.close().await;
    drop(guarded);

    let soul_path =
        nexus_home_layout::character_soul_md_path(&env.user_home, CREATOR, &env.character_id);
    std::fs::create_dir_all(soul_path.parent().expect("character root")).unwrap();
    std::fs::write(&soul_path, MIND_SOUL).unwrap();
    let bearer = MemoryBearerRef::Character {
        owner_creator_id: CREATOR,
        character_id: &env.character_id,
    };
    for (slug, kind, body, updated_at) in [
        (
            "alpha-memory",
            "story_summary",
            "Alpha memory body.",
            "2026-01-05T00:00:00Z",
        ),
        (
            "zeta-memory",
            "character_note",
            "Zeta memory body.",
            "2026-01-06T00:00:00Z",
        ),
    ] {
        let mut memory = nexus_creator_memory::LongTermMemory::new(kind);
        memory.set_body(body);
        // Older than every seeded event row: a promoted memory must survive a
        // flood of newer unrelated records regardless of how large it is.
        memory.frontmatter.updated_at = updated_at.to_string();
        nexus_creator_memory::memory_io::save_memory(&env.user_home, bearer, slug, &memory)
            .expect("promoted memory fixture");
    }
    let read = |slug: &str| {
        std::fs::read_to_string(bearer.long_term_memory_path(&env.user_home, slug))
            .expect("memory file bytes")
    };
    (read("alpha-memory"), read("zeta-memory"))
}

#[tokio::test]
async fn character_mind_projection_serves_provider_seeded_soul_fragments_and_memories() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let other_binding = core
        .add_binding(
            &principal,
            env.character_id.clone(),
            WORLD_B.to_string(),
            None,
        )
        .await
        .expect("second binding")
        .binding
        .binding_id
        .as_str()
        .to_string();
    // Creator-side content of the same workspace: never part of the Character's
    // mind (a bearer sees its own subject only).
    seed_legacy_creator_content(&env).await;
    let (alpha, zeta) = seed_mind_projection_fixtures(&env, &other_binding).await;

    let mind = CoreCharacterMind::new(
        plain_pool(&env).await,
        core.memory_provider_for_test().expect("pinned provider"),
    )
    .projection(
        &principal,
        CREATOR,
        &env.character_id,
        Some(&env.binding_id),
    )
    .await
    .expect("the projection serves the seeded Character mind");

    assert_eq!(
        mind.soul.as_deref(),
        Some(MIND_SOUL),
        "the SOUL slot carries the Character's own document"
    );
    assert_eq!(
        mind.memory,
        vec![
            "- Binding-local Character fragment. — keywords: [\"kw_binding\"]".to_string(),
            "- Shared Character fragment. — keywords: [\"kw_shared\"]".to_string(),
            format!("- {alpha}"),
            format!("- {zeta}"),
        ],
        "830 newer unrelated events never evict the fragments or the promoted \
         memories, and each promoted memory renders as the legacy memory file \
         it came from, in slug order"
    );

    let shared_only = CoreCharacterMind::new(
        plain_pool(&env).await,
        core.memory_provider_for_test().expect("pinned provider"),
    )
    .projection(&principal, CREATOR, &env.character_id, None)
    .await
    .expect("shared-scope projection");
    assert_eq!(
        shared_only.memory,
        vec![
            "- Shared Character fragment. — keywords: [\"kw_shared\"]".to_string(),
            format!("- {alpha}"),
            format!("- {zeta}"),
        ],
        "a shared-scope projection keeps shared records and never leaks a binding's"
    );
    assert!(
        !shared_only
            .memory
            .iter()
            .any(|line| line.contains("Legacy long-term body.")),
        "the Creator's promoted memory is not the Character's"
    );
}

/// The legacy projection listed the complete memory directory by slug and then
/// took the head 20 (`memory_io::list_memories` + `take`). Ranking a recency
/// window instead drops the slug-first memory whenever 20 newer memories exist,
/// so this fixture makes recency and slug order disagree: `alpha-memory` is the
/// oldest file and exactly the memory a recency window would drop.
#[tokio::test]
async fn character_mind_projection_ranks_the_complete_memory_set_by_slug_before_capping() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let bearer = MemoryBearerRef::Character {
        owner_creator_id: CREATOR,
        character_id: &env.character_id,
    };
    let mut slugs = vec!["alpha-memory".to_string()];
    for index in 0..20_u32 {
        slugs.push(format!("memory-{index:02}"));
    }
    let mut expected: Vec<String> = Vec::new();
    for (index, slug) in slugs.iter().enumerate() {
        let mut memory = nexus_creator_memory::LongTermMemory::new("story_summary");
        memory.set_body(&format!("Memory {slug} body."));
        // `alpha-memory` is older than every other file, so only the complete
        // slug ranking keeps it inside the 20-entry selection.
        memory.frontmatter.updated_at = if index == 0 {
            "2025-12-01T00:00:00Z".to_string()
        } else {
            format!("2026-01-{index:02}T00:00:00Z")
        };
        nexus_creator_memory::memory_io::save_memory(&env.user_home, bearer, slug, &memory)
            .expect("promoted memory fixture");
        if index < 20 {
            let bytes = std::fs::read_to_string(bearer.long_term_memory_path(&env.user_home, slug))
                .expect("memory file bytes");
            expected.push(format!("- {bytes}"));
        }
    }

    let mind = CoreCharacterMind::new(
        plain_pool(&env).await,
        core.memory_provider_for_test().expect("pinned provider"),
    )
    .projection(&principal, CREATOR, &env.character_id, None)
    .await
    .expect("the projection serves the promoted set");

    assert_eq!(
        mind.memory, expected,
        "the slug-first 20 of the complete promoted set survive; provider recency \
         never evicts the legacy selection"
    );
    assert!(
        !mind
            .memory
            .iter()
            .any(|line| line.contains("memory-19 body.")),
        "the slug-last memory falls outside the 20-entry selection"
    );
}

/// The legacy per-scope fragment read took one instant's rows by `fragment_id
/// DESC` (`list_character_fragments`, `LIMIT 100`), while the provider pages an
/// equal `created_at` by record id ascending. A read that stops at the 100th row
/// therefore keeps the wrong rows of a tie group, and the fragment the legacy
/// selection put first disappears. 101 shared fragments at one instant pin that
/// selection: the projection's first line is `frag-100` (the member a
/// provider-ordered prefix drops) and the row that prefix kept instead
/// (`frag-080`) never renders.
#[tokio::test]
async fn character_mind_projection_keeps_the_legacy_tie_break_at_the_scope_cap() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    for index in 0..101_u32 {
        nexus_local_db::create_character_fragment(
            &pool,
            CREATOR,
            &nexus_local_db::NewCharacterMemoryFragment {
                fragment_id: format!("frag-{index:03}"),
                session_id: format!("sess_frag_{index:03}"),
                character_id: env.character_id.clone(),
                actor_world_binding_id: None,
                keywords: "[]".to_string(),
                summary: format!("Fragment frag-{index:03}."),
                created_at: "2026-02-01T00:00:00Z".to_string(),
                ttl: None,
            },
        )
        .await
        .unwrap();
    }
    pool.close().await;

    let mind = CoreCharacterMind::new(
        plain_pool(&env).await,
        core.memory_provider_for_test().expect("pinned provider"),
    )
    .projection(&principal, CREATOR, &env.character_id, None)
    .await
    .expect("the projection serves the tie group");

    let expected: Vec<String> = (81..=100)
        .rev()
        .map(|index| format!("- Fragment frag-{index:03}."))
        .collect();
    assert_eq!(
        mind.memory, expected,
        "the legacy `created_at DESC, fragment_id DESC` order picks the rows one \
         instant keeps at the per-scope bound, before the mind's own 20-line cap"
    );
    assert!(
        !mind.memory.iter().any(|line| line.contains("frag-080")),
        "the row a provider-ordered prefix kept instead of frag-100 never renders"
    );
}

/// The legacy per-scope read ordered the RAW `created_at` column text
/// (`ORDER BY created_at DESC, fragment_id DESC`), while the provider pages by
/// parsed instant. With 101 fragments at `2026-02-01T00:00:00Z` and one at
/// `2026-02-01T00:00:00.900Z`, SQLite ranks every `Z` row above the fractional
/// one (`.` < `Z`), so the legacy `LIMIT 100` excludes the fractional-second
/// fragment. Its instant is the newest of the fixture, so an instant-ordered
/// selection would put it first in the mind's 20 lines and drop a
/// legacy-selected fragment (fix round 4, finding R4-F1).
#[tokio::test]
async fn character_mind_projection_keeps_the_legacy_text_order_across_timestamp_formats() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    for index in 0..101_u32 {
        nexus_local_db::create_character_fragment(
            &pool,
            CREATOR,
            &nexus_local_db::NewCharacterMemoryFragment {
                fragment_id: format!("frag-{index:03}"),
                session_id: format!("sess_frag_{index:03}"),
                character_id: env.character_id.clone(),
                actor_world_binding_id: None,
                keywords: "[]".to_string(),
                summary: format!("Fragment frag-{index:03}."),
                created_at: "2026-02-01T00:00:00Z".to_string(),
                ttl: None,
            },
        )
        .await
        .unwrap();
    }
    // The newest instant, but the lowest text key of the fixture: the legacy
    // selection never reaches it.
    nexus_local_db::create_character_fragment(
        &pool,
        CREATOR,
        &nexus_local_db::NewCharacterMemoryFragment {
            fragment_id: "frag-900".to_string(),
            session_id: "sess_frag_900".to_string(),
            character_id: env.character_id.clone(),
            actor_world_binding_id: None,
            keywords: "[]".to_string(),
            summary: "Fractional-second fragment.".to_string(),
            created_at: "2026-02-01T00:00:00.900Z".to_string(),
            ttl: None,
        },
    )
    .await
    .unwrap();
    pool.close().await;

    let mind = CoreCharacterMind::new(
        plain_pool(&env).await,
        core.memory_provider_for_test().expect("pinned provider"),
    )
    .projection(&principal, CREATOR, &env.character_id, None)
    .await
    .expect("the projection serves the mixed-format family");

    let expected: Vec<String> = (81..=100)
        .rev()
        .map(|index| format!("- Fragment frag-{index:03}."))
        .collect();
    assert_eq!(
        mind.memory, expected,
        "the legacy `created_at DESC, fragment_id DESC` TEXT order picks the \
         whole-second `Z` rows, so the fractional-second row changes neither \
         the per-scope 100 selection nor the mind's head 20"
    );
    assert!(
        !mind
            .memory
            .iter()
            .any(|line| line.contains("Fractional-second fragment.")),
        "the fractional-second row is the newest instant but the lowest legacy \
         text key, so the legacy `LIMIT 100` excludes it"
    );
}

/// §7.7: an archived subject's records leave recall. The E6 projection replaces
/// recall on this read path, and the listing read behind it is the audit route
/// that keeps serving them — so a read-only core (whose `MemoryAdapter::open`
/// skips `ensure_subject`) must project no record of an archived subject.
/// The SOUL slot is a profile, not a record, and is not seeded here.
#[tokio::test]
async fn character_mind_projection_excludes_an_archived_subject_like_recall() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let provider = core.memory_provider_for_test().expect("pinned provider");

    let pool = plain_pool(&env).await;
    nexus_local_db::create_character_fragment(
        &pool,
        CREATOR,
        &nexus_local_db::NewCharacterMemoryFragment {
            fragment_id: "afrag_shared".to_string(),
            session_id: "sess_afrag_shared".to_string(),
            character_id: env.character_id.clone(),
            actor_world_binding_id: None,
            keywords: "[]".to_string(),
            summary: "Archived-scope shared fragment.".to_string(),
            created_at: "2026-02-01T00:00:00Z".to_string(),
            ttl: None,
        },
    )
    .await
    .unwrap();
    pool.close().await;
    let bearer = MemoryBearerRef::Character {
        owner_creator_id: CREATOR,
        character_id: &env.character_id,
    };
    let mut memory = nexus_creator_memory::LongTermMemory::new("story_summary");
    memory.set_body("Archived-scope memory body.");
    memory.frontmatter.updated_at = "2026-01-05T00:00:00Z".to_string();
    nexus_creator_memory::memory_io::save_memory(
        &env.user_home,
        bearer,
        "archived-memory",
        &memory,
    )
    .expect("promoted memory fixture");
    let document =
        std::fs::read_to_string(bearer.long_term_memory_path(&env.user_home, "archived-memory"))
            .expect("memory file bytes");

    // A live subject: the projection serves both record families.
    let live = CoreCharacterMind::new(plain_pool(&env).await, provider.clone())
        .projection(&principal, CREATOR, &env.character_id, None)
        .await
        .expect("live projection");
    assert_eq!(
        live.memory,
        vec![
            "- Archived-scope shared fragment.".to_string(),
            format!("- {document}"),
        ],
        "a live subject projects its fragments and promoted memories"
    );

    // Archive the subject the projection addresses, through the provider's own
    // terminal transition (§7.7), so the read sees a real archived subject.
    let subject = legacy::character_subject_id(&env.character_id);
    let archived = bridge_ok(
        &provider
            .fence_subject(
                &bridge_principal(),
                dto(serde_json::json!({
                    "space_id": BRIDGE_SPACE,
                    "subject_id": subject,
                    "target": { "type": "subject" },
                    "action": "archive",
                    "expected_revision": 1,
                    "idempotency_key": "k-mind-archive",
                })),
            )
            .await,
    );
    assert_eq!(archived["result"]["status"], "archived");

    // A read-only core writes nothing, so the archived state is the one the
    // provider already holds — and the projection is recall-equivalent.
    let read_only = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .expect("read-only open");
    let principal = read_only.active_principal().await.unwrap();
    let excluded = CoreCharacterMind::new(
        plain_pool(&env).await,
        read_only
            .memory_provider_for_test()
            .expect("pinned provider"),
    )
    .projection(&principal, CREATOR, &env.character_id, None)
    .await
    .expect("an archived subject still projects");
    assert!(
        excluded.memory.is_empty(),
        "recall excludes an archived subject's records, and so does the E6 \
         projection: {:?}",
        excluded.memory
    );
}

#[tokio::test]
async fn character_mind_projection_fails_closed_when_the_provider_refuses() {
    let env = seed_env().await;
    // A read-only open with no writer behind it: no space or subject exists, so
    // the projection's first read is refused instead of yielding an empty mind.
    let read_only = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .expect("read-only open");
    let principal = read_only.active_principal().await.unwrap();
    let provider = read_only
        .memory_provider_for_test()
        .expect("pinned provider");
    let pool = plain_pool(&env).await;

    let refused = CoreCharacterMind::new(pool.clone(), provider.clone())
        .projection(&principal, CREATOR, &env.character_id, None)
        .await
        .expect_err("an absent provider space cannot serve the projection");
    assert!(
        matches!(refused, CoreError::NotFound { .. }),
        "the refusal propagates instead of becoming an empty mind: {refused:?}"
    );

    let malformed = CoreCharacterMind::new(pool, provider)
        .projection(&principal, CREATOR, "not-a-character", None)
        .await
        .expect_err("a malformed bearer id names no subject");
    assert!(
        matches!(malformed, CoreError::InvalidInput { ref field, .. } if field == "character_id"),
        "got {malformed:?}"
    );
}
