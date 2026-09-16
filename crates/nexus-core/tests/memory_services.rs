//! P2-T2 bearer memory services contract tests (core service semantics):
//! revision-checked fragment promotion is atomic and cache-scoped (ported
//! from the daemon `character_memory_api.rs` promotion anchor), both memory
//! arms share the classification while their storage stays bearer-isolated
//! (ported from the daemon dual-arm semantic suite), and the SOUL reflect
//! state machine follows the fragment gate and the per-bearer cache without
//! ever synthesizing in the background.

use nexus_contracts::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest;
use nexus_contracts::daemon_api::memory::review_request::ReviewRequest;
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
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
