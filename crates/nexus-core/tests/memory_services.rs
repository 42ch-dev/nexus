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
use nexus_core::{AdmittedActor, CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::soul_narrative::{
    SoulNarrativeDraft, SoulNarrativeSynthesisInput, SoulNarrativeSynthesizer,
};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{
    create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams,
};
use sqlx::SqlitePool;
use std::path::PathBuf;
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

    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        0
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM character_memory_pending_review"
        )
        .await,
        0
    );
    assert_eq!(
        count_where(
            &pool,
            &format!("SELECT COUNT(*) FROM memory_fragments WHERE creator_id = '{CREATOR}'")
        )
        .await,
        1
    );
    assert_eq!(
        count_where(
            &pool,
            &format!("SELECT COUNT(*) FROM memory_fragments WHERE creator_id = '{OTHER}'")
        )
        .await,
        0,
        "no fragment may cross to another creator"
    );
    assert_eq!(
        count_where(
            &pool,
            &format!(
                "SELECT COUNT(*) FROM character_memory_fragments WHERE character_id = '{chr}'"
            )
        )
        .await,
        1
    );
    pool.close().await;

    // The pipeline composes bearer paths off the nexus root (the same
    // `state.nexus_home()` the daemon passed), not the raw user home.
    let nexus_root = nexus_home_layout::nexus_root_from_home(&env.user_home);
    let cdir = MemoryBearerRef::Creator(CREATOR).long_term_memory_dir(&nexus_root);
    let hdir = MemoryBearerRef::Character {
        owner_creator_id: CREATOR,
        character_id: &chr,
    }
    .long_term_memory_dir(&nexus_root);
    assert_ne!(cdir, hdir);
    assert_eq!(
        std::fs::read_dir(&cdir).unwrap().count(),
        1,
        "creator memory dir"
    );
    assert_eq!(
        std::fs::read_dir(&hdir).unwrap().count(),
        1,
        "character memory dir"
    );
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

/// Pre-insert the fragment id a later `FragmentOnly` action mints for
/// `pending_id` (`frag_{pending_id}`), so that action collides on the primary
/// key and fails, leaving the pending row in place.
async fn block_fragment_creation(pool: &SqlitePool, pending_id: &str) {
    nexus_local_db::memory_fragment::create_fragment(
        pool,
        &nexus_local_db::memory_fragment::MemoryFragmentRecord {
            fragment_id: format!("frag_{pending_id}"),
            session_id: "sess_block".to_string(),
            creator_id: CREATOR.to_string(),
            keywords: "[]".to_string(),
            summary: "blocker".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            ttl: None,
            world_id: None,
        },
    )
    .await
    .unwrap();
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

    // One bounded call: at most the batch limit, and more rows signalled.
    let first = core
        .review_memory(&principal, creator_review_request())
        .await
        .unwrap();
    assert_eq!(
        first.processed,
        Some(50),
        "one call processes the batch bound"
    );
    assert_eq!(
        first.has_more,
        Some(true),
        "remaining rows must be signalled"
    );
    assert_eq!(first.fragmented, 50);
    assert_eq!(first.promoted, 0);
    assert_eq!(first.dropped, 0);

    // The remainder drains and the drain reports completion.
    let second = core
        .review_memory(&principal, creator_review_request())
        .await
        .unwrap();
    assert_eq!(second.processed, Some(5), "the second call drains the tail");
    assert_eq!(second.has_more, Some(false));
    assert_eq!(second.fragmented, 5);

    // A call on the empty queue reports zero work and completion.
    let third = core
        .review_memory(&principal, creator_review_request())
        .await
        .unwrap();
    assert_eq!(third.processed, Some(0));
    assert_eq!(third.has_more, Some(false));
    assert_eq!(third.promoted + third.fragmented + third.dropped, 0);

    // No row was lost and none was processed twice: the queue is empty and
    // exactly one fragment exists per seeded row.
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        0,
        "the queue is fully drained"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_fragments").await,
        i64::try_from(DRAIN_TOTAL).unwrap(),
        "one fragment per seeded row, no duplicate insert"
    );
    pool.close().await;
}

/// Ported from the daemon W-QC3-001 drain-completion regressions
/// (`memory_review_fragments_api.rs`): `has_more` reflects whether pending rows
/// REMAIN, not whether rows were attempted. A row whose action fails is not
/// deleted, so `has_more` stays true (never a false "Review complete") and the
/// fragment insert that could not consume exactly one pending row is rolled
/// back rather than duplicated.
#[tokio::test]
async fn retained_creator_review_row_failure_keeps_has_more_true() {
    // Only pending row fails: the serialized call re-inspects it, reports
    // has_more, counts nothing, and never drops it.
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    seed_creator_pending(&env, "pending_blocked", 1).await;
    let pool = plain_pool(&env).await;
    block_fragment_creation(&pool, "pending_blocked_000").await;
    pool.close().await;

    for call in 1..=3 {
        let out = core
            .review_memory(&principal, creator_review_request())
            .await
            .unwrap();
        assert_eq!(
            out.processed,
            Some(1),
            "call {call}: the row is re-inspected"
        );
        assert_eq!(
            out.has_more,
            Some(true),
            "call {call}: has_more must stay true while a row remains"
        );
        assert_eq!(out.promoted, 0, "call {call}");
        assert_eq!(out.fragmented, 0, "call {call}: nothing completed");
        assert_eq!(out.dropped, 0, "call {call}");
        let pool = plain_pool(&env).await;
        assert_eq!(
            count_where(
                &pool,
                "SELECT COUNT(*) FROM memory_pending_review WHERE pending_id = 'pending_blocked_000'"
            )
            .await,
            1,
            "call {call}: the failed row must never be dropped"
        );
        assert_eq!(
            count_where(&pool, "SELECT COUNT(*) FROM memory_fragments").await,
            1,
            "call {call}: the blocked fragment is never duplicated"
        );
        pool.close().await;
    }

    // Final-row failure inside a batch: every other row completes, the blocked
    // row (oldest, therefore processed last) stays pending and has_more is true.
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    seed_creator_pending(&env, "pending_blocked", 4).await;
    let pool = plain_pool(&env).await;
    block_fragment_creation(&pool, "pending_blocked_000").await;
    pool.close().await;

    let out = core
        .review_memory(&principal, creator_review_request())
        .await
        .unwrap();
    assert_eq!(out.processed, Some(4), "all four rows were inspected");
    assert_eq!(out.fragmented, 3, "only the blocked row failed");
    assert_eq!(
        out.has_more,
        Some(true),
        "has_more must be true when the final row in the batch remains pending"
    );
    let pool = plain_pool(&env).await;
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1,
        "only the failed row remains pending"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM memory_pending_review WHERE pending_id = 'pending_blocked_000'"
        )
        .await,
        1
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
    assert_eq!(
        first.processed,
        Some(50),
        "one call processes the batch bound"
    );
    assert_eq!(
        first.has_more,
        Some(true),
        "remaining rows must be signalled"
    );
    assert_eq!(first.dropped, 50);

    let second = core
        .review_character_memory(
            &principal,
            chr.clone(),
            dto::<ReviewCharacterMemoryRequest>(serde_json::json!({})),
        )
        .await
        .unwrap();
    assert_eq!(second.processed, Some(3), "the second call drains the tail");
    assert_eq!(second.has_more, Some(false));

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
        0,
        "the queue is fully drained"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM character_memory_fragments").await,
        0,
        "dropped digests create no fragments"
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
    dto(serde_json::json!({
        "space_id": BRIDGE_SPACE,
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
    dto(serde_json::json!({
        "space_id": BRIDGE_SPACE,
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
