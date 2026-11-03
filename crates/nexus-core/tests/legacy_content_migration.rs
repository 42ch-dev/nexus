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
//! * an already-adopted candidate, and an edit made before the pass through the
//!   rerouted entry points, are the provider's own successor and are accepted;
//!   a row at a legacy id without that legacy identity refuses
//!   `idempotency_conflict`; after the marker the legacy tables no longer reach
//!   any read or just-in-time adoption.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use std::path::{Path, PathBuf};

use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, LegacyMigrationStatus};
use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::{memory_io, LongTermMemory};
use nexus_local_db::writer_protocol::{init_engine_pool, init_guarded_pool, GuardedPoolOptions};
use nexus_local_db::{
    create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams,
};
use serde_json::json;
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
/// The Character's authored body opens with a blank line and indentation: the
/// parser trims that prefix, so the migration must serve the authored bytes.
const CHARACTER_LONG_TERM_BODY: &str = "\n    Legacy character long-term body.\n";
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

/// Every JSON document one column holds, in id order.
async fn contents(pool: &SqlitePool, sql: &str) -> Vec<serde_json::Value> {
    let raw: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_all(pool)
        .await
        .unwrap();
    raw.iter()
        .map(|value| serde_json::from_str(value).expect("stored JSON"))
        .collect()
}

/// The `nexus_legacy` block of one projected content, or null.
fn legacy(content: &serde_json::Value) -> serde_json::Value {
    content
        .pointer("/structured/nexus_legacy")
        .cloned()
        .unwrap_or_default()
}

/// Assert `expected`'s fields equal the projected `nexus_legacy` block's.
fn assert_legacy(content: &serde_json::Value, expected: &serde_json::Value, label: &str) {
    let block = legacy(content);
    for (field, want) in expected.as_object().expect("expected legacy fields") {
        assert_eq!(&block[field], want, "{label}.{field}: {content}");
    }
}

/// The terminal refusal of a subject whose journaled file intent is unsettled.
fn assert_unreconciled(error: CoreError, subject_id: &str) {
    match error {
        CoreError::Internal { category } => {
            assert!(
                category.contains("unresolved memory journal intent"),
                "{category}"
            );
            assert!(category.contains(subject_id), "{subject_id}: {category}");
        }
        other => panic!("expected the unresolved-intent refusal, got {other:?}"),
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

    // (4) Every legacy object's content is in provider surfaces, value-exact:
    // each record/candidate carries the legacy text and fields verbatim, and
    // each bearer file's own bytes travel with its projection (P3-F3).
    let pool = plain_pool(&env).await;
    let records = contents(
        &pool,
        "SELECT content_json FROM memory_records ORDER BY record_id",
    )
    .await;
    let candidates = contents(
        &pool,
        "SELECT content_json FROM memory_candidates ORDER BY candidate_id",
    )
    .await;
    for (family, text, expected) in [
        (
            "memory_fragments",
            FRAGMENT_TEXT,
            json!({
                "family": "memory_fragments",
                "fragment_id": "frag_legacy",
                "session_id": "sess_legacy",
                "world_id": WORLD,
                "keywords": ["kw_legacy"],
                "created_at": "2026-03-01T00:00:00Z",
            }),
        ),
        (
            "memory_soul_narratives",
            CREATOR_NARRATIVE_TEXT,
            json!({
                "family": "memory_soul_narratives",
                "scope_id": null,
                "generated_at": "2026-03-01T00:00:03Z",
                "fragment_count_at_generation": 1,
            }),
        ),
        (
            "character_memory_fragments",
            CHARACTER_FRAGMENT_TEXT,
            json!({
                "family": "character_memory_fragments",
                "fragment_id": "hfrag_legacy",
                "session_id": "hsess_legacy",
                "actor_world_binding_id": binding_id,
                "keywords": ["kw_binding"],
                "created_at": "2026-03-02T00:00:00Z",
            }),
        ),
        (
            "character_soul_narratives",
            CHARACTER_NARRATIVE_TEXT,
            json!({
                "family": "character_soul_narratives",
                "scope_id": binding_id,
                "generated_at": "2026-03-02T00:00:03Z",
                "fragment_count_at_generation": 1,
            }),
        ),
        (
            "memory_long_term",
            LONG_TERM_BODY,
            json!({
                "family": "memory_long_term",
                "slug": "legacy-memory",
                "memory_kind": "story_summary",
            }),
        ),
        (
            "character_long_term",
            CHARACTER_LONG_TERM_BODY,
            json!({
                "family": "character_long_term",
                "slug": "legacy-character-memory",
                "memory_kind": "fact",
            }),
        ),
    ] {
        let matched: Vec<&serde_json::Value> = records
            .iter()
            .filter(|content| content["text"] == text && legacy(content)["family"] == family)
            .collect();
        assert_eq!(matched.len(), 1, "family {family} maps to one record");
        assert_eq!(matched[0]["text"], text, "{family}: the exact source text");
        assert_legacy(matched[0], &expected, family);
    }
    // A receipt-only capture keeps its tuple and invents no review outcome; the
    // stats-only narrative maps its absent narrative to empty text.
    let run_capture = records
        .iter()
        .find(|content| legacy(content)["family"] == "character_run_captures")
        .expect("the receipt-only run capture");
    assert_eq!(run_capture["text"], "", "a capture carries no text");
    assert_legacy(
        run_capture,
        &json!({
            "family": "character_run_captures",
            "operation_id": "op_run_legacy",
            "session_id": "sess_run_legacy",
            "binding_id": binding_id,
            "pending_id": "pend_run_legacy",
        }),
        "character_run_captures",
    );
    let stats_only = records
        .iter()
        .find(|content| legacy(content)["stats_fingerprint"] == "stats-only-fingerprint")
        .expect("the stats-only narrative");
    assert_eq!(
        stats_only["text"], "",
        "an absent narrative maps to empty text"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_records WHERE kind = 'nexus.soul_narrative'",
        )
        .await,
        3,
        "both Creators' scopes plus the Character's one narrative"
    );
    // A fragment's provider revision is its preserved legacy revision + 1.
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_records \
             WHERE record_id LIKE 'lgc:character_memory_fragments:%' AND revision = 1",
        )
        .await,
        1,
        "legacy revision 0 is provider revision 1"
    );
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM memory_records").await, 8);
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_candidates").await,
        3
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM memory_profiles").await,
        2
    );

    // The writer's own long-term files, byte-exact per file: `content.text` is
    // the file's body, and the source document, its frontmatter block and its
    // body are the file's own bytes (P3-F3/R2-C1).
    for (bearer, slug) in [
        (MemoryBearerRef::Creator(CREATOR), "legacy-memory"),
        (
            MemoryBearerRef::Character {
                owner_creator_id: CREATOR,
                character_id: &env.character_id,
            },
            "legacy-character-memory",
        ),
    ] {
        let source = std::fs::read_to_string(bearer.long_term_memory_path(&env.home, slug))
            .expect("long-term file bytes");
        let spans = nexus_creator_memory::long_term_memory::source_spans(&source)
            .expect("memory source spans");
        let content = records
            .iter()
            .find(|content| legacy(content)["slug"] == slug)
            .unwrap_or_else(|| panic!("no migrated record for {slug}"));
        assert_eq!(
            content["text"],
            &source[spans.body.clone()],
            "{slug}: content.text is the exact source body"
        );
        assert_eq!(
            legacy(content)["document"],
            source,
            "{slug}: the exact file bytes"
        );
        assert_eq!(
            legacy(content)["frontmatter"],
            &source[spans.frontmatter],
            "{slug}: the exact frontmatter block"
        );
        assert_eq!(
            legacy(content)["body"],
            &source[spans.body],
            "{slug}: the exact body bytes"
        );
    }

    // R2-C1: the Character file's body opens with a blank line and indentation,
    // so the exact body the record serves and the parser's trimmed view differ —
    // the assertions above cannot be satisfied by the trimmed view.
    let character = MemoryBearerRef::Character {
        owner_creator_id: CREATOR,
        character_id: &env.character_id,
    };
    let character_source = std::fs::read_to_string(
        character.long_term_memory_path(&env.home, "legacy-character-memory"),
    )
    .expect("character long-term file bytes");
    let character_spans = nexus_creator_memory::long_term_memory::source_spans(&character_source)
        .expect("memory source spans");
    let character_body: &str = &character_source[character_spans.body.clone()];
    assert!(
        character_body.starts_with("\n    "),
        "the fixture body opens with a blank line and indentation"
    );
    let character_content = records
        .iter()
        .find(|content| legacy(content)["slug"] == "legacy-character-memory")
        .expect("the character's migrated record");
    assert_eq!(
        character_content["text"], character_body,
        "content.text is the exact source body"
    );
    assert_ne!(
        character_content["text"],
        LongTermMemory::parse(&character_source)
            .expect("parsed memory")
            .body,
        "the served text is not the parser's trimmed view"
    );

    // Every pending queue row keeps its own values, exactly.
    let creator_pending = candidates
        .iter()
        .find(|content| legacy(content)["family"] == "memory_pending_review")
        .expect("the creator pending review");
    assert_legacy(
        creator_pending,
        &json!({
            "family": "memory_pending_review",
            "pending_id": "pend_legacy",
            "session_id": "sess_pend_legacy",
            "world_id": WORLD,
            "task_kind": "research",
            "raw_digest": PENDING_DIGEST,
        }),
        "memory_pending_review",
    );
    let character_pending: Vec<&serde_json::Value> = candidates
        .iter()
        .filter(|content| legacy(content)["family"] == "character_memory_pending_review")
        .collect();
    assert_eq!(character_pending.len(), 2, "the captured queue rows");
    let queued = character_pending
        .iter()
        .find(|content| legacy(content)["pending_id"] == "pend_char_legacy")
        .expect("the seeded character queue row");
    assert_legacy(
        queued,
        &json!({
            "family": "character_memory_pending_review",
            "pending_id": "pend_char_legacy",
            "session_id": "hsess_pend_legacy",
            "task_kind": "chapter",
            "raw_digest": PENDING_DIGEST,
            "actor_world_binding_id": binding_id,
        }),
        "character_memory_pending_review",
    );
    let captured = character_pending
        .iter()
        .find(|content| legacy(content)["pending_id"] == "pend_run_legacy")
        .expect("the receipt-only queue row");
    assert_legacy(
        captured,
        &json!({
            "family": "character_memory_pending_review",
            "session_id": "sess_run_legacy",
            "task_kind": "unknown",
            "source_operation_id": "op_run_legacy",
        }),
        "character_memory_pending_review (receipt-only)",
    );

    // Both `SOUL.md` documents are their files' exact bytes.
    let souls: Vec<String> = sqlx::query_scalar("SELECT document_json FROM memory_profiles")
        .fetch_all(&pool)
        .await
        .unwrap();
    let mut bodies: Vec<String> = souls
        .iter()
        .map(|document| {
            serde_json::from_str::<serde_json::Value>(document).expect("profile document")["body"]
                .as_str()
                .expect("profile body")
                .to_string()
        })
        .collect();
    bodies.sort();
    let mut expected_souls = vec![CREATOR_SOUL.to_string(), CHARACTER_SOUL.to_string()];
    expected_souls.sort();
    assert_eq!(bodies, expected_souls, "each SOUL.md's exact bytes");

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
    let mut projected_souls = profile_files
        .iter()
        .map(|(_, bytes)| {
            let rendered = text(bytes);
            // `---`-delimited header, one blank line, then the exact document.
            rendered
                .split_once("\n\n")
                .unwrap_or_else(|| panic!("profile projection framing: {rendered}"))
                .1
                .to_string()
        })
        .collect::<Vec<_>>();
    projected_souls.sort();
    let mut expected_projected = vec![CREATOR_SOUL.to_string(), CHARACTER_SOUL.to_string()];
    expected_projected.sort();
    assert_eq!(
        projected_souls, expected_projected,
        "each projection's exact SOUL bytes"
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

/// PR-F1: an edit made through the rerouted entry point before the pass adopts
/// the legacy row and then changes it. That adopted row is the object's
/// legitimate successor — the legacy source is never written (D6) — so the pass
/// accepts it, publishes the marker for the whole space, and provider-only
/// reads serve the edited body.
#[tokio::test]
async fn migration_accepts_a_memory_edit_made_before_the_pass() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    let files_before = bearer_files(&env);

    // The edit adopts the projected long-term memory and then changes its body.
    assert!(core
        .update_creator_memory(&principal, "legacy-memory", "Edited after adoption.", None)
        .await
        .unwrap());

    let migration = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(migration.status, LegacyMigrationStatus::Migrated);

    // The marker records the revision provider-only reads serve: a rebuilt
    // served snapshot equals it, while a source-derived one would not.
    let rerun = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(rerun.status, LegacyMigrationStatus::AlreadyComplete);
    assert_eq!(rerun.digest, migration.digest, "the same served snapshot");

    let record = serde_json::to_value(
        core.get_creator_memory(&principal, "legacy-memory")
            .await
            .unwrap()
            .expect("the edited memory is listed"),
    )
    .unwrap();
    assert_eq!(
        record["content"]["text"], "Edited after adoption.",
        "provider-only reads serve the edit"
    );
    assert_eq!(
        bearer_files(&env),
        files_before,
        "the edit never writes the legacy source"
    );
}

/// PR-F1: the same for a `SOUL.md` edited before the pass. The profile write
/// creates the provider row instead of adopting a projection, so its document
/// and revision are the provider's own successor as well.
#[tokio::test]
async fn migration_accepts_a_soul_edit_made_before_the_pass() {
    const EDITED_SOUL: &str = "# Soul\n\nEdited before the migration.\n";
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    let files_before = bearer_files(&env);

    core.set_creator_soul(&principal, EDITED_SOUL, None)
        .await
        .expect("the pre-migration SOUL write");

    let migration = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(migration.status, LegacyMigrationStatus::Migrated);
    let rerun = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(rerun.status, LegacyMigrationStatus::AlreadyComplete);
    assert_eq!(rerun.digest, migration.digest, "the same served snapshot");

    let soul = core
        .get_creator_soul(&principal)
        .await
        .unwrap()
        .expect("the edited SOUL is listed");
    assert_eq!(
        soul.document.body, EDITED_SOUL,
        "provider-only SOUL reads serve the edit"
    );
    assert_eq!(
        bearer_files(&env),
        files_before,
        "the write never touches `SOUL.md`"
    );
}

/// The successor rule is bounded by identity: a row that sits at a legacy id
/// without the bridge's own provenance is unrelated content, so the pass
/// refuses instead of absorbing it into the marker.
#[tokio::test]
async fn a_row_without_the_legacy_identity_at_a_legacy_id_refuses_the_pass() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    core.migrate_legacy_content(&principal).await.unwrap();

    // A foreign provenance at the migrated id, and the marker lost (an
    // interrupted upgrade), so the pass re-enters its first-pass path.
    let record_id = "lgc:memory_fragments:frag_legacy";
    let rewritten = sqlx::query(
        "UPDATE memory_records \
         SET provenance_json = '{\"captured_by\":\"someone-else\"}' WHERE record_id = ?",
    )
    .bind(record_id)
    .execute(core.pool())
    .await
    .unwrap();
    assert_eq!(rewritten.rows_affected(), 1, "the migrated fragment exists");
    sqlx::query("DELETE FROM memory_provider_meta WHERE key = ?")
        .bind(nexus_core::memory_provider::legacy::legacy_migration_key(
            nexus_core::memory_provider::DEFAULT_SPACE_ID,
        ))
        .execute(core.pool())
        .await
        .unwrap();

    let refused = core.migrate_legacy_content(&principal).await.unwrap_err();
    assert_core_conflict(refused, "idempotency_conflict");
    let pool = plain_pool(&env).await;
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

/// P5-F1: the migration commits file effects for every bearer under **one**
/// intent, so an interrupted publication withholds every affected subject — not
/// only the first — and the recovery pass then publishes them all.
#[tokio::test]
async fn a_failed_publication_fences_every_affected_subject_until_recovery() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    let character_id = env.character_id.clone();
    let binding_id = env.binding_id.clone();
    let creator_subject = nexus_core::memory_provider::legacy::creator_subject_id(CREATOR);
    let character_subject =
        nexus_core::memory_provider::legacy::character_subject_id(&character_id);

    // Marker, rows and intent commit; the file publication stops after staging.
    core.memory_provider_for_test()
        .expect("pinned provider")
        .inject_file_fault(false);
    let failed = core.migrate_legacy_content(&principal).await.unwrap_err();
    assert!(
        matches!(failed, CoreError::Internal { .. }),
        "outcome uncertain: {failed:?}"
    );

    // Both bearers answer the terminal unresolved-intent refusal: the one intent
    // fences every subject it committed files for.
    assert_unreconciled(
        core.list_creator_memories(&principal).await.unwrap_err(),
        &creator_subject,
    );
    assert_unreconciled(
        core.list_character_memory_fragments(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            100,
            0,
        )
        .await
        .unwrap_err(),
        &character_subject,
    );
    // The queues are fenced by the same set.
    assert_unreconciled(
        core.list_pending_reviews(&principal, None, 100)
            .await
            .unwrap_err(),
        &creator_subject,
    );
    assert_unreconciled(
        core.list_character_pending_reviews(
            &principal,
            character_id.clone(),
            Some(binding_id.clone()),
            100,
            0,
        )
        .await
        .unwrap_err(),
        &character_subject,
    );

    let pool = plain_pool(&env).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM memory_journal")
            .fetch_all(&pool)
            .await
            .unwrap(),
        vec!["quarantined".to_string()]
    );
    let record_ids: Vec<String> =
        sqlx::query_scalar("SELECT record_id FROM memory_records ORDER BY record_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(record_ids.len(), 8);
    let store = env.db_path.parent().unwrap().join("memory-store");
    assert!(
        !store
            .join("records")
            .join(format!("{}.md", record_ids[0]))
            .exists(),
        "the failed pass published no projection"
    );
    pool.close().await;

    // A reopen runs the recovery pass, which publishes every owed projection.
    drop(core);
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM memory_journal")
            .fetch_all(&pool)
            .await
            .unwrap(),
        vec!["applied".to_string()]
    );
    for record_id in &record_ids {
        assert!(
            store
                .join("records")
                .join(format!("{record_id}.md"))
                .exists(),
            "recovery publishes {record_id}"
        );
    }
    pool.close().await;
    assert_eq!(
        core.list_creator_memories(&principal).await.unwrap().len(),
        1,
        "the creator's content is readable once its files are published"
    );
    let fragments = serde_json::to_value(
        core.list_character_memory_fragments(&principal, character_id, Some(binding_id), 100, 0)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        fragments["fragments"].as_array().unwrap().len(),
        1,
        "the character's content is readable once its files are published"
    );
    // Retry after recovery: the marker verifies (P5-F2's success branch).
    assert_eq!(
        core.migrate_legacy_content(&principal)
            .await
            .unwrap()
            .status,
        LegacyMigrationStatus::AlreadyComplete
    );
}

/// P5-F2: a retry on the completed marker re-drives the intent the failed pass
/// left unsettled instead of reporting the space `already_complete`.
#[tokio::test]
async fn a_retry_settles_the_unsettled_migration_before_reporting_complete() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    core.memory_provider_for_test()
        .expect("pinned provider")
        .inject_file_fault(false);
    assert!(core.migrate_legacy_content(&principal).await.is_err());

    // No reopen: the retry itself publishes what the failed pass owed.
    let retried = core.migrate_legacy_content(&principal).await.unwrap();
    assert_eq!(retried.status, LegacyMigrationStatus::AlreadyComplete);

    let pool = plain_pool(&env).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM memory_journal")
            .fetch_all(&pool)
            .await
            .unwrap(),
        vec!["applied".to_string()]
    );
    let record_ids: Vec<String> =
        sqlx::query_scalar("SELECT record_id FROM memory_records ORDER BY record_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let store = env.db_path.parent().unwrap().join("memory-store");
    for record_id in &record_ids {
        assert!(
            store
                .join("records")
                .join(format!("{record_id}.md"))
                .exists(),
            "the retry publishes {record_id}"
        );
    }
    pool.close().await;
    assert_eq!(
        core.list_creator_memories(&principal).await.unwrap().len(),
        1,
        "the retry un-fences the subject it settled"
    );
}

/// P5-F2's other branch: while the owed target cannot be settled, the marker
/// path refuses with the retryable unsettled-predecessor error — never
/// `already_complete`, and never by overwriting the foreign bytes.
#[tokio::test]
async fn a_retry_refuses_while_the_owed_target_stays_quarantined() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    core.memory_provider_for_test()
        .expect("pinned provider")
        .inject_file_fault(false);
    assert!(core.migrate_legacy_content(&principal).await.is_err());

    // A foreign write now owns one owed target, so the re-drive cannot publish.
    let pool = plain_pool(&env).await;
    let targets: String = sqlx::query_scalar("SELECT targets_json FROM memory_journal")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    let effects: serde_json::Value = serde_json::from_str(&targets).unwrap();
    let path = effects[0]["path"]
        .as_str()
        .expect("effect path")
        .to_string();
    let store = env.db_path.parent().unwrap().join("memory-store");
    let target = store.join(&path);
    std::fs::create_dir_all(target.parent().expect("target dir")).unwrap();
    std::fs::write(&target, "someone else's file\n").unwrap();

    let refused = core.migrate_legacy_content(&principal).await.unwrap_err();
    match refused {
        CoreError::Internal { category } => assert!(
            category.contains("unsettled"),
            "the retryable unsettled-predecessor refusal: {category}"
        ),
        other => panic!("expected a typed refusal, got {other:?}"),
    }
    let pool = plain_pool(&env).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM memory_journal")
            .fetch_all(&pool)
            .await
            .unwrap(),
        vec!["quarantined".to_string()]
    );
    pool.close().await;
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "someone else's file\n",
        "the pass never overwrites bytes it did not produce"
    );
}

/// P3-F3: the manifest covers the source bytes, so a rewrite of bytes the
/// parsed view drops — an authored YAML comment, a blank first body line — is a
/// changed snapshot, never a silent no-op.
#[tokio::test]
async fn a_long_term_byte_change_outside_the_parsed_view_refuses_the_rerun() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    core.migrate_legacy_content(&principal).await.unwrap();

    let creator = MemoryBearerRef::Creator(CREATOR);
    let path = creator.long_term_memory_path(&env.home, "legacy-memory");
    let source = std::fs::read_to_string(&path).expect("long-term file");
    let (front, body) = source.split_once("\n---\n").expect("frontmatter framing");
    let changed = format!("{front}\n# an authored comment\n---\n\n{body}");
    let before = LongTermMemory::parse(&source).expect("parsed before");
    let after = LongTermMemory::parse(&changed).expect("parsed after");
    assert_eq!(
        after.frontmatter, before.frontmatter,
        "the parsed frontmatter is unchanged"
    );
    assert_eq!(after.body, before.body, "the parsed body is unchanged");
    assert_ne!(changed, source, "the file's bytes did change");
    std::fs::write(&path, &changed).expect("rewrite the source file");

    let refused = core.migrate_legacy_content(&principal).await.unwrap_err();
    assert_core_conflict(refused, "idempotency_conflict");
}

/// P5-F4: the marker records the values provider-only reads served when it was
/// written, so a provider row that moved afterwards is a drift conflict — the
/// re-run may not silently adopt it as the recorded snapshot.
#[tokio::test]
async fn a_provider_row_that_moved_after_the_pass_refuses_the_rerun() {
    let env = seed_legacy_home().await;
    let (core, principal) = open_core(&env).await;
    core.migrate_legacy_content(&principal).await.unwrap();

    // The row the pass wrote, then a provider-native metadata bump: the content
    // is untouched while its revision and timestamp move.
    let record_id = "lgc:memory_fragments:frag_legacy";
    let bumped = sqlx::query(
        "UPDATE memory_records SET revision = revision + 1, \
         updated_at = '2026-05-01T00:00:00Z' WHERE record_id = ?",
    )
    .bind(record_id)
    .execute(core.pool())
    .await
    .unwrap();
    assert_eq!(bumped.rows_affected(), 1, "the migrated fragment exists");
    let content_before: String =
        sqlx::query_scalar("SELECT content_json FROM memory_records WHERE record_id = ?")
            .bind(record_id)
            .fetch_one(core.pool())
            .await
            .unwrap();
    let marker_sql =
        "SELECT value FROM memory_provider_meta WHERE key = 'legacy_content_migration:spc_local_default'";
    let marker_before: String = sqlx::query_scalar(marker_sql)
        .fetch_one(core.pool())
        .await
        .unwrap();

    let refused = core.migrate_legacy_content(&principal).await.unwrap_err();
    assert_core_conflict(refused, "idempotency_conflict");

    // Nothing was overwritten, the drifted row stays, and the marker keeps the
    // snapshot the completed pass recorded.
    let pool = plain_pool(&env).await;
    let content_after: String =
        sqlx::query_scalar("SELECT content_json FROM memory_records WHERE record_id = ?")
            .bind(record_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(content_after, content_before, "the provider keeps its row");
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM memory_records WHERE record_id = 'lgc:memory_fragments:frag_legacy' AND revision = 2",
        )
        .await,
        1,
        "the provider keeps its metadata"
    );
    let marker_after: String = sqlx::query_scalar(marker_sql).fetch_one(&pool).await.unwrap();
    assert_eq!(
        marker_after, marker_before,
        "a refused re-run leaves the recorded marker as it was"
    );
    pool.close().await;
}
