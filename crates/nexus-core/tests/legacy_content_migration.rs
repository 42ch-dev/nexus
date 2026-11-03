//! P5 (plan `v228-n2-legacy-content-migration`, Task 1): the one-shot
//! legacy-content migration and its data-preservation proof (compass D7).
//!
//! Every scenario runs on a temp nexus home seeded with one row of **every**
//! named legacy family plus both bearers' `SOUL.md` and long-term memory files.
//! The proof is content equivalence, not "the migration ran":
//!
//! * the unified read path serves the same DTOs before and after the migration
//!   (stable external ids, no duplicate intermediate state);
//! * every legacy row's content is present in the provider tables and in the
//!   `memory-store/` projections;
//! * every legacy table and bearer file is byte-identical afterwards (D6);
//! * a completed re-run verifies its manifest and writes nothing;
//! * an already-adopted candidate is accepted, a body that differs from the
//!   legacy source refuses `idempotency_conflict`, and after the marker the
//!   legacy tables no longer reach any read or just-in-time adoption.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use std::path::{Path, PathBuf};

use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, LegacyMigrationStatus};
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::{memory_io, LongTermMemory};
use nexus_local_db::writer_protocol::{init_engine_pool, init_guarded_pool, GuardedPoolOptions};
use nexus_local_db::{
    create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams,
};
use sqlx::SqlitePool;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD: &str = "wld_worldA";
const CREATOR_SOUL: &str = "# Soul\n\nLegacy creator soul document.\n";
const CHARACTER_SOUL: &str = "# Character soul\n\nLegacy character soul document.\n";
const FRAGMENT_TEXT: &str = "Legacy creator fragment summary text.";
const CHARACTER_FRAGMENT_TEXT: &str = "Legacy binding-local character fragment text.";
const CREATOR_NARRATIVE_TEXT: &str = "Legacy creator soul narrative.";
const CHARACTER_NARRATIVE_TEXT: &str = "Legacy character soul narrative.";
const LONG_TERM_BODY: &str = "Legacy creator long-term body.";
const CHARACTER_LONG_TERM_BODY: &str = "Legacy character long-term body.";
const PENDING_DIGEST: &str = "Legacy creator pending review raw digest long enough to classify.";

/// Every legacy memory table the migration must preserve, in a stable order.
const LEGACY_TABLES: [&str; 7] = [
    "memory_pending_review",
    "memory_fragments",
    "memory_soul_narratives",
    "character_memory_pending_review",
    "character_memory_fragments",
    "character_soul_narratives",
    "character_run_captures",
];

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    db_path: PathBuf,
    character_id: String,
    binding_id: String,
}

fn seed_env() -> Env {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().to_path_buf();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &home, CREATOR, "default",
    ))
    .unwrap();
    std::fs::write(
        home.join(".nexus42").join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"default\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, "default");
    Env {
        _tmp: tmp,
        home,
        db_path,
        character_id: String::new(),
        binding_id: String::new(),
    }
}

async fn seed_world(pool: &SqlitePool, world_id: &str) {
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

/// Seed the workspace shell and one row of every legacy memory family.
///
/// The whole seed runs through one engine-mode writer: `character_run_captures`
/// is engine-owned, while an engine writer is also admitted by every direct-mode
/// guard, so one session seeds all of them.
async fn seed_legacy_home() -> Env {
    let mut env = seed_env();
    let guarded = init_engine_pool(&env.db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .unwrap();
    let pool = guarded.clone_pool();
    ensure_creator_row(&pool, CREATOR, "Owner").await.unwrap();
    seed_world(&pool, WORLD).await;
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
    env.character_id = created.character.character_id;
    env.binding_id = created.binding.binding_id;

    nexus_local_db::memory_fragment::create_fragment(
        &pool,
        &nexus_local_db::memory_fragment::MemoryFragmentRecord {
            fragment_id: "frag_legacy".to_string(),
            session_id: "sess_legacy".to_string(),
            creator_id: CREATOR.to_string(),
            keywords: serde_json::json!(["kw_legacy"]).to_string(),
            summary: FRAGMENT_TEXT.to_string(),
            created_at: "2026-03-01T00:00:00Z".to_string(),
            ttl: None,
            world_id: Some(WORLD.to_string()),
        },
    )
    .await
    .unwrap();
    nexus_local_db::create_pending_review(
        &pool,
        &nexus_local_db::PendingReviewRecord {
            pending_id: "pend_legacy".to_string(),
            session_id: "sess_pend_legacy".to_string(),
            creator_id: CREATOR.to_string(),
            world_id: Some(WORLD.to_string()),
            task_kind: "research".to_string(),
            raw_digest: PENDING_DIGEST.to_string(),
            created_at: "2026-03-01T00:00:02Z".to_string(),
        },
    )
    .await
    .unwrap();
    for (world, narrative, fingerprint) in [
        (None, Some(CREATOR_NARRATIVE_TEXT), None),
        (Some(WORLD), None, Some("stats-only-fingerprint")),
    ] {
        nexus_local_db::upsert_soul_narrative(
            &pool,
            &nexus_local_db::SoulNarrativeRecord {
                creator_id: CREATOR.to_string(),
                world_id: world.map(str::to_string),
                narrative: narrative.map(str::to_string),
                generated_at: narrative.map(|_| "2026-03-01T00:00:03Z".to_string()),
                fragment_count_at_generation: 1,
                max_fragment_created_at_at_generation: None,
                distinct_keyword_count_cache: 1,
                stats_fingerprint: fingerprint.map(str::to_string),
                created_at: "2026-03-01T00:00:03Z".to_string(),
                updated_at: "2026-03-01T00:00:03Z".to_string(),
            },
        )
        .await
        .unwrap();
    }

    nexus_local_db::create_character_fragment(
        &pool,
        CREATOR,
        &nexus_local_db::NewCharacterMemoryFragment {
            fragment_id: "hfrag_legacy".to_string(),
            session_id: "hsess_legacy".to_string(),
            character_id: env.character_id.clone(),
            actor_world_binding_id: Some(env.binding_id.clone()),
            keywords: serde_json::json!(["kw_binding"]).to_string(),
            summary: CHARACTER_FRAGMENT_TEXT.to_string(),
            created_at: "2026-03-02T00:00:00Z".to_string(),
            ttl: None,
        },
    )
    .await
    .unwrap();
    nexus_local_db::create_character_pending_review(
        &pool,
        CREATOR,
        &nexus_local_db::CharacterPendingReviewRecord {
            pending_id: "pend_char_legacy".to_string(),
            session_id: "hsess_pend_legacy".to_string(),
            character_id: env.character_id.clone(),
            actor_world_binding_id: Some(env.binding_id.clone()),
            task_kind: "chapter".to_string(),
            raw_digest: PENDING_DIGEST.to_string(),
            created_at: "2026-03-02T00:00:02Z".to_string(),
            source_operation_id: Some("op_pending_source".to_string()),
        },
    )
    .await
    .unwrap();
    nexus_local_db::upsert_character_soul_narrative(
        &pool,
        CREATOR,
        &nexus_local_db::CharacterSoulNarrativeRecord {
            character_id: env.character_id.clone(),
            actor_world_binding_id: Some(env.binding_id.clone()),
            narrative: Some(CHARACTER_NARRATIVE_TEXT.to_string()),
            generated_at: Some("2026-03-02T00:00:03Z".to_string()),
            fragment_count_at_generation: 1,
            max_fragment_created_at_at_generation: None,
            distinct_keyword_count_cache: 1,
            stats_fingerprint: None,
            created_at: "2026-03-02T00:00:03Z".to_string(),
            updated_at: "2026-03-02T00:00:03Z".to_string(),
        },
    )
    .await
    .unwrap();
    let lifecycle_epoch: i64 =
        sqlx::query_scalar("SELECT lifecycle_epoch FROM characters WHERE character_id = ?")
            .bind(&env.character_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    nexus_local_db::capture_character_run(
        &pool,
        CREATOR,
        nexus_local_db::RunCaptureInput {
            operation_id: "op_run_legacy",
            session_id: "sess_run_legacy",
            character_id: &env.character_id,
            binding_id: &env.binding_id,
            pending_id: "pend_run_legacy",
            raw_digest: PENDING_DIGEST,
            captured_at: "2026-03-02T00:00:04Z",
            lifecycle_epoch,
        },
    )
    .await
    .unwrap();
    pool.close().await;
    drop(guarded);

    // The bearer files the migration reads (and never rewrites).
    let creator = MemoryBearerRef::Creator(CREATOR);
    let mut memory = LongTermMemory::new("story_summary");
    memory.set_body(LONG_TERM_BODY);
    memory_io::save_memory(&env.home, creator, "legacy-memory", &memory).unwrap();
    std::fs::write(creator.soul_path(&env.home), CREATOR_SOUL).unwrap();

    let character = MemoryBearerRef::Character {
        owner_creator_id: CREATOR,
        character_id: &env.character_id,
    };
    let mut character_memory = LongTermMemory::new("fact");
    character_memory.set_body(CHARACTER_LONG_TERM_BODY);
    memory_io::save_memory(
        &env.home,
        character,
        "legacy-character-memory",
        &character_memory,
    )
    .unwrap();
    std::fs::write(character.soul_path(&env.home), CHARACTER_SOUL).unwrap();
    env
}

async fn open_core(env: &Env) -> (CoreService, nexus_core::Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: env.home.clone(),
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

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every row of one legacy table rendered as a JSON array (`quote()` keeps the
/// SQLite type of each value; ordering by that rendering keeps the snapshot
/// independent of storage order).
async fn table_rows(pool: &SqlitePool, table: &str) -> Vec<String> {
    let columns: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT name FROM pragma_table_info('{table}')"
    )))
    .fetch_all(pool)
    .await
    .unwrap();
    let rendered = columns
        .iter()
        .map(|column| format!("quote(\"{column}\")"))
        .collect::<Vec<_>>()
        .join(", ");
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT json_array({rendered}) FROM \"{table}\" ORDER BY 1"
    )))
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Every legacy memory table's content, as one comparable value.
async fn legacy_tables(pool: &SqlitePool) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::with_capacity(LEGACY_TABLES.len());
    for table in LEGACY_TABLES {
        out.push((table.to_string(), table_rows(pool, table).await));
    }
    out
}

fn files_under(root: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("bearer entry").path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            let bytes = std::fs::read(&path).expect("bearer bytes");
            out.push((path, bytes));
        }
    }
}

/// The legacy memory bearer files of both bearers — `SOUL.md` and every
/// `memory/long-term/*.md` — bytes included, in path order.
fn bearer_files(env: &Env) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for bearer in [
        MemoryBearerRef::Creator(CREATOR),
        MemoryBearerRef::Character {
            owner_creator_id: CREATOR,
            character_id: &env.character_id,
        },
    ] {
        let soul = bearer.soul_path(&env.home);
        files.push((soul.clone(), std::fs::read(&soul).expect("SOUL bytes")));
        let long_term = bearer
            .validated_long_term_memory_dir(&env.home)
            .expect("bearer ids");
        files_under(&long_term, &mut files);
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

/// One `memory-store/` projection file as text.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 projection")
}

fn assert_core_conflict(error: CoreError, code: &str) {
    match error {
        CoreError::ActorConflict { code: got, .. } => assert_eq!(got, code, "conflict code"),
        other => panic!("expected ActorConflict {code}, got {other:?}"),
    }
}

/// The migration moves every legacy row and bearer file into provider surfaces
/// with the unified read path serving the same content before and after, and a
/// re-run verifies its manifest instead of writing (compass D7).
#[tokio::test]
async fn migration_preserves_every_legacy_object_and_rerun_is_a_no_op() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    let character_id = env.character_id.clone();
    let binding_id = env.binding_id.clone();

    // (1) The unified read path before the marker: every family through E4.
    let fragments_before = serde_json::to_value(
        core.list_memory_fragments(&principal, None, None, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    let pending_before = serde_json::to_value(
        core.list_pending_reviews(&principal, None, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    let memories_before =
        serde_json::to_value(core.list_creator_memories(&principal).await.unwrap()).unwrap();
    let soul_before =
        serde_json::to_value(core.get_creator_soul(&principal).await.unwrap().unwrap()).unwrap();
    let character_pending_before = serde_json::to_value(
        core.list_character_pending_reviews(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            100,
            0,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let character_fragments_before = serde_json::to_value(
        core.list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            100,
            0,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fragments_before["fragments"].as_array().unwrap().len(), 1);
    assert_eq!(pending_before["items"].as_array().unwrap().len(), 1);
    assert_eq!(memories_before.as_array().unwrap().len(), 1);
    assert_eq!(
        character_pending_before["items"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        character_fragments_before["fragments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // (2) The historical surfaces, byte-for-byte, before the pass.
    let tables_before = {
        let pool = plain_pool(&env).await;
        let tables = legacy_tables(&pool).await;
        pool.close().await;
        tables
    };
    let files_before = bearer_files(&env);

    // (3) The pass itself.
    let migration = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(migration.status, LegacyMigrationStatus::Migrated);
    assert_eq!(migration.records, 8, "{migration:?}");
    assert_eq!(migration.candidates, 3, "{migration:?}");
    assert_eq!(migration.profiles, 2, "{migration:?}");
    let families = &migration.families;
    for (family, expected) in [
        ("memory_fragments", 1),
        ("memory_soul_narratives", 2),
        ("memory_long_term", 1),
        ("character_memory_fragments", 1),
        ("character_soul_narratives", 1),
        ("character_run_captures", 1),
        ("character_long_term", 1),
        ("memory_pending_review", 1),
        ("character_memory_pending_review", 2),
        ("SOUL.md", 2),
    ] {
        assert_eq!(
            families.get(family),
            Some(&expected),
            "{family}: {families:?}"
        );
    }

    // (4) Every legacy object's content is present in provider surfaces …
    let pool = plain_pool(&env).await;
    for (family, needle) in [
        ("memory_fragments", FRAGMENT_TEXT),
        ("memory_soul_narratives", CREATOR_NARRATIVE_TEXT),
        ("memory_long_term", LONG_TERM_BODY),
        ("character_memory_fragments", CHARACTER_FRAGMENT_TEXT),
        ("character_soul_narratives", CHARACTER_NARRATIVE_TEXT),
        ("character_run_captures", "op_run_legacy"),
        ("character_long_term", CHARACTER_LONG_TERM_BODY),
        ("memory_pending_review", PENDING_DIGEST),
        ("character_memory_pending_review", "hsess_pend_legacy"),
        ("SOUL.md", "Legacy creator soul document."),
        ("SOUL.md", "Legacy character soul document."),
    ] {
        let found = count(
            &pool,
            &format!(
                "SELECT (SELECT COUNT(*) FROM memory_records WHERE content_json LIKE '%{needle}%') \
                 + (SELECT COUNT(*) FROM memory_candidates WHERE content_json LIKE '%{needle}%') \
                 + (SELECT COUNT(*) FROM memory_profiles WHERE document_json LIKE '%{needle}%')"
            ),
        )
        .await;
        assert!(found > 0, "family {family} content {needle:?} is missing");
    }
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM memory_records").await, 8);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_candidates").await,
        3
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_profiles").await,
        2
    );
    // The stats-only narrative keeps its nullable narrative (empty text) and the
    // receipt-only run capture keeps its tuple without inventing a review.
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_records \
             WHERE content_json LIKE '%stats-only-fingerprint%'",
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_records WHERE kind = 'nexus.soul_narrative'",
        )
        .await,
        3
    );
    // A fragment's provider revision is its preserved legacy revision + 1.
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_records \
             WHERE record_id LIKE 'lgc:character_memory_fragments:%' AND revision = 1",
        )
        .await,
        1
    );

    // … and every migrated record owns its `memory-store/` projection.
    let store = env.db_path.parent().unwrap().join("memory-store");
    let record_ids: Vec<String> =
        sqlx::query_scalar("SELECT record_id FROM memory_records ORDER BY record_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    for record_id in &record_ids {
        let projection =
            std::fs::read_to_string(store.join("records").join(format!("{record_id}.md")))
                .unwrap_or_else(|error| panic!("record projection {record_id}: {error}"));
        assert!(
            projection.contains("record_id: ") && projection.contains(record_id),
            "projection for {record_id} is not its own"
        );
    }
    let mut profile_files = Vec::new();
    files_under(&store.join("profiles"), &mut profile_files);
    assert_eq!(
        profile_files.len(),
        2,
        "one projection per migrated SOUL.md"
    );
    let profiles = profile_files
        .iter()
        .map(|(_, bytes)| text(bytes))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(profiles.contains(CREATOR_SOUL.trim()), "creator SOUL bytes");
    assert!(
        profiles.contains(CHARACTER_SOUL.trim()),
        "character SOUL bytes"
    );
    pool.close().await;

    // (5) The marker flips reads to provider-only with the same content.
    let fragments_after = serde_json::to_value(
        core.list_memory_fragments(&principal, None, None, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    let pending_after = serde_json::to_value(
        core.list_pending_reviews(&principal, None, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    let memories_after =
        serde_json::to_value(core.list_creator_memories(&principal).await.unwrap()).unwrap();
    let soul_after =
        serde_json::to_value(core.get_creator_soul(&principal).await.unwrap().unwrap()).unwrap();
    let character_pending_after = serde_json::to_value(
        core.list_character_pending_reviews(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            100,
            0,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let character_fragments_after = serde_json::to_value(
        core.list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            100,
            0,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fragments_after, fragments_before, "creator fragments");
    assert_eq!(pending_after, pending_before, "creator pending queue");
    assert_eq!(
        memories_after, memories_before,
        "creator long-term memories"
    );
    assert_eq!(soul_after, soul_before, "creator SOUL profile");
    assert_eq!(
        character_pending_after, character_pending_before,
        "character pending queue"
    );
    assert_eq!(
        character_fragments_after, character_fragments_before,
        "character fragments"
    );

    // (6) The legacy surfaces are byte-identical through the pass (D6).
    let tables_after = {
        let pool = plain_pool(&env).await;
        let tables = legacy_tables(&pool).await;
        pool.close().await;
        tables
    };
    assert_eq!(tables_after, tables_before, "legacy tables stay write-idle");
    assert_eq!(
        bearer_files(&env),
        files_before,
        "bearer files stay byte-identical"
    );

    // (7) A completed re-run verifies the manifest and writes nothing.
    let rerun = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(rerun.status, LegacyMigrationStatus::AlreadyComplete);
    assert_eq!(rerun.digest, migration.digest, "the same snapshot digest");
    assert_eq!(rerun.records, migration.records);
    assert_eq!(rerun.families, migration.families);
    let pool = plain_pool(&env).await;
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM memory_records").await, 8);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_candidates").await,
        3
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_profiles").await,
        2
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_journal").await,
        1,
        "the re-run journals no second intent"
    );
    pool.close().await;
}

/// A legacy queue row the provider already adopted and consumed is the
/// provider's own disposition: the pass accepts it and never resurrects the
/// pending row.
#[tokio::test]
async fn migration_accepts_an_already_consumed_adoption() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;

    // The pre-marker dismissal adopts the projection and consumes it in one
    // guarded transaction, so the candidate is provider-owned before the pass.
    assert!(
        core.delete_pending_review(&principal, "pend_legacy".to_string())
            .await
            .unwrap()
            .success
    );

    let migration = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(migration.status, LegacyMigrationStatus::Migrated);
    assert_eq!(migration.candidates, 3, "{migration:?}");

    // The consumed row stays consumed: post-marker reads see the provider's
    // outcome, not a resurrected pending candidate.
    let pending = serde_json::to_value(
        core.list_pending_reviews(&principal, None, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pending["items"].as_array().unwrap().len(), 0);

    let pool = plain_pool(&env).await;
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_candidates \
             WHERE candidate_id LIKE 'lgc:memory_pending_review:%' AND status = 'consumed'",
        )
        .await,
        1
    );
    // The legacy row itself is untouched.
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_pending_review").await,
        1
    );
    pool.close().await;
}

/// An adopted record whose body no longer matches the legacy source refuses the
/// pass (`idempotency_conflict`) instead of overwriting the provider's state.
#[tokio::test]
async fn migration_refuses_a_body_that_differs_from_the_legacy_source() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;

    // The edit adopts the projected long-term memory and then changes its body.
    assert!(core
        .update_creator_memory(&principal, "legacy-memory", "Edited after adoption.", None)
        .await
        .unwrap());

    let refused = core.migrate_legacy_content(&principal).await.unwrap_err();
    assert_core_conflict(refused, "idempotency_conflict");

    let pool = plain_pool(&env).await;
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_provider_meta \
             WHERE key = 'legacy_content_migration:spc_local_default'",
        )
        .await,
        0,
        "a refused pass publishes no completion marker"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_records WHERE content_json LIKE '%Edited after adoption.%'",
        )
        .await,
        1,
        "the provider keeps the user's edit"
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_journal").await,
        1,
        "a refused pass journals nothing new"
    );
    pool.close().await;
}

/// After the marker every read is provider-only: a legacy row written afterwards
/// reaches neither the merged read nor the just-in-time adoption path.
#[tokio::test]
async fn markers_turn_reads_and_adoption_provider_only() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    core.migrate_legacy_content(&principal).await.unwrap();
    drop(core);

    // A stale legacy write lands after the space was migrated.
    let guarded = init_guarded_pool(&env.db_path, CREATOR).await.unwrap();
    let pool = guarded.clone_pool();
    nexus_local_db::memory_fragment::create_fragment(
        &pool,
        &nexus_local_db::memory_fragment::MemoryFragmentRecord {
            fragment_id: "frag_after_marker".to_string(),
            session_id: "sess_after".to_string(),
            creator_id: CREATOR.to_string(),
            keywords: serde_json::json!(["kw_after"]).to_string(),
            summary: "Legacy row written after the migration.".to_string(),
            created_at: "2026-04-01T00:00:00Z".to_string(),
            ttl: None,
            world_id: None,
        },
    )
    .await
    .unwrap();
    nexus_local_db::create_pending_review(
        &pool,
        &nexus_local_db::PendingReviewRecord {
            pending_id: "pend_after_marker".to_string(),
            session_id: "sess_pend_after".to_string(),
            creator_id: CREATOR.to_string(),
            world_id: None,
            task_kind: "research".to_string(),
            raw_digest: "Late legacy digest that must not reach any read.".to_string(),
            created_at: "2026-04-01T00:00:01Z".to_string(),
        },
    )
    .await
    .unwrap();
    pool.close().await;
    drop(guarded);

    let (core, principal) = open_core(&env).await;
    let fragments = serde_json::to_value(
        core.list_memory_fragments(&principal, None, None, 100)
            .await
            .unwrap(),
    )
    .unwrap();
    let rendered = serde_json::to_string(&fragments).unwrap();
    assert!(
        !rendered.contains("frag_after_marker"),
        "a post-marker legacy row never resurfaces through the bridge: {rendered}"
    );
    // The just-in-time adoption path is inert too: the legacy pending id has no
    // provider candidate and the marker makes the bridge project nothing.
    assert!(matches!(
        core.delete_pending_review(&principal, "pend_after_marker".to_string())
            .await,
        Err(CoreError::NotFound { .. })
    ));
    // The space is no longer the snapshot that was migrated, so a re-run refuses
    // rather than silently absorbing the late write.
    let refused = core.migrate_legacy_content(&principal).await.unwrap_err();
    assert_core_conflict(refused, "idempotency_conflict");
}
