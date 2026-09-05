//! Dual-bearer memory-pipeline semantic suite (v1.184 P3 Task 2 fix round 1).
//!
//! Lives outside the `memory_pipeline` module so it cannot fabricate a
//! [`BearerPipelineCtx`] (fields are private). Every Character context is
//! obtained through the validated `BearerPipelineCtx::character` constructor,
//! which verifies format, ownership, and the active lifecycle before any DB
//! read, file write, or synthesis.

#![allow(clippy::unwrap_used)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::memory_pipeline::{
    process_bearer_review_batch, reflect_bearer_soul, BearerPipelineCtx, ReflectState,
};
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::long_term_memory::LongTermMemory;
use nexus_creator_memory::review::PendingReviewInput;
use nexus_creator_memory::soul_narrative::{
    SoulNarrativeDraft, SoulNarrativeSynthesisInput, SoulNarrativeSynthesizer,
};
use nexus_local_db::{create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams};
use std::path::PathBuf;

const OWNER_A: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OWNER_B: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD_A: &str = "wld_worldA";

const PROMOTE_DIGEST: &str =
    "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";
const FRAGMENT_DIGEST: &str =
    "Research summary long enough to classify as a fragment rather than being dropped for shortness.";
const DROP_DIGEST: &str = "Too short.";

struct Sync {
    tmp: crate::test_utils::TestTempRoot,
    nexus_home: PathBuf,
    pool: sqlx::SqlitePool,
    chr_a: String,
}

async fn setup() -> Sync {
    let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
    let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
    ensure_creator_row(&pool, OWNER_A, "Owner A").await.unwrap();
    ensure_creator_row(&pool, OWNER_B, "Owner B").await.unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(WORLD_A)
    .bind(OWNER_A)
    .bind(WORLD_A)
    .bind(WORLD_A)
    .execute(&pool)
    .await
    .unwrap();
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER_A,
            display_name: "Ava",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    Sync {
        tmp,
        nexus_home,
        pool,
        chr_a: created.character.character_id.clone(),
    }
}

fn ctxc() -> BearerPipelineCtx<'static> {
    BearerPipelineCtx::creator(OWNER_A, None)
}

async fn ctxh<'a>(pool: &'a sqlx::SqlitePool, chr: &'a str) -> BearerPipelineCtx<'a> {
    BearerPipelineCtx::character(pool, OWNER_A, chr, None)
        .await
        .expect("active owned Character must authorize")
}

fn pcr(id: &str, sess: &str, digest: &str, kind: &str) -> PendingReviewInput {
    PendingReviewInput {
        pending_id: id.to_string(),
        session_id: sess.to_string(),
        bearer_id: OWNER_A.to_string(),
        scope_id: None,
        task_kind: kind.to_string(),
        raw_digest: digest.to_string(),
        created_at: "2026-01-01T00:00:01Z".to_string(),
    }
}

fn pch(id: &str, sess: &str, digest: &str, kind: &str, chr: &str) -> PendingReviewInput {
    PendingReviewInput {
        pending_id: id.to_string(),
        session_id: sess.to_string(),
        bearer_id: chr.to_string(),
        scope_id: None,
        task_kind: kind.to_string(),
        raw_digest: digest.to_string(),
        created_at: "2026-01-01T00:00:01Z".to_string(),
    }
}

async fn count(pool: &sqlx::SqlitePool, sql: &str, bind: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(sql).bind(bind).fetch_one(pool).await.unwrap();
    row.0
}

async fn count_all(pool: &sqlx::SqlitePool, sql: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(sql).fetch_one(pool).await.unwrap();
    row.0
}

struct NoSynth;
impl SoulNarrativeSynthesizer for NoSynth {
    async fn synthesize(
        &self,
        _: MemoryBearerRef<'_>,
        _: SoulNarrativeSynthesisInput,
    ) -> Result<SoulNarrativeDraft, MemoryError> {
        Err(MemoryError::WorkerUnavailable)
    }
}

#[tokio::test]
async fn review_both_arms_share_classification_and_isolate_storage() {
    let s = setup().await;
    let home = s.nexus_home.clone();
    let pool = s.pool.clone();
    let chr = s.chr_a.clone();

    let horizon = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let creator_out = process_bearer_review_batch(
        &[
            pcr("c_p", "s_p", PROMOTE_DIGEST, "brainstorm"),
            pcr("c_f", "s_f", FRAGMENT_DIGEST, "research"),
            pcr("c_d", "s_d", DROP_DIGEST, "unknown"),
        ],
        &home,
        &ctxc(),
        &pool,
        horizon,
    )
    .await;
    assert_eq!(creator_out.promoted, 1);
    assert_eq!(creator_out.fragmented, 1);
    assert_eq!(creator_out.dropped, 1);

    let char_out = process_bearer_review_batch(
        &[
            pch("c_p", "s_p", PROMOTE_DIGEST, "brainstorm", &chr),
            pch("c_f", "s_f", FRAGMENT_DIGEST, "research", &chr),
            pch("c_d", "s_d", DROP_DIGEST, "unknown", &chr),
        ],
        &home,
        &ctxh(&pool, &chr).await,
        &pool,
        horizon,
    )
    .await;
    assert_eq!(char_out.promoted, 1);
    assert_eq!(char_out.fragmented, 1);
    assert_eq!(char_out.dropped, 1);

    assert_eq!(count_all(&pool, "SELECT COUNT(*) FROM memory_pending_review").await, 0);
    assert_eq!(
        count_all(&pool, "SELECT COUNT(*) FROM character_memory_pending_review").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_fragments WHERE creator_id = ?", OWNER_A)
            .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM character_memory_fragments WHERE character_id = ?", &chr)
            .await,
        1
    );
    assert_eq!(
        count_all(
            &pool,
            "SELECT COUNT(*) FROM memory_fragments WHERE creator_id = 'ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'"
        )
        .await,
        0
    );

    let cdir = MemoryBearerRef::Creator(OWNER_A).long_term_memory_dir(&home);
    let hdir = MemoryBearerRef::Character {
        owner_creator_id: OWNER_A,
        character_id: &chr,
    }
    .long_term_memory_dir(&home);
    assert_ne!(cdir, hdir);
    assert_eq!(std::fs::read_dir(&cdir).unwrap().count(), 1, "creator memory dir");
    assert_eq!(std::fs::read_dir(&hdir).unwrap().count(), 1, "character memory dir");

    drop(s.tmp);
}

#[tokio::test]
async fn promotion_is_idempotent_for_both_arms() {
    let s = setup().await;
    let home = s.nexus_home.clone();
    let chr = s.chr_a.clone();

    use nexus_creator_memory::review::SessionDigestSummarizer;
    struct Fix;
    impl SessionDigestSummarizer for Fix {
        async fn summarize(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: Option<&str>,
        ) -> Result<String, MemoryError> {
            Ok("fixed body.".to_string())
        }
    }
    let fix = Fix;

    let ci = pcr("p1", "sess_x", PROMOTE_DIGEST, "brainstorm");
    let hi = pch("p2", "sess_x", PROMOTE_DIGEST, "brainstorm", &chr);
    let cb = MemoryBearerRef::Creator(OWNER_A);
    let hb = MemoryBearerRef::Character {
        owner_creator_id: OWNER_A,
        character_id: &chr,
    };

    nexus_creator_memory::review::promote_to_long_term(&home, cb, &ci, &fix)
        .await
        .unwrap();
    let dup = nexus_creator_memory::review::promote_to_long_term(&home, cb, &ci, &fix).await;
    assert!(dup.is_err());
    assert!(dup.unwrap_err().to_string().contains("already promoted"));

    nexus_creator_memory::review::promote_to_long_term(&home, hb, &hi, &fix)
        .await
        .unwrap();
    let dup = nexus_creator_memory::review::promote_to_long_term(&home, hb, &hi, &fix).await;
    assert!(dup.is_err());
    assert!(dup.unwrap_err().to_string().contains("already promoted"));

    drop(s.tmp);
}

#[tokio::test]
async fn aggregation_updates_soul_in_the_right_root() {
    let s = setup().await;
    let home = s.nexus_home.clone();
    let chr = s.chr_a.clone();

    let cb = MemoryBearerRef::Creator(OWNER_A);
    let hb = MemoryBearerRef::Character {
        owner_creator_id: OWNER_A,
        character_id: &chr,
    };
    nexus_creator_memory::soul_io::create(&home, cb).unwrap();
    let mut cmem = LongTermMemory::new("story_summary");
    cmem.set_body("A grand adventure story.");
    nexus_creator_memory::memory_io::save_memory(&home, cb, "adventure", &cmem).unwrap();

    nexus_creator_memory::soul_io::create(&home, hb).unwrap();
    let mut chmem = LongTermMemory::new("story_summary");
    chmem.set_body("A grand adventure story.");
    nexus_creator_memory::memory_io::save_memory(&home, hb, "adventure", &chmem).unwrap();

    let cres = nexus_creator_memory::experience_aggregation::aggregate_experience(&home, cb, None)
        .await
        .unwrap();
    let hres = nexus_creator_memory::experience_aggregation::aggregate_experience(&home, hb, None)
        .await
        .unwrap();
    assert_eq!(cres.experience_markdown, hres.experience_markdown);
    assert_eq!(cres.memories_processed, 1);
    assert_eq!(hres.memories_processed, 1);

    let c_soul = std::fs::read_to_string(cb.soul_path(&home)).unwrap();
    let h_soul = std::fs::read_to_string(hb.soul_path(&home)).unwrap();
    assert!(c_soul.contains("### Story Summary"));
    assert!(h_soul.contains("### Story Summary"));

    drop(s.tmp);
}

#[tokio::test]
async fn reflect_both_arms_report_insufficient_data_and_ungenerated() {
    let s = setup().await;
    let pool = s.pool.clone();
    let chr = s.chr_a.clone();

    let c_ctx = ctxc();
    let h_ctx = ctxh(&pool, &chr).await;
    let no_synth: Option<&NoSynth> = None;

    assert_eq!(
        reflect_bearer_soul(&pool, &c_ctx, false, no_synth).await.unwrap().state,
        ReflectState::InsufficientData
    );
    assert_eq!(
        reflect_bearer_soul(&pool, &h_ctx, false, no_synth).await.unwrap().state,
        ReflectState::InsufficientData
    );

    for i in 0..25 {
        let kw = format!("uniq_{i}");
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO memory_fragments \
             (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl, world_id) \
             VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(format!("cf_{i:04}"))
        .bind(format!("scf_{i:04}"))
        .bind(OWNER_A)
        .bind(format!(r#"["{kw}"]"#))
        .bind(format!("summary {i}"))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO character_memory_fragments \
             (fragment_id, session_id, character_id, actor_world_binding_id, keywords, summary, created_at, ttl, revision) \
             VALUES (?, ?, ?, NULL, ?, ?, ?, NULL, 0)",
        )
        .bind(format!("chf_{i:04}"))
        .bind(format!("schf_{i:04}"))
        .bind(&chr)
        .bind(format!(r#"["{kw}"]"#))
        .bind(format!("summary {i}"))
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
    }

    let o = reflect_bearer_soul(&pool, &c_ctx, false, no_synth).await.unwrap();
    assert_eq!(o.state, ReflectState::Ungenerated);
    assert_eq!(o.current_fragment_count, 25);
    let o = reflect_bearer_soul(&pool, &h_ctx, false, no_synth).await.unwrap();
    assert_eq!(o.state, ReflectState::Ungenerated);
    assert_eq!(o.current_fragment_count, 25);

    struct Mock;
    impl SoulNarrativeSynthesizer for Mock {
        async fn synthesize(
            &self,
            _: MemoryBearerRef<'_>,
            input: SoulNarrativeSynthesisInput,
        ) -> Result<SoulNarrativeDraft, MemoryError> {
            let kw = input
                .top_keywords
                .first()
                .map(|(k, _)| k.clone())
                .unwrap_or_default();
            Ok(SoulNarrativeDraft {
                narrative: format!(
                    "A reflective narrative about {kw} and magic, looking ahead."
                ),
            })
        }
    }
    let mock = Mock;

    let o = reflect_bearer_soul(&pool, &c_ctx, true, Some(&mock)).await.unwrap();
    assert_eq!(o.state, ReflectState::Current);
    assert_eq!(
        count_all(&pool, "SELECT COUNT(*) FROM memory_soul_narratives").await,
        1
    );

    let o = reflect_bearer_soul(&pool, &h_ctx, true, Some(&mock)).await.unwrap();
    assert_eq!(o.state, ReflectState::Current);
    assert_eq!(
        count_all(&pool, "SELECT COUNT(*) FROM character_soul_narratives").await,
        1
    );

    assert_eq!(
        count_all(&pool, "SELECT COUNT(*) FROM memory_soul_narratives").await,
        1
    );

    drop(s.tmp);
}

#[tokio::test]
async fn character_provenance_rejects_foreign_owner_before_side_effects() {
    let s = setup().await;
    let charted = s.chr_a.clone();

    let res = BearerPipelineCtx::character(&s.pool, OWNER_B, &charted, None).await;
    assert!(res.is_err());
    assert!(matches!(res, Err(NexusApiError::Forbidden { .. })));

    let res = BearerPipelineCtx::character(
        &s.pool,
        OWNER_A,
        "chr_ffffffffffffffffffffffffffffffff",
        None,
    )
    .await;
    assert!(res.is_err());

    drop(s.tmp);
}

/// An owned-but-archived Character must be rejected by the context
/// constructor with no DB/file/synthesis side effects.
#[tokio::test]
async fn character_provenance_rejects_inactive_character_before_side_effects() {
    let s = setup().await;
    let charted = s.chr_a.clone();

    // Archive the Character (status is mutable lifecycle state, distinct from
    // ownership).
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(&charted)
        .execute(&s.pool)
        .await
        .unwrap();

    let res = BearerPipelineCtx::character(&s.pool, OWNER_A, &charted, None).await;
    assert!(res.is_err());
    assert!(matches!(res, Err(NexusApiError::Forbidden { .. })));

    // No side effect happened for the archived Character: the pipeline never
    // got a context, so no SOUL/LTM file and no DB row is created.
    let hdir = MemoryBearerRef::Character {
        owner_creator_id: OWNER_A,
        character_id: &charted,
    }
    .long_term_memory_dir(&s.nexus_home);
    assert!(
        !hdir.exists(),
        "no file side effect for an archived Character context"
    );
    let hsoul = MemoryBearerRef::Character {
        owner_creator_id: OWNER_A,
        character_id: &charted,
    }
    .soul_path(&s.nexus_home);
    assert!(!hsoul.exists(), "no SOUL.md for an archived Character");

    // No DB rows for the archived Character in any Character memory table.
    let schema_checks = [
        ("SELECT COUNT(*) FROM character_memory_pending_review WHERE character_id = ?", "pending"),
        ("SELECT COUNT(*) FROM character_memory_fragments WHERE character_id = ?", "fragments"),
        ("SELECT COUNT(*) FROM character_soul_narratives WHERE character_id = ?", "narratives"),
        ("SELECT COUNT(*) FROM character_soul_meta WHERE character_id = ?", "soul_meta"),
    ];
    for (sql, label) in schema_checks {
        let n = count(&s.pool, sql, &charted).await;
        assert_eq!(n, 0, "no {label} row for an archived Character");
    }

    drop(s.tmp);
}

#[tokio::test]
async fn character_mind_projection_is_bounded_scoped_and_honest_empty() {
    use crate::api::handlers::memory_pipeline::load_character_mind_projection;
    use nexus_creator_memory::soul_io;

    let s = setup().await;
    // The character was created with one initial binding to WORLD_A.
    let binding_a1 = nexus_local_db::list_bindings_for_character(
        &s.pool,
        OWNER_A,
        &s.chr_a,
        10,
        0,
    )
    .await
    .unwrap()
    .into_iter()
    .next()
    .unwrap()
    .binding_id;

    // Honest empty: no SOUL.md and no fragments → both slots empty.
    let mind = load_character_mind_projection(
        &s.pool, &s.nexus_home, OWNER_A, &s.chr_a, Some(&binding_a1),
    )
    .await
    .expect("honest-empty projection ok");
    assert_eq!(mind.memory.len(), 0, "no fragments yet");
    assert!(mind.soul.is_none(), "no SOUL.md yet");

    // A shared fragment and a binding-local fragment.
    let shared = nexus_local_db::NewCharacterMemoryFragment {
        fragment_id: "frag_shared_p".to_string(),
        session_id: "sess_shared_p".to_string(),
        character_id: s.chr_a.clone(),
        actor_world_binding_id: None,
        keywords: r#"["harbor"]"#.to_string(),
        summary: "SHAREDPROJ the harbor accord holds.".to_string(),
        created_at: "2026-01-01T00:00:01Z".to_string(),
        ttl: None,
    };
    nexus_local_db::create_character_fragment(&s.pool, OWNER_A, &shared)
        .await
        .unwrap();
    let local = nexus_local_db::NewCharacterMemoryFragment {
        fragment_id: "frag_local_a".to_string(),
        session_id: "sess_local_a".to_string(),
        character_id: s.chr_a.clone(),
        actor_world_binding_id: Some(binding_a1.clone()),
        keywords: r#"["lantern"]"#.to_string(),
        summary: "LOCALPROJ only this binding saw the lantern.".to_string(),
        created_at: "2026-01-01T00:00:02Z".to_string(),
        ttl: None,
    };
    nexus_local_db::create_character_fragment(&s.pool, OWNER_A, &local)
        .await
        .unwrap();

    // Projection for that binding: shared + binding-local, newest first.
    let mind = load_character_mind_projection(
        &s.pool, &s.nexus_home, OWNER_A, &s.chr_a, Some(&binding_a1),
    )
    .await
    .expect("projection ok");
    assert_eq!(mind.memory.len(), 2);
    assert!(
        mind.memory[0].contains("LOCALPROJ"),
        "newest (binding-local) first: {:?}",
        mind.memory
    );
    assert!(mind.memory[1].contains("SHAREDPROJ"));

    // The shared scope (no binding) sees only the shared fragment.
    let mind = load_character_mind_projection(
        &s.pool, &s.nexus_home, OWNER_A, &s.chr_a, None,
    )
    .await
    .expect("projection ok");
    assert_eq!(mind.memory.len(), 1);
    assert!(mind.memory[0].contains("SHAREDPROJ"));

    // A foreign OWNER_B scope never observes Character A data — it fails
    // closed (repo owner gate) rather than projecting an empty Character mind.
    let foreign = load_character_mind_projection(
        &s.pool, &s.nexus_home, OWNER_B, &s.chr_a, Some(&binding_a1),
    )
    .await;
    assert!(
        foreign.is_err(),
        "foreign owner projection must fail closed, got ok"
    );

    // Writing SOUL.md makes the soul slot present (raw text, honest).
    let bearer = MemoryBearerRef::Character {
        owner_creator_id: OWNER_A,
        character_id: &s.chr_a,
    };
    soul_io::create(&s.nexus_home, bearer).unwrap();
    let doc = soul_io::load(&s.nexus_home, bearer).unwrap();
    soul_io::save(&s.nexus_home, bearer, &doc).unwrap();
    let mind = load_character_mind_projection(
        &s.pool, &s.nexus_home, OWNER_A, &s.chr_a, Some(&binding_a1),
    )
    .await
    .expect("projection ok");
    assert!(mind.soul.is_some(), "SOUL.md present after save");

    drop(s.tmp);
}

#[tokio::test]
async fn promoted_character_long_term_memory_is_projected_into_run() {
    use crate::api::handlers::memory_pipeline::{load_character_mind_projection, process_bearer_review_batch};

    let s = setup().await;
    let ctx = ctxh(&s.pool, &s.chr_a).await;

    // A high-signal creative digest (>= 80 chars, brainstorm) → PromoteToLongTerm.
    let promote_digest = format!(
        "LTM_PROJ_MARKER The tavern ledger records a debt repaid at dawn, and the \
         character named it aloud for the first time, rewriting the family pact."
    );
    let input = pch(
        "pend_promote_proj",
        "sess_promote_proj",
        &promote_digest,
        "brainstorm",
        &s.chr_a,
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let outcome =
        process_bearer_review_batch(&[input], &s.nexus_home, &ctx, &s.pool, deadline).await;
    assert_eq!(outcome.promoted, 1, "high-signal brainstorm must promote");
    assert_eq!(outcome.fragmented, 0);

    // The promoted long-term-memory file is now projected into the run mind.
    let mind = load_character_mind_projection(&s.pool, &s.nexus_home, OWNER_A, &s.chr_a, None)
        .await
        .expect("projection ok");
    assert!(
        !mind.memory.is_empty(),
        "promoted LTM must appear in the Character mind projection"
    );
    // The promoted file's authoritative frontmatter (`memory_kind:`) is the
    // marker that this came from the pipeline's LTM sink, not a fragment row.
    assert!(
        mind.memory.iter().any(|l| l.contains("memory_kind:")),
        "promoted LTM frontmatter must be visible to character run: {:?}",
        mind.memory
    );

    drop(s.tmp);
}
