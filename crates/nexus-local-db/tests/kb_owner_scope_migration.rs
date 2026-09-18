//! v1.184 P1 Task 1 — `kb_key_blocks` owner-scope migration proofs.
//!
//! Covers migration `20260905000002_actor_knowledge_owners.sql`:
//! - pre-v1.184 upgrade fidelity: every legacy column/payload byte, child row
//!   (`kb_source_anchors`, `kb_relationships`, `mind_states`,
//!   `actor_world_bindings.world_sheet_entry_id`), and pre-existing index
//!   survives the rebuild;
//! - exactly-one-owner CHECK constraints and the owner-scoped partial unique
//!   indexes on the rebuilt `kb_key_blocks`;
//! - the v1.191 P1 T3 holder cutover: registry/quarantine schema, the native
//!   governance columns (the legacy `creator_only` column is gone), the
//!   `creator_only = true` → stored-Creator `owner-private` backfill, the
//!   knowledge revisions, and every preflight abort (all in
//!   `v1191_holder_migration_*` cases below);
//! - owner FK actions (CASCADE world, RESTRICT character/binding, SET NULL
//!   `WorldSheet` link);
//! - owner-scoped active uniqueness partial unique indexes;
//! - legacy World `KbStore` behavior on an upgraded database.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::store::KbStoreError;
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::migrate::{Migration, Migrator};
use sqlx::SqlitePool;

/// Version of the owner-scope migration under test. The 14-digit
/// `20260905000002` keeps sqlx's numeric ordering after every shipped
/// migration (the July/August rebuilds use 14-digit versions, e.g.
/// `20260815000001`; a 12-digit `202609050002` would sort *before* them).
const OWNER_MIGRATION_VERSION: i64 = 20_260_905_000_002;

const CREATOR: &str = "ctr_cccccccccccccccccccccccccccccccc";
const WORLD_A: &str = "wld_ownerA";
const WORLD_CASCADE: &str = "wld_ownerC";
const CHARACTER: &str = "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CHARACTER_B: &str = "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BINDING: &str = "awb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Index names that existed before v1.184 P1 and must be restored verbatim.
const PRE_EXISTING_INDEXES: [&str; 6] = [
    "idx_kb_key_blocks_active_unique",
    "idx_kb_key_blocks_source_work_id",
    "idx_kb_key_blocks_world_canonical_name",
    "idx_kb_key_blocks_world_id",
    "idx_kb_key_blocks_world_status",
    "idx_kb_key_blocks_world_type",
];

/// Owner-scope indexes added by the migration under test.
const OWNER_INDEXES: [&str; 4] = [
    "idx_kb_key_blocks_actor_world_binding_id",
    "idx_kb_key_blocks_binding_active_unique",
    "idx_kb_key_blocks_character_active_unique",
    "idx_kb_key_blocks_character_id",
];

// ── Pool / migrator helpers ─────────────────────────────────────────────

/// Fresh file-backed pool with deterministic FK enforcement (single
/// connection, `PRAGMA foreign_keys = ON`).
async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("test.db").display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    (pool, dir)
}

/// Migrator containing every migration shipped *before* the owner-scope
/// migration — the pre-v1.184 schema state.
fn pre_upgrade_migrator() -> Migrator {
    let full = sqlx::migrate!("./migrations");
    let pre: Vec<Migration> = full
        .migrations
        .iter()
        .filter(|m| m.version < OWNER_MIGRATION_VERSION)
        .cloned()
        .collect();
    Migrator::with_migrations(pre)
}

/// Run a migrator on a single connection with foreign keys OFF.
///
/// Keep foreign keys disabled across the pre-upgrade migration batch so
/// table rebuilds do not cascade while legacy fixtures are being installed.
async fn run_migrator(pool: &SqlitePool, migrator: Migrator) {
    let mut conn = pool.acquire().await.unwrap();
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
    migrator.run_direct(None, &mut *conn, false).await.unwrap();
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);
}

/// Pool migrated to the full current schema (including the owner migration).
///
/// Uses the production admitted factory: post-upgrade fixture writes target
/// guarded tables, which a raw pool cannot mutate (the protocol scalar
/// functions are connection-local).
async fn migrated_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::init_pool(&dir.path().join("test.db"))
        .await
        .unwrap();
    (pool, dir)
}

// ── Seed helpers ────────────────────────────────────────────────────────

async fn seed_creator(pool: &SqlitePool) {
    nexus_local_db::ensure_creator_row(pool, CREATOR, "Owner")
        .await
        .unwrap();
}

async fn seed_world(pool: &SqlitePool, world_id: &str) {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', '2026-08-01T00:00:00Z')",
    )
    .bind(world_id)
    .bind(CREATOR)
    .bind(world_id)
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_character(pool: &SqlitePool, character_id: &str, display_name: &str) {
    sqlx::query(
        "INSERT INTO characters \
         (character_id, owner_creator_id, display_name, status, image_uri, persona_json, \
          created_at, updated_at) \
         VALUES (?, ?, ?, 'active', NULL, '{}', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
    )
    .bind(character_id)
    .bind(CREATOR)
    .bind(display_name)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_binding(
    pool: &SqlitePool,
    binding_id: &str,
    character_id: &str,
    world_id: &str,
    world_sheet_entry_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO actor_world_bindings \
         (binding_id, character_id, world_id, status, world_sheet_entry_id, \
          created_at, updated_at) \
         VALUES (?, ?, ?, 'active', ?, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
    )
    .bind(binding_id)
    .bind(character_id)
    .bind(world_id)
    .bind(world_sheet_entry_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a pre-v1.184 `kb_key_blocks` row through the 16 legacy columns
/// (the exact pre-upgrade shape: no owner columns exist yet).
#[allow(clippy::too_many_arguments)]
async fn seed_legacy_kb(
    pool: &SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    canonical_name: &str,
    status: &str,
    revision: Option<i64>,
    body_json: Option<&str>,
    source_anchor_json: Option<&str>,
    created_from_command_id: Option<&str>,
    created_at: &str,
    updated_at: Option<&str>,
    source_work_id: Option<&str>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<&str>,
    extensions_nexus_json: Option<&str>,
    modules_json: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, revision, \
          body_json, source_anchor_json, created_from_command_id, created_at, \
          updated_at, source_work_id, source_chapter, source_provenance_kind, \
          extensions_nexus_json, modules_json) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(block_type)
    .bind(canonical_name)
    .bind(status)
    .bind(revision)
    .bind(body_json)
    .bind(source_anchor_json)
    .bind(created_from_command_id)
    .bind(created_at)
    .bind(updated_at)
    .bind(source_work_id)
    .bind(source_chapter)
    .bind(source_provenance_kind)
    .bind(extensions_nexus_json)
    .bind(modules_json)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a post-migration row with explicit owner columns.
#[allow(clippy::too_many_arguments)] // raw column values — the full row shape
async fn insert_owned_kb(
    pool: &SqlitePool,
    key_block_id: &str,
    owner_kind: &str,
    world_id: Option<&str>,
    character_id: Option<&str>,
    binding_id: Option<&str>,
    holder_entry_id: Option<&str>,
    canonical_name: &str,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, world_id, character_id, actor_world_binding_id, \
          holder_entry_id, disclosure, block_type, canonical_name, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'character', ?, ?, '2026-09-05T00:00:00Z')",
    )
    .bind(key_block_id)
    .bind(owner_kind)
    .bind(world_id)
    .bind(character_id)
    .bind(binding_id)
    .bind(holder_entry_id)
    // A resolved holder implies `owner-private` (durable §3); unusual pairs are
    // exercised with direct SQL in the v1.191 governance cases below.
    .bind(holder_entry_id.map(|_| "owner-private"))
    .bind(canonical_name)
    .bind(status)
    .execute(pool)
    .await
    .map(|_| ())
}

// ── Dump / assertion helpers ────────────────────────────────────────────

/// Run a `json_array(...)` dump query, one canonical JSON string per row.
async fn dump(pool: &SqlitePool, sql: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Canonical dump of the 16 legacy `kb_key_blocks` columns.
const LEGACY_ROW_DUMP_SQL: &str = "SELECT json_array(\
       key_block_id, world_id, block_type, canonical_name, status, revision, \
       body_json, source_anchor_json, created_from_command_id, created_at, \
       updated_at, source_work_id, source_chapter, source_provenance_kind, \
       extensions_nexus_json, modules_json) \
     FROM kb_key_blocks ORDER BY key_block_id";

const ANCHOR_DUMP_SQL: &str =
    "SELECT json_array(key_block_id, anchor_ordinal, source_anchor_json, created_at) \
     FROM kb_source_anchors ORDER BY key_block_id, anchor_ordinal";

const RELATIONSHIP_DUMP_SQL: &str = "SELECT json_array(\
       relationship_id, world_id, source_entity_id, target_entity_id, relation_type, \
       custom_label, symmetric, confidence, source_anchor_ids, metadata, created_at, \
       updated_at, revision, needs_review, source, extensions_nexus_json) \
     FROM kb_relationships ORDER BY relationship_id";

const MIND_STATE_DUMP_SQL: &str = "SELECT json_array(\
       mind_state_id, schema_version, holder_entry_id, canonical_name, occurred_at, \
       sort_key, snapshot_json, deltas_json, source_anchor_json, created_at, \
       updated_at, extensions_json) \
     FROM mind_states ORDER BY mind_state_id";

const BINDING_DUMP_SQL: &str = "SELECT json_array(\
       binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, \
       updated_at) \
     FROM actor_world_bindings ORDER BY binding_id";

async fn index_names(pool: &SqlitePool) -> Vec<String> {
    // sqlite_autoindex_* (the implicit TEXT PRIMARY KEY unique index) is
    // internal, carries NULL sql, and is recreated by the rebuild itself;
    // the inventory covers only named, schema-owned indexes.
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master \
         WHERE type = 'index' AND tbl_name = 'kb_key_blocks' \
           AND name NOT LIKE 'sqlite_autoindex_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn table_sql(pool: &SqlitePool) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'kb_key_blocks'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Exact `sqlite_master` definitions for every schema object attached to
/// `kb_key_blocks` (the table row itself, its indexes, its triggers), sorted
/// for deterministic comparison. `sqlite_autoindex_*` entries carry NULL
/// `sql`, no schema-owned definition, and are excluded.
async fn kb_schema_objects(pool: &SqlitePool) -> Vec<(String, String, String)> {
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT type, name, sql FROM sqlite_master \
         WHERE tbl_name = 'kb_key_blocks' AND sql IS NOT NULL \
         ORDER BY type, name",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn count_where(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn expect_check_violation(result: Result<(), sqlx::Error>, case: &str) {
    let err = result.expect_err(&format!("{case} must be rejected"));
    assert!(
        err.to_string().contains("CHECK constraint failed"),
        "{case}: expected CHECK violation, got: {err}"
    );
}

fn expect_fk_violation(result: Result<(), sqlx::Error>, case: &str) {
    let err = result.expect_err(&format!("{case} must be rejected"));
    assert!(
        err.to_string().contains("FOREIGN KEY constraint failed"),
        "{case}: expected FK violation, got: {err}"
    );
}

fn expect_unique_violation(result: Result<(), sqlx::Error>, case: &str) {
    let err = result.expect_err(&format!("{case} must be rejected"));
    assert!(
        err.to_string().contains("UNIQUE constraint failed"),
        "{case}: expected UNIQUE violation, got: {err}"
    );
}

// ── Fixture upgrade fidelity ────────────────────────────────────────────

#[tokio::test]
#[allow(clippy::too_many_lines)] // single end-to-end byte-fidelity proof
async fn pre_v1184_upgrade_preserves_bytes_children_and_schema_objects() {
    let (pool, dir) = fresh_pool().await;
    run_migrator(&pool, pre_upgrade_migrator()).await;
    let db_path = dir.path().join("test.db");

    // Sanity: owner columns do not exist pre-upgrade.
    let column_names: Vec<String> =
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('kb_key_blocks')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(column_names.len(), 16, "pre-upgrade column count");
    assert!(!column_names.iter().any(|c| c == "owner_kind"));

    // ── Seed the pre-v1.184 fixture ─────────────────────────────────────
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_world(&pool, WORLD_CASCADE).await;
    seed_character(&pool, CHARACTER, "Aria").await;

    // Full-coverage row: every column populated, unicode payload bytes.
    seed_legacy_kb(
        &pool,
        "kb_full_0001",
        WORLD_A,
        "character",
        "Aria Full",
        "confirmed",
        Some(3),
        Some("{\"summary\":\"Résumé ünïcode ⚙\",\"tags\":[\"α\",\"β\"]}"),
        Some("{\"kind\":\"chapter\",\"work\":\"we_work1\",\"chapter\":2}"),
        Some("cmd_abc123"),
        "2026-08-01T10:00:00Z",
        Some("2026-08-02T11:11:11Z"),
        Some("we_work1"),
        Some(2),
        Some("pack_import"),
        Some("{\"world_id\":\"wld_ownerA\",\"custom_unknown\":{\"x\":1}}"),
        Some("{\"activation\":{\"mode\":\"always\"},\"pack\":{\"id\":7}}"),
    )
    .await;
    // Minimal row: every nullable column NULL.
    seed_legacy_kb(
        &pool,
        "kb_nulls_001",
        WORLD_A,
        "organization",
        "Null Fields",
        "provisional",
        None,
        None,
        None,
        None,
        "2026-08-03T00:00:00Z",
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    // Terminal statuses ride the partial-unique predicate.
    for (id, status) in [
        ("kb_deprecated1", "deprecated"),
        ("kb_merged_001", "merged"),
        ("kb_deleted_01", "deleted"),
    ] {
        seed_legacy_kb(
            &pool,
            id,
            WORLD_A,
            "conflict",
            &format!("Terminal {status}"),
            status,
            Some(0),
            Some("{}"),
            None,
            None,
            "2026-08-04T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    }
    // WorldSheet row referenced by the P0 binding link.
    seed_legacy_kb(
        &pool,
        "kb_sheet_0001",
        WORLD_A,
        "character",
        "Aria Sheet",
        "confirmed",
        Some(1),
        Some("{}"),
        None,
        None,
        "2026-08-05T00:00:00Z",
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    // Relationship endpoints + MindState holder (world A).
    for id in ["kb_rel_src_01", "kb_rel_tgt_01", "kb_mindholder"] {
        seed_legacy_kb(
            &pool,
            id,
            WORLD_A,
            "species",
            &format!("Society {id}"),
            "confirmed",
            None,
            Some("{}"),
            None,
            None,
            "2026-08-06T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    }
    // Cascade world rows (deleted post-upgrade to prove inbound FK actions).
    for id in ["kb_casc_0001", "kb_casc_0002"] {
        seed_legacy_kb(
            &pool,
            id,
            WORLD_CASCADE,
            "scene",
            &format!("Cascade {id}"),
            "confirmed",
            None,
            Some("{}"),
            None,
            None,
            "2026-08-07T00:00:00Z",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    }

    // Inbound children.
    for (kb, ordinal) in [
        ("kb_full_0001", 0),
        ("kb_full_0001", 1),
        ("kb_casc_0001", 0),
    ] {
        sqlx::query(
            "INSERT INTO kb_source_anchors \
             (key_block_id, anchor_ordinal, source_anchor_json, created_at) \
             VALUES (?, ?, ?, '2026-08-08T00:00:00Z')",
        )
        .bind(kb)
        .bind(ordinal)
        .bind(format!("{{\"ordinal\":{ordinal}}}"))
        .execute(&pool)
        .await
        .unwrap();
    }
    for (rel_id, world, src, tgt) in [
        ("rel_world_a", WORLD_A, "kb_rel_src_01", "kb_rel_tgt_01"),
        ("rel_cascade", WORLD_CASCADE, "kb_casc_0001", "kb_casc_0002"),
    ] {
        sqlx::query(
            "INSERT INTO kb_relationships \
             (relationship_id, world_id, source_entity_id, target_entity_id, \
              relation_type, custom_label, symmetric, confidence, source_anchor_ids, \
              metadata, created_at, updated_at, revision, needs_review, source, \
              extensions_nexus_json) \
             VALUES (?, ?, ?, ?, 'ally_of', 'sworn', 1, 0.75, '[\"a1\"]', '{\"m\":1}', \
              '2026-08-09T00:00:00Z', '2026-08-09T01:00:00Z', 2, 1, 'extraction', \
              '{\"unknown_rel_key\":true}')",
        )
        .bind(rel_id)
        .bind(world)
        .bind(src)
        .bind(tgt)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (ms_id, holder) in [
        ("ms_holder_a", "kb_mindholder"),
        ("ms_cascade", "kb_casc_0002"),
    ] {
        sqlx::query(
            "INSERT INTO mind_states \
             (mind_state_id, schema_version, holder_entry_id, canonical_name, occurred_at, \
              sort_key, snapshot_json, deltas_json, source_anchor_json, created_at, \
              updated_at, extensions_json) \
             VALUES (?, 1, ?, 'holder', '2026-08-10T00:00:00Z', 'k1', '{\"s\":1}', \
              '{\"d\":1}', NULL, '2026-08-10T00:00:00Z', '2026-08-10T01:00:00Z', \
              '{\"e\":1}')",
        )
        .bind(ms_id)
        .bind(holder)
        .execute(&pool)
        .await
        .unwrap();
    }
    seed_binding(&pool, BINDING, CHARACTER, WORLD_A, Some("kb_sheet_0001")).await;

    // ── Capture pre-upgrade state ───────────────────────────────────────
    let pre_rows = dump(&pool, LEGACY_ROW_DUMP_SQL).await;
    let pre_anchors = dump(&pool, ANCHOR_DUMP_SQL).await;
    let pre_relationships = dump(&pool, RELATIONSHIP_DUMP_SQL).await;
    let pre_mind_states = dump(&pool, MIND_STATE_DUMP_SQL).await;
    let pre_bindings = dump(&pool, BINDING_DUMP_SQL).await;
    let pre_indexes = index_names(&pool).await;
    assert_eq!(
        pre_indexes,
        PRE_EXISTING_INDEXES.to_vec(),
        "pre-upgrade index inventory"
    );
    assert_eq!(pre_rows.len(), 11, "fixture row count");

    // ── Apply the owner-scope migration through the real production path ─
    // `run_migrations` scopes an FK-off window to the rebuild migration so
    // the DROP/recreate does not cascade into child tables. The post-upgrade
    // phase then continues on the admitted factory, because the remaining
    // fixture mutations (cascade delete) target guarded tables.
    drop(pool);
    let pool = nexus_local_db::init_pool(&db_path).await.unwrap();

    // ── Byte/identity fidelity of legacy rows ───────────────────────────
    let post_rows = dump(&pool, LEGACY_ROW_DUMP_SQL).await;
    assert_eq!(
        post_rows, pre_rows,
        "every legacy column/payload byte must survive the rebuild"
    );
    assert_eq!(dump(&pool, ANCHOR_DUMP_SQL).await, pre_anchors);
    assert_eq!(dump(&pool, RELATIONSHIP_DUMP_SQL).await, pre_relationships);
    assert_eq!(dump(&pool, MIND_STATE_DUMP_SQL).await, pre_mind_states);
    assert_eq!(dump(&pool, BINDING_DUMP_SQL).await, pre_bindings);

    // ── Owner defaults: World-owned, shared governance, null other owners ─
    let owner_dump = dump(
        &pool,
        "SELECT json_array(owner_kind, character_id, actor_world_binding_id, \
                           holder_entry_id, disclosure) \
         FROM kb_key_blocks ORDER BY key_block_id",
    )
    .await;
    assert_eq!(owner_dump.len(), pre_rows.len());
    for row in &owner_dump {
        assert_eq!(
            row, "[\"world\",null,null,null,null]",
            "legacy shared rows become World-owned with no governance"
        );
    }

    // ── Schema objects restored ─────────────────────────────────────────
    let post_indexes = index_names(&pool).await;
    for name in PRE_EXISTING_INDEXES {
        assert!(
            post_indexes.contains(&name.to_string()),
            "missing index {name}"
        );
    }
    for name in OWNER_INDEXES {
        assert!(
            post_indexes.contains(&name.to_string()),
            "missing index {name}"
        );
    }
    // All three active-uniqueness indexes keep the status predicate.
    let partial_count = count_where(
        &pool,
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' \
         AND tbl_name = 'kb_key_blocks' AND name LIKE 'idx_kb_key_blocks_%active_unique' \
         AND sql LIKE '%status NOT IN%'",
    )
    .await;
    assert_eq!(
        partial_count, 3,
        "three owner-scoped partial unique indexes"
    );

    let ddl = table_sql(&pool).await;
    for fragment in [
        "owner_kind",
        "character_id",
        "actor_world_binding_id",
        "holder_entry_id",
        "disclosure",
        "REFERENCES knowledge_holders",
        "REFERENCES characters",
        "REFERENCES actor_world_bindings",
        "REFERENCES narrative_worlds",
        "CHECK",
    ] {
        assert!(ddl.contains(fragment), "rebuilt DDL missing: {fragment}");
    }
    assert!(
        !ddl.contains("creator_only"),
        "the legacy creator_only column must not survive the cutover"
    );

    // ── foreign_key_check is empty after rebuild ────────────────────────
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        violations.is_empty(),
        "foreign_key_check returned {} violation(s)",
        violations.len()
    );

    // ── Legacy World store behavior on the upgraded database ────────────
    let store = SqliteKbStore::new(pool.clone());
    let kb = KnowledgeEntryRecord::new(WORLD_A, BlockType::Character, "Post Upgrade Hero");
    let inserted = store.insert_knowledge_entry(kb.clone()).await.unwrap();
    assert_eq!(inserted.entry_id, kb.entry_id);
    let fetched = store.get_knowledge_entry(&kb.entry_id).await.unwrap();
    assert_eq!(fetched.world_id(), Some(WORLD_A));
    assert_eq!(fetched.canonical_name, "Post Upgrade Hero");
    // New store inserts land as World-owned shared rows.
    let new_owner: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT owner_kind, holder_entry_id, disclosure \
         FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(&kb.entry_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(new_owner, ("world".to_string(), None, None));
    // World listing still sees the world rows (9 active in world A + new one).
    let listed = store.list_by_world(WORLD_A).await.unwrap();
    assert!(
        listed.iter().any(|e| e.entry_id == kb.entry_id),
        "list_by_world must include the new entry"
    );
    // Legacy duplicate mapping intact (2067 → KbStoreError::Duplicate).
    let dup = store
        .insert_knowledge_entry(KnowledgeEntryRecord::new(
            WORLD_A,
            BlockType::Character,
            "Post Upgrade Hero",
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(dup, KbStoreError::Duplicate { .. }),
        "expected Duplicate, got: {dup:?}"
    );

    // ── Inbound FK actions survive the rebuild ──────────────────────────
    // SET NULL: deleting the WorldSheet row clears the binding link.
    sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = 'kb_sheet_0001'")
        .execute(&pool)
        .await
        .unwrap();
    let sheet_link: Option<String> = sqlx::query_scalar(
        "SELECT world_sheet_entry_id FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(BINDING)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        sheet_link, None,
        "ON DELETE SET NULL must fire post-rebuild"
    );

    // CASCADE: deleting the cascade world removes its KEs and their children.
    sqlx::query("DELETE FROM narrative_worlds WHERE world_id = ?")
        .bind(WORLD_CASCADE)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM kb_key_blocks WHERE world_id = 'wld_ownerC'"
        )
        .await,
        0,
        "world delete must cascade to kb_key_blocks"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM kb_source_anchors WHERE key_block_id = 'kb_casc_0001'"
        )
        .await,
        0,
        "cascade must reach kb_source_anchors"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM mind_states WHERE holder_entry_id = 'kb_casc_0002'"
        )
        .await,
        0,
        "cascade must reach mind_states"
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM kb_relationships WHERE world_id = 'wld_ownerC'"
        )
        .await,
        0,
        "cascade must reach kb_relationships"
    );
}

/// Minor (task-1-review): capture exact pre-upgrade `sqlite_master` SQL for
/// the table, every associated index, and every associated trigger; prove
/// legacy index SQL survives the rebuild byte-for-byte and the upgraded
/// schema converges exactly to a fresh install. Only migration-owned schema
/// objects are supported — the pre-upgrade inventory is pinned so a deployed
/// custom index/trigger would fail loudly here instead of being dropped
/// silently by the DROP/recreate strategy.
#[tokio::test]
async fn upgrade_preserves_exact_sqlite_master_definitions() {
    let (pool, _dir) = fresh_pool().await;
    run_migrator(&pool, pre_upgrade_migrator()).await;

    // ── Pin the pre-upgrade inventory: exactly the table plus the six known
    //    legacy indexes, and no table-attached triggers. ──────────────────
    let pre_objects = kb_schema_objects(&pool).await;
    let pre_tables = pre_objects
        .iter()
        .filter(|(ty, _, _)| ty == "table")
        .count();
    assert_eq!(pre_tables, 1, "exactly one kb_key_blocks table row");
    let pre_index_names: Vec<&str> = pre_objects
        .iter()
        .filter(|(ty, _, _)| ty == "index")
        .map(|(_, name, _)| name.as_str())
        .collect();
    let mut expected_indexes = PRE_EXISTING_INDEXES.to_vec();
    expected_indexes.sort_unstable();
    assert_eq!(
        pre_index_names, expected_indexes,
        "pre-upgrade index inventory"
    );
    assert!(
        !pre_objects.iter().any(|(ty, _, _)| ty == "trigger"),
        "no table-attached triggers are part of the supported inventory"
    );

    // ── Upgrade through the real production path ─────────────────────────
    nexus_local_db::run_migrations(&pool).await.unwrap();
    let upgraded_objects = kb_schema_objects(&pool).await;

    // ── Every pre-existing index survives with byte-identical SQL ────────
    for (ty, name, sql) in pre_objects.iter().filter(|(ty, _, _)| ty == "index") {
        debug_assert_eq!(ty, "index");
        let post = upgraded_objects
            .iter()
            .find(|(_, post_name, _)| post_name == name)
            .unwrap_or_else(|| panic!("index {name} lost in rebuild"));
        assert_eq!(&post.2, sql, "index {name} SQL changed across the rebuild");
    }

    // ── The upgraded schema is byte-identical to a fresh install ─────────
    let (fresh_pool, _fresh_dir) = migrated_pool().await;
    let fresh_objects = kb_schema_objects(&fresh_pool).await;
    assert_eq!(
        upgraded_objects, fresh_objects,
        "upgraded schema must converge exactly to the fresh-install schema"
    );
}

// ── Owner CHECK constraints ─────────────────────────────────────────────

// CHECK-constraint matrix: sequential insert/expect cases; splitting would scatter one contract.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn exactly_one_owner_check_rejects_invalid_combinations() {
    let (pool, _dir) = migrated_pool().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER, "Aria").await;
    seed_binding(&pool, BINDING, CHARACTER, WORLD_A, None).await;

    // Valid baselines: exactly one owner column per kind.
    insert_owned_kb(
        &pool,
        "kb_ok_world",
        "world",
        Some(WORLD_A),
        None,
        None,
        None,
        "Ok World",
        "confirmed",
    )
    .await
    .unwrap();
    insert_owned_kb(
        &pool,
        "kb_ok_char",
        "character",
        None,
        Some(CHARACTER),
        None,
        None,
        "Ok Char",
        "confirmed",
    )
    .await
    .unwrap();
    insert_owned_kb(
        &pool,
        "kb_ok_bind",
        "actor_world_binding",
        None,
        None,
        Some(BINDING),
        None,
        "Ok Bind",
        "confirmed",
    )
    .await
    .unwrap();

    // World owner without world_id.
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_01",
            "world",
            None,
            None,
            None,
            None,
            "Bad 01",
            "confirmed",
        )
        .await,
        "world owner without world_id",
    );
    // World owner with a second owner column set.
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_02",
            "world",
            Some(WORLD_A),
            Some(CHARACTER),
            None,
            None,
            "Bad 02",
            "confirmed",
        )
        .await,
        "world owner with character_id",
    );
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_03",
            "world",
            Some(WORLD_A),
            None,
            Some(BINDING),
            None,
            "Bad 03",
            "confirmed",
        )
        .await,
        "world owner with binding id",
    );
    // Character owner with world_id or without character_id.
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_04",
            "character",
            Some(WORLD_A),
            Some(CHARACTER),
            None,
            None,
            "Bad 04",
            "confirmed",
        )
        .await,
        "character owner with world_id",
    );
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_05",
            "character",
            None,
            None,
            None,
            None,
            "Bad 05",
            "confirmed",
        )
        .await,
        "character owner without character_id",
    );
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_06",
            "character",
            None,
            Some(CHARACTER),
            Some(BINDING),
            None,
            "Bad 06",
            "confirmed",
        )
        .await,
        "character owner with binding id",
    );
    // Binding owner without binding id / with world_id.
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_07",
            "actor_world_binding",
            None,
            None,
            None,
            None,
            "Bad 07",
            "confirmed",
        )
        .await,
        "binding owner without binding id",
    );
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_08",
            "actor_world_binding",
            Some(WORLD_A),
            None,
            Some(BINDING),
            None,
            "Bad 08",
            "confirmed",
        )
        .await,
        "binding owner with world_id",
    );
    // Unknown owner kind.
    expect_check_violation(
        insert_owned_kb(
            &pool,
            "kb_bad_09",
            "creator",
            None,
            None,
            None,
            None,
            "Bad 09",
            "confirmed",
        )
        .await,
        "unknown owner_kind",
    );
}

/// v1.191 P1 T3: the native governance columns carry the disclosure policy.
///
/// Shared is the absence of both columns; `owner-private` requires a registered
/// holder; the pair is FK-bound to the registry with `ON DELETE RESTRICT`; the
/// retired `creator_only` World-only rule is not reimposed on the native shape.
#[allow(clippy::too_many_lines)] // one governance matrix on one schema
#[tokio::test]
async fn v1191_holder_migration_governance_columns() {
    let (pool, _dir) = migrated_pool().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER, "Aria").await;
    seed_binding(&pool, BINDING, CHARACTER, WORLD_A, None).await;

    let creator_holder = {
        let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
        let id = nexus_local_db::ensure_creator_holder_in_tx(&mut tx, CREATOR)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        id
    };
    let character_holder = {
        let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
        let id = nexus_local_db::ensure_character_holder_in_tx(&mut tx, CHARACTER)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        id
    };
    assert_eq!(
        creator_holder,
        nexus_local_db::creator_holder_entry_id(CREATOR)
    );
    assert_eq!(
        character_holder,
        nexus_local_db::character_holder_entry_id(CHARACTER)
    );

    // Shared (both columns NULL) on a World owner.
    insert_owned_kb(
        &pool,
        "kb_gov_shared",
        "world",
        Some(WORLD_A),
        None,
        None,
        None,
        "Shared Lore",
        "confirmed",
    )
    .await
    .unwrap();
    // Owner-private under the World Creator's holder.
    insert_owned_kb(
        &pool,
        "kb_gov_private",
        "world",
        Some(WORLD_A),
        None,
        None,
        Some(&creator_holder),
        "Private Lore",
        "confirmed",
    )
    .await
    .unwrap();
    // A Character-owned private row is legal natively: governance is not
    // re-constrained to the (retired) legacy World-only rule.
    insert_owned_kb(
        &pool,
        "kb_gov_char",
        "character",
        None,
        Some(CHARACTER),
        None,
        Some(&character_holder),
        "Character Private",
        "confirmed",
    )
    .await
    .unwrap();

    let stored: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT key_block_id, holder_entry_id, disclosure FROM kb_key_blocks \
         WHERE key_block_id LIKE 'kb_gov_%' ORDER BY key_block_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        vec![
            (
                "kb_gov_char".to_string(),
                Some(character_holder.clone()),
                Some("owner-private".to_string())
            ),
            (
                "kb_gov_private".to_string(),
                Some(creator_holder.clone()),
                Some("owner-private".to_string())
            ),
            ("kb_gov_shared".to_string(), None, None),
        ]
    );

    // A disclosure without a holder is not a state the schema can hold.
    expect_check_violation(
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, disclosure) \
             VALUES ('kb_gov_orphan', ?, 'character', 'Orphan', 'confirmed', 'owner-private')",
        )
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .map(|_| ()),
        "owner-private without a holder",
    );
    // Empty strings are invalid.
    expect_check_violation(
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, holder_entry_id) \
             VALUES ('kb_gov_empty', ?, 'character', 'Empty', 'confirmed', '')",
        )
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .map(|_| ()),
        "empty holder_entry_id",
    );
    // Unknown vocabulary stays out of the native table (it is quarantined at
    // the import boundary, never stored here).
    expect_check_violation(
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, holder_entry_id, disclosure) \
             VALUES ('kb_gov_unknown', ?, 'character', 'Unknown', 'confirmed', ?, 'owner-public')",
        )
        .bind(WORLD_A)
        .bind(&creator_holder)
        .execute(&pool)
        .await
        .map(|_| ()),
        "unknown disclosure vocabulary",
    );
    // The holder must exist in the registry (FK, native rows only).
    expect_fk_violation(
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, holder_entry_id, disclosure) \
             VALUES ('kb_gov_fk', ?, 'character', 'Dangling', 'confirmed', 'hld_unregistered', 'owner-private')",
        )
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .map(|_| ()),
        "holder referencing an unregistered registry row",
    );
    // A governed row pins its holder: the registry row cannot be deleted out
    // from under it (ON DELETE RESTRICT).
    expect_fk_violation(
        sqlx::query("DELETE FROM knowledge_holders WHERE holder_entry_id = ?")
            .bind(&creator_holder)
            .execute(&pool)
            .await
            .map(|_| ()),
        "delete a holder referenced by a private row",
    );
    // And the subject pin is one-to-one and one-directional.
    expect_unique_violation(
        sqlx::query(
            "INSERT INTO knowledge_holders (holder_entry_id, creator_id) VALUES ('hld_second', ?)",
        )
        .bind(CREATOR)
        .execute(&pool)
        .await
        .map(|_| ()),
        "second holder for the same Creator subject",
    );
}

// ── Owner FK actions ────────────────────────────────────────────────────

#[tokio::test]
async fn owner_foreign_keys_enforced_with_actions() {
    let (pool, _dir) = migrated_pool().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER, "Aria").await;
    seed_binding(&pool, BINDING, CHARACTER, WORLD_A, None).await;

    // Dangling owner references reject.
    expect_fk_violation(
        insert_owned_kb(
            &pool,
            "kb_fk_char",
            "character",
            None,
            Some("chr_deadbeefdeadbeefdeadbeefdeadbeef"),
            None,
            None,
            "Dangling Char",
            "confirmed",
        )
        .await,
        "character_id referencing missing character",
    );
    expect_fk_violation(
        insert_owned_kb(
            &pool,
            "kb_fk_bind",
            "actor_world_binding",
            None,
            None,
            Some("awb_deadbeefdeadbeefdeadbeefdeadbeef"),
            None,
            "Dangling Bind",
            "confirmed",
        )
        .await,
        "binding id referencing missing binding",
    );

    // RESTRICT: a Character with owned KE cannot be deleted.
    insert_owned_kb(
        &pool,
        "kb_own_char",
        "character",
        None,
        Some(CHARACTER),
        None,
        None,
        "Owned Char",
        "confirmed",
    )
    .await
    .unwrap();
    let delete_character = sqlx::query("DELETE FROM characters WHERE character_id = ?")
        .bind(CHARACTER)
        .execute(&pool)
        .await
        .map(|_| ());
    expect_fk_violation(delete_character, "delete character owning KE (RESTRICT)");

    // RESTRICT: a binding with owned KE cannot be deleted.
    insert_owned_kb(
        &pool,
        "kb_own_bind",
        "actor_world_binding",
        None,
        None,
        Some(BINDING),
        None,
        "Owned Bind",
        "confirmed",
    )
    .await
    .unwrap();
    let delete_binding = sqlx::query("DELETE FROM actor_world_bindings WHERE binding_id = ?")
        .bind(BINDING)
        .execute(&pool)
        .await
        .map(|_| ());
    expect_fk_violation(delete_binding, "delete binding owning KE (RESTRICT)");
}

// ── Owner-scoped active uniqueness ──────────────────────────────────────

// uniqueness matrix: sequential cases per owner scope; splitting would scatter one contract.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn owner_scoped_active_uniqueness_enforced() {
    let (pool, _dir) = migrated_pool().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER, "Aria").await;
    seed_character(&pool, CHARACTER_B, "Beatrice").await;
    seed_binding(&pool, BINDING, CHARACTER, WORLD_A, None).await;

    // Character scope: same (character, type, name) active pair rejects.
    insert_owned_kb(
        &pool,
        "kb_u_char_a",
        "character",
        None,
        Some(CHARACTER),
        None,
        None,
        "Shared Name",
        "confirmed",
    )
    .await
    .unwrap();
    expect_unique_violation(
        insert_owned_kb(
            &pool,
            "kb_u_char_b",
            "character",
            None,
            Some(CHARACTER),
            None,
            None,
            "Shared Name",
            "confirmed",
        )
        .await,
        "duplicate active character-owned name",
    );
    // Same name under a different Character is a different scope.
    insert_owned_kb(
        &pool,
        "kb_u_char_c",
        "character",
        None,
        Some(CHARACTER_B),
        None,
        None,
        "Shared Name",
        "confirmed",
    )
    .await
    .unwrap();
    // Same name in World and binding scopes does not collide across scopes.
    insert_owned_kb(
        &pool,
        "kb_u_world",
        "world",
        Some(WORLD_A),
        None,
        None,
        None,
        "Shared Name",
        "confirmed",
    )
    .await
    .unwrap();
    insert_owned_kb(
        &pool,
        "kb_u_bind",
        "actor_world_binding",
        None,
        None,
        Some(BINDING),
        None,
        "Shared Name",
        "confirmed",
    )
    .await
    .unwrap();
    expect_unique_violation(
        insert_owned_kb(
            &pool,
            "kb_u_bind2",
            "actor_world_binding",
            None,
            None,
            Some(BINDING),
            None,
            "Shared Name",
            "confirmed",
        )
        .await,
        "duplicate active binding-owned name",
    );
    expect_unique_violation(
        insert_owned_kb(
            &pool,
            "kb_u_world2",
            "world",
            Some(WORLD_A),
            None,
            None,
            None,
            "Shared Name",
            "confirmed",
        )
        .await,
        "duplicate active world-owned name (legacy invariant)",
    );
    // Terminal statuses drop out of the partial predicate: a deleted row no
    // longer blocks a replacement in the same owner scope.
    insert_owned_kb(
        &pool,
        "kb_u_term",
        "character",
        None,
        Some(CHARACTER),
        None,
        None,
        "Terminal Name",
        "deleted",
    )
    .await
    .unwrap();
    insert_owned_kb(
        &pool,
        "kb_u_term2",
        "character",
        None,
        Some(CHARACTER),
        None,
        None,
        "Terminal Name",
        "confirmed",
    )
    .await
    .unwrap();
}

// ── Legacy insert shapes ────────────────────────────────────────────────

#[tokio::test]
async fn legacy_insert_without_owner_columns_defaults_to_world_owner() {
    let (pool, _dir) = migrated_pool().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;

    // The 5-column shape used by seed helpers and daemon test fixtures.
    sqlx::query(
        "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status) \
         VALUES ('kb_legacy_5c', ?, 'character', 'Legacy Five', 'confirmed')",
    )
    .bind(WORLD_A)
    .execute(&pool)
    .await
    .unwrap();

    let row: (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT owner_kind, character_id, actor_world_binding_id, \
                    holder_entry_id, disclosure \
             FROM kb_key_blocks WHERE key_block_id = 'kb_legacy_5c'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("world".to_string(), None, None, None, None));
}

// ── v1.191 P1 T3: holder registry and the offline governance cutover ────────

const CREATOR_B: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD_B: &str = "wld_ownerB";
const CHARACTER_ARCHIVED: &str = "chr_cccccccccccccccccccccccccccccccc";

/// Canonical dump of the pre-cutover `kb_key_blocks` columns that must survive
/// the rebuild verbatim. Extensions are asserted separately: the retired legacy
/// `creator_only` key is removed while unknown keys stay.
const PRE_CUTOVER_COLUMNS_DUMP_SQL: &str = "SELECT json_array(\
       key_block_id, owner_kind, world_id, character_id, actor_world_binding_id, \
       block_type, canonical_name, status, revision, body_json, source_anchor_json, \
       created_from_command_id, created_at, updated_at, source_work_id, source_chapter, \
       source_provenance_kind, modules_json) \
     FROM kb_key_blocks ORDER BY key_block_id";

/// Version of the core writer-protocol migration (`20260912000001`). Every
/// migration before it is seeded raw; it and everything after run through the
/// production runner.
const CORE_WRITER_PROTOCOL_VERSION: i64 = 20_260_912_000_001;

/// Migrator containing every migration shipped before the writer guards — the
/// state a legacy workspace can still be seeded from directly (no guarded
/// tables, no protocol scalar functions on the fixture connection).
fn pre_writer_protocol_migrator() -> Migrator {
    let full = sqlx::migrate!("./migrations");
    let pre: Vec<Migration> = full
        .migrations
        .iter()
        .filter(|m| m.version < CORE_WRITER_PROTOCOL_VERSION)
        .cloned()
        .collect();
    Migrator::with_migrations(pre)
}

/// A pre-cutover workspace: the writer protocol and the v1.191 holder migration
/// are both still pending.
///
/// The fixture pool is raw (no writer admission, no protocol scalar functions)
/// and keeps foreign keys **off** after the pre-migration batch, so the invalid
/// subject references the preflight must refuse can be installed at all — the
/// same state a hand-repaired or legacy workspace can present. The pending
/// migrations are then applied by the production runner (`run_migrations`).
async fn pre_holder_db() -> (SqlitePool, tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    run_migrator(&pool, pre_writer_protocol_migrator()).await;
    // SAFETY: PRAGMA statement — the fixture models pre-cutover state.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    (pool, dir, db_path)
}

/// Insert a pre-cutover `kb_key_blocks` row (the v1.184 shape, `creator_only`
/// included).
async fn seed_pre_cutover_kb(
    pool: &SqlitePool,
    key_block_id: &str,
    canonical_name: &str,
    world_id: Option<&str>,
    character_id: Option<&str>,
    creator_only: i64,
    extensions_nexus_json: Option<&str>,
) {
    let owner_kind = if world_id.is_some() {
        "world"
    } else {
        "character"
    };
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, world_id, character_id, creator_only, block_type, \
          canonical_name, status, extensions_nexus_json, created_at) \
         VALUES (?, ?, ?, ?, ?, 'character', ?, 'confirmed', ?, '2026-09-01T00:00:00Z')",
    )
    .bind(key_block_id)
    .bind(owner_kind)
    .bind(world_id)
    .bind(character_id)
    .bind(creator_only)
    .bind(canonical_name)
    .bind(extensions_nexus_json)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a minimal Creator row with an explicit id (the shared `seed_creator`
/// helper is fixed to `CREATOR`).
async fn seed_creator_row(pool: &SqlitePool, creator_id: &str) {
    sqlx::query(
        "INSERT INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES (?, ?, 'active', '2026-09-01T00:00:00Z', '{}')",
    )
    .bind(creator_id)
    .bind(creator_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a World whose stored controlling Creator is `owner_creator_id`.
async fn seed_world_owned_by(pool: &SqlitePool, world_id: &str, owner_creator_id: &str) {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', '2026-09-01T00:00:00Z')",
    )
    .bind(world_id)
    .bind(owner_creator_id)
    .bind(world_id)
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a Character row with an explicit status (archived identities are
/// backfilled too).
async fn seed_character_with_status(pool: &SqlitePool, character_id: &str, status: &str) {
    sqlx::query(
        "INSERT INTO characters \
         (character_id, owner_creator_id, display_name, status, persona_json, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, '{}', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
    )
    .bind(character_id)
    .bind(CREATOR)
    .bind(character_id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn column_names(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(format!(
        "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
    )))
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn table_exists(pool: &SqlitePool, table: &str) -> bool {
    count_where(
        pool,
        &format!("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '{table}'"),
    )
    .await
        == 1
}

/// The aborted cutover must leave the workspace exactly as it was: no registry,
/// no quarantine, no governance columns, no revisions and no bookkeeping row.
async fn assert_holder_migration_aborted(pool: &SqlitePool, case: &str) {
    assert!(
        !table_exists(pool, "knowledge_holders").await,
        "{case}: the registry must not survive the abort"
    );
    assert!(
        !table_exists(pool, "knowledge_import_quarantine").await,
        "{case}: the quarantine table must not survive the abort"
    );
    let kb_columns = column_names(pool, "kb_key_blocks").await;
    assert!(
        kb_columns.iter().any(|c| c == "creator_only"),
        "{case}: the legacy column must still be present"
    );
    assert!(
        !kb_columns.iter().any(|c| c == "holder_entry_id"),
        "{case}: governance columns must not be added"
    );
    assert!(
        !column_names(pool, "narrative_worlds")
            .await
            .iter()
            .any(|c| c == "knowledge_revision"),
        "{case}: knowledge_revision must not be added"
    );
    assert!(
        !column_names(pool, "characters")
            .await
            .iter()
            .any(|c| c == "knowledge_revision"),
        "{case}: knowledge_revision must not be added"
    );
    let applied = count_where(
        pool,
        &format!(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = {}",
            nexus_local_db::HOLDER_MIGRATION_VERSION
        ),
    )
    .await;
    assert_eq!(applied, 0, "{case}: the migration must not be recorded");
    assert!(
        count_where(pool, "SELECT COUNT(*) FROM kb_key_blocks").await > 0,
        "{case}: fixture rows must remain"
    );
}

/// Backfill equivalence: every `creator_only = true` row becomes `owner-private`
/// under the holder of its **stored World controlling Creator** (never the
/// active shell Creator), shared rows stay NULL/NULL, and the legacy key leaves
/// the extension document while unknown keys stay.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn v1191_holder_migration_backfills_stored_creator_private() {
    let (pool, _dir, db_path) = pre_holder_db().await;
    seed_creator(&pool).await;
    seed_creator_row(&pool, CREATOR_B).await;
    seed_world(&pool, WORLD_A).await;
    seed_world_owned_by(&pool, WORLD_B, CREATOR_B).await;
    seed_character(&pool, CHARACTER, "Aria").await;
    seed_character_with_status(&pool, CHARACTER_ARCHIVED, "archived").await;

    seed_pre_cutover_kb(
        &pool,
        "kb_priv_a",
        "Private A",
        Some(WORLD_A),
        None,
        1,
        None,
    )
    .await;
    seed_pre_cutover_kb(
        &pool,
        "kb_priv_b",
        "Private B",
        Some(WORLD_B),
        None,
        1,
        None,
    )
    .await;
    seed_pre_cutover_kb(
        &pool,
        "kb_shared_a",
        "Shared A",
        Some(WORLD_A),
        None,
        0,
        Some("{\"world_id\":\"wld_ownerA\",\"creator_only\":false,\"custom_unknown\":{\"x\":1}}"),
    )
    .await;
    seed_pre_cutover_kb(
        &pool,
        "kb_shared_chr",
        "Shared Char",
        None,
        Some(CHARACTER),
        0,
        None,
    )
    .await;

    let pre_columns = dump(&pool, PRE_CUTOVER_COLUMNS_DUMP_SQL).await;
    assert_eq!(pre_columns.len(), 4, "fixture rows");
    let pre_private = count_where(
        &pool,
        "SELECT COUNT(*) FROM kb_key_blocks WHERE creator_only = 1",
    )
    .await;
    assert_eq!(pre_private, 2);

    // ── The cutover runs through the production migration transaction ────
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("holder migration applies");

    // Registry: one holder per stored subject, archived Character included.
    let mut registry: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT holder_entry_id, creator_id, character_id FROM knowledge_holders")
            .fetch_all(&pool)
            .await
            .unwrap();
    let mut expected = vec![
        (
            nexus_local_db::creator_holder_entry_id(CREATOR),
            Some(CREATOR.to_string()),
            None,
        ),
        (
            nexus_local_db::creator_holder_entry_id(CREATOR_B),
            Some(CREATOR_B.to_string()),
            None,
        ),
        (
            nexus_local_db::character_holder_entry_id(CHARACTER),
            None,
            Some(CHARACTER.to_string()),
        ),
        (
            nexus_local_db::character_holder_entry_id(CHARACTER_ARCHIVED),
            None,
            Some(CHARACTER_ARCHIVED.to_string()),
        ),
    ];
    registry.sort();
    expected.sort();
    assert_eq!(registry, expected, "registry backfill");

    // Governance: true → stored-Creator owner-private; shared unchanged.
    let governance: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT key_block_id, holder_entry_id, disclosure FROM kb_key_blocks \
         ORDER BY key_block_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        governance,
        vec![
            (
                "kb_priv_a".to_string(),
                Some(nexus_local_db::creator_holder_entry_id(CREATOR)),
                Some("owner-private".to_string())
            ),
            (
                "kb_priv_b".to_string(),
                Some(nexus_local_db::creator_holder_entry_id(CREATOR_B)),
                Some("owner-private".to_string())
            ),
            ("kb_shared_a".to_string(), None, None),
            ("kb_shared_chr".to_string(), None, None),
        ],
        "stored World controlling Creator, not the shell Creator"
    );

    // Visibility equivalence: exactly the legacy private/shared split.
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM kb_key_blocks WHERE disclosure = 'owner-private'"
        )
        .await,
        pre_private
    );
    assert_eq!(
        count_where(
            &pool,
            "SELECT COUNT(*) FROM kb_key_blocks WHERE disclosure IS NULL"
        )
        .await,
        4 - pre_private
    );

    // Every surviving pre-cutover column byte-identical.
    assert_eq!(
        dump(&pool, PRE_CUTOVER_COLUMNS_DUMP_SQL).await,
        pre_columns,
        "row ids/bodies/statuses/created_at must survive the rebuild"
    );

    // The retired legacy key is gone; unknown keys stay verbatim.
    let extensions: Option<String> = sqlx::query_scalar(
        "SELECT extensions_nexus_json FROM kb_key_blocks WHERE key_block_id = 'kb_shared_a'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let extensions: serde_json::Value = serde_json::from_str(&extensions.unwrap()).unwrap();
    assert!(
        extensions.get("creator_only").is_none(),
        "the legacy extension key is not unknown passthrough: {extensions}"
    );
    assert_eq!(extensions["world_id"], "wld_ownerA");
    assert_eq!(extensions["custom_unknown"]["x"], 1);

    // ── Post-cutover schema shape ───────────────────────────────────────
    let kb_columns = column_names(&pool, "kb_key_blocks").await;
    assert!(kb_columns.iter().any(|c| c == "holder_entry_id"));
    assert!(kb_columns.iter().any(|c| c == "disclosure"));
    assert!(
        !kb_columns.iter().any(|c| c == "creator_only"),
        "no runtime creator_only column"
    );
    for table in ["narrative_worlds", "characters"] {
        assert!(
            column_names(&pool, table)
                .await
                .iter()
                .any(|c| c == "knowledge_revision"),
            "{table} must carry knowledge_revision"
        );
        assert_eq!(
            count_where(
                &pool,
                &format!("SELECT COUNT(*) FROM {table} WHERE knowledge_revision = 0")
            )
            .await,
            count_where(&pool, &format!("SELECT COUNT(*) FROM {table}")).await,
            "{table}.knowledge_revision starts at 0"
        );
    }
    assert!(
        table_exists(&pool, "knowledge_import_quarantine").await,
        "quarantine store exists"
    );
    assert_eq!(
        column_names(&pool, "knowledge_import_quarantine").await,
        vec![
            "quarantine_id",
            "import_batch_id",
            "controlling_creator_id",
            "owner_kind",
            "world_id",
            "character_id",
            "actor_world_binding_id",
            "quarantine_reason",
            "original_entry_json",
            "source_provenance_json",
            "created_at",
        ],
        "quarantine store shape"
    );
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM knowledge_import_quarantine").await,
        0,
        "the backfill quarantines nothing on its own"
    );

    // ── The completed version is admitted, and the run is idempotent ────
    pool.close().await;
    let pool = nexus_local_db::init_pool(&db_path).await.unwrap();
    let versions = nexus_local_db::read_versions(&pool).await.unwrap();
    assert_eq!(
        versions.db_schema_version,
        nexus_local_db::DB_SCHEMA_VERSION,
        "the version line advanced with the cutover"
    );
    nexus_local_db::validate(&pool, nexus_local_db::RuntimeRole::Cli)
        .await
        .expect("a migrated workspace validates as current");
    assert_eq!(
        count_where(&pool, "SELECT COUNT(*) FROM knowledge_holders").await,
        4,
        "re-running the migration must not add holders"
    );
}

/// The registry/quarantine/revision/governance schema, its guards and its
/// indexes, inspected on a migrated workspace.
#[tokio::test]
async fn v1191_holder_migration_schema_shape() {
    let (pool, _dir) = migrated_pool().await;

    assert_eq!(
        column_names(&pool, "knowledge_holders").await,
        vec![
            "holder_entry_id",
            "creator_id",
            "character_id",
            "created_at"
        ],
        "registry shape"
    );
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' \
         AND tbl_name IN ('knowledge_holders', 'knowledge_import_quarantine') \
         ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        indexes.contains(&"idx_knowledge_holders_creator_unique".to_string())
            && indexes.contains(&"idx_knowledge_holders_character_unique".to_string()),
        "per-subject unique partial indexes: {indexes:?}"
    );
    assert!(indexes.contains(&"idx_knowledge_import_quarantine_batch".to_string()));
    // Governance is indexed alongside the owner/page keys without replacing
    // them.
    let kb_indexes = index_names(&pool).await;
    for name in OWNER_INDEXES {
        assert!(kb_indexes.contains(&name.to_string()), "missing {name}");
    }
    assert!(
        kb_indexes.contains(&"idx_kb_key_blocks_holder_governance".to_string()),
        "governance index: {kb_indexes:?}"
    );

    // Both new persistent tables carry the writer-admission guards.
    let triggers: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'trigger' ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap();
    for table in ["knowledge_holders", "knowledge_import_quarantine"] {
        for op in ["insert", "update", "delete"] {
            let expected = format!("guard_{table}_{op}");
            assert!(
                triggers.contains(&expected),
                "missing writer guard {expected}"
            );
        }
    }
    // The governance FK is a real RESTRICT reference.
    let fks: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT \"table\", \"from\", on_delete FROM pragma_foreign_key_list('kb_key_blocks')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        fks.iter()
            .any(|(table, from, on_delete)| table == "knowledge_holders"
                && from == "holder_entry_id"
                && on_delete == "RESTRICT"),
        "governance FK must be RESTRICT: {fks:?}"
    );
}

/// The row→record projection reads the real governance columns: a migrated
/// restricted row never reads back as shared (the T2 placeholder risk), the
/// ordinary update path cannot transfer governance, and neither read nor write
/// re-creates the retired legacy key.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn v1191_holder_migration_projection_never_reads_back_shared() {
    let (pool, _dir) = migrated_pool().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    let holder = {
        let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
        let id = nexus_local_db::ensure_creator_holder_in_tx(&mut tx, CREATOR)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        id
    };

    let store = SqliteKbStore::new(pool.clone());
    let mut private = KnowledgeEntryRecord::new(WORLD_A, BlockType::Character, "Private Hero");
    private.holder_entry_id = Some(holder.clone());
    private.disclosure = Some("owner-private".to_string());
    store.insert_knowledge_entry(private.clone()).await.unwrap();

    let shared = KnowledgeEntryRecord::new(WORLD_A, BlockType::Character, "Shared Hero");
    store.insert_knowledge_entry(shared.clone()).await.unwrap();

    let fetched_private = store.get_knowledge_entry(&private.entry_id).await.unwrap();
    assert_eq!(
        fetched_private.holder_entry_id.as_deref(),
        Some(holder.as_str())
    );
    assert_eq!(fetched_private.disclosure.as_deref(), Some("owner-private"));
    assert!(
        fetched_private.creator_only,
        "the legacy carrier must not report a restricted row as shared"
    );
    let fetched_shared = store.get_knowledge_entry(&shared.entry_id).await.unwrap();
    assert_eq!(fetched_shared.holder_entry_id, None);
    assert_eq!(fetched_shared.disclosure, None);
    assert!(!fetched_shared.creator_only);

    // The listing path projects the same pair.
    let listed = store.list_by_world(WORLD_A).await.unwrap();
    let listed_private = listed
        .iter()
        .find(|e| e.entry_id == private.entry_id)
        .expect("private entry listed");
    assert_eq!(
        listed_private.holder_entry_id.as_deref(),
        Some(holder.as_str())
    );
    assert_eq!(listed_private.disclosure.as_deref(), Some("owner-private"));

    // Ordinary update is not a governance transfer mechanism.
    let mut moved = fetched_private.clone();
    moved.disclosure = None;
    moved.holder_entry_id = None;
    assert!(
        matches!(
            store.update_knowledge_entry(moved).await,
            Err(KbStoreError::ImmutableGovernance(_))
        ),
        "clearing governance through the ordinary store must be refused"
    );
    // A content edit that preserves the pair succeeds and keeps it.
    let mut edited = fetched_private.clone();
    edited.canonical_name = "Private Hero Renamed".to_string();
    store.update_knowledge_entry(edited).await.unwrap();
    let after = store.get_knowledge_entry(&private.entry_id).await.unwrap();
    assert_eq!(after.holder_entry_id.as_deref(), Some(holder.as_str()));
    assert_eq!(after.disclosure.as_deref(), Some("owner-private"));

    // The retired legacy key is never re-created in the extension document.
    let extensions: Option<String> = sqlx::query_scalar(
        "SELECT extensions_nexus_json FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(&private.entry_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !extensions.unwrap_or_default().contains("creator_only"),
        "governance never travels through extensions.nexus"
    );
}

/// The same global Creator resolves to the same holder id in every workspace
/// materialization, and the registry refuses a colliding id or a second holder.
#[tokio::test]
async fn v1191_holder_migration_cross_workspace_ids_are_stable() {
    let mut registry_rows = Vec::new();
    for _ in 0..2 {
        let (pool, _dir, _db_path) = pre_holder_db().await;
        seed_creator(&pool).await;
        seed_world(&pool, WORLD_A).await;
        seed_pre_cutover_kb(&pool, "kb_priv", "Private", Some(WORLD_A), None, 1, None).await;
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("holder migration applies");
        registry_rows.push(
            sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT holder_entry_id, creator_id FROM knowledge_holders",
            )
            .fetch_all(&pool)
            .await
            .unwrap(),
        );
    }
    assert_eq!(registry_rows[0], registry_rows[1]);
    assert_eq!(
        registry_rows[0],
        vec![(
            nexus_local_db::creator_holder_entry_id(CREATOR),
            Some(CREATOR.to_string())
        )],
        "the same Creator materialization resolves the same holder id"
    );

    // A colliding id (PK) or a second subject binding is refused outright.
    let (pool, _dir) = migrated_pool().await;
    seed_creator(&pool).await;
    seed_creator_row(&pool, CREATOR_B).await;
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    let holder = nexus_local_db::ensure_creator_holder_in_tx(&mut tx, CREATOR)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    expect_unique_violation(
        sqlx::query("INSERT INTO knowledge_holders (holder_entry_id, creator_id) VALUES (?, ?)")
            .bind(&holder)
            .bind(CREATOR_B)
            .execute(&pool)
            .await
            .map(|_| ()),
        "an id already bound to another subject",
    );
    // The mismatch is a hard error through the registry API too.
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    sqlx::query("UPDATE knowledge_holders SET creator_id = ? WHERE holder_entry_id = ?")
        .bind(CREATOR_B)
        .bind(&holder)
        .execute(&mut *tx)
        .await
        .unwrap();
    let err = nexus_local_db::ensure_creator_holder_in_tx(&mut tx, CREATOR_B)
        .await
        .unwrap_err();
    let _ = tx.rollback().await;
    assert!(
        matches!(&err, nexus_local_db::LocalDbError::ConstraintViolation { table, constraint }
            if table == "knowledge_holders" && constraint.contains("refusing reassignment")),
        "expected a hard mismatch error, got {err:?}"
    );
}

/// Every ambiguous or corrupt preflight aborts the whole migration transaction:
/// unresolved controlling Creator, a contradictory legacy extension document, a
/// `WorldSheet` row this cutover would privatize, a pre-existing governance
/// column, and a malformed extension document. Each case asserts the error
/// names its guard **and** that the workspace is untouched.
#[allow(clippy::too_many_lines)] // one abort matrix, one contract
#[tokio::test]
async fn v1191_holder_migration_aborts_without_mutation() {
    // 1. The stored World controlling Creator cannot be resolved.
    let (pool, _dir, _db_path) = pre_holder_db().await;
    seed_creator(&pool).await;
    seed_world_owned_by(&pool, WORLD_A, "ctr_orphaned").await;
    seed_pre_cutover_kb(&pool, "kb_priv", "Private", Some(WORLD_A), None, 1, None).await;
    let err = nexus_local_db::run_migrations(&pool).await.unwrap_err();
    assert!(
        err.to_string().contains(
            "NOT NULL constraint failed: _preflight_unresolvable_world_creator.violations"
        ),
        "unexpected error: {err}"
    );
    assert_holder_migration_aborted(&pool, "unresolvable controlling Creator").await;

    // 2. The legacy extension document contradicts the column.
    let (pool, _dir, _db_path) = pre_holder_db().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_pre_cutover_kb(
        &pool,
        "kb_conflict",
        "Conflict",
        Some(WORLD_A),
        None,
        0,
        Some("{\"creator_only\":true}"),
    )
    .await;
    let err = nexus_local_db::run_migrations(&pool).await.unwrap_err();
    assert!(
        err.to_string().contains(
            "NOT NULL constraint failed: _preflight_legacy_extension_conflict.violations"
        ),
        "unexpected error: {err}"
    );
    assert_holder_migration_aborted(&pool, "contradictory legacy extension").await;

    // 3. A WorldSheet-linked row would become private.
    let (pool, _dir, _db_path) = pre_holder_db().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER, "Aria").await;
    seed_pre_cutover_kb(
        &pool,
        "kb_sheet_priv",
        "Sheet",
        Some(WORLD_A),
        None,
        1,
        None,
    )
    .await;
    sqlx::query(
        "INSERT INTO actor_world_bindings \
         (binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, updated_at) \
         VALUES (?, ?, ?, 'active', 'kb_sheet_priv', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
    )
    .bind(BINDING)
    .bind(CHARACTER)
    .bind(WORLD_A)
    .execute(&pool)
    .await
    .unwrap();
    let err = nexus_local_db::run_migrations(&pool).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("NOT NULL constraint failed: _preflight_world_sheet_privatized.violations"),
        "unexpected error: {err}"
    );
    assert_holder_migration_aborted(&pool, "privatized WorldSheet").await;

    // 4. A pre-existing native governance column (hostile/partial schema).
    let (pool, _dir, _db_path) = pre_holder_db().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_pre_cutover_kb(&pool, "kb_priv", "Private", Some(WORLD_A), None, 1, None).await;
    sqlx::query("ALTER TABLE kb_key_blocks ADD COLUMN holder_entry_id TEXT")
        .execute(&pool)
        .await
        .unwrap();
    let err = nexus_local_db::run_migrations(&pool).await.unwrap_err();
    assert!(
        err.to_string().contains(
            "NOT NULL constraint failed: _preflight_preexisting_governance_columns.violations"
        ),
        "unexpected error: {err}"
    );
    assert!(
        !table_exists(&pool, "knowledge_holders").await,
        "the registry must not survive the abort"
    );
    assert_eq!(
        count_where(
            &pool,
            &format!(
                "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = {}",
                nexus_local_db::HOLDER_MIGRATION_VERSION
            )
        )
        .await,
        0
    );

    // 5. A malformed extension document.
    let (pool, _dir, _db_path) = pre_holder_db().await;
    seed_creator(&pool).await;
    seed_world(&pool, WORLD_A).await;
    seed_pre_cutover_kb(
        &pool,
        "kb_malformed",
        "Malformed",
        Some(WORLD_A),
        None,
        0,
        Some("not-json"),
    )
    .await;
    let err = nexus_local_db::run_migrations(&pool).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("NOT NULL constraint failed: _preflight_malformed_extensions.violations"),
        "unexpected error: {err}"
    );
    assert_holder_migration_aborted(&pool, "malformed extension document").await;
}
