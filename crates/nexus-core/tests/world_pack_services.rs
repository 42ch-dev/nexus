//! P0-T2 service-layer migration of the retained cross-world skip/reimport
//! regression. Also proves dry-run and the transitional bridge's admission.

use nexus_contracts::daemon_api::kb::{PackExportRequest, PackImportRequest};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;

const CREATOR: &str = "test_creator";
const SOURCE: &str = "wld_pack_source";
const TARGET: &str = "wld_pack_target";
const FOREIGN: &str = "wld_pack_foreign";

async fn seed(pool: &SqlitePool) {
    for owner in [CREATOR, "other_creator"] {
        sqlx::query("INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, 'Test', 'active', datetime('now'), '{}')")
            .bind(owner).execute(pool).await.unwrap();
    }
    for (world, owner) in [(SOURCE, CREATOR), (TARGET, CREATOR), (FOREIGN, "other_creator")] {
        sqlx::query("INSERT INTO narrative_worlds (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) VALUES (?, 'ws', ?, 'Pack World', 'pack-world', 'active', 'private', 'manual', '{}')")
            .bind(world).bind(owner).execute(pool).await.unwrap();
    }
    for (id, name) in [("kb_pack_a", "Aria"), ("kb_pack_b", "Kael"), ("kb_pack_c", "Mira")] {
        sqlx::query("INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, created_at, updated_at) VALUES (?, ?, 'character', ?, 'confirmed', 0, ?, datetime('now'), datetime('now'))")
            .bind(id).bind(SOURCE).bind(name).bind(json!({"summary": format!("{name} from pack")}).to_string())
            .execute(pool).await.unwrap();
    }
    sqlx::query("INSERT INTO kb_relationships (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, symmetric, confidence, source_anchor_ids, metadata, created_at, updated_at, revision, needs_review, source) VALUES ('rel_pack_1', ?, 'kb_pack_a', 'kb_pack_b', 'mentors', 0, 1.0, '[]', '{}', datetime('now'), datetime('now'), 0, 0, 'manual')")
        .bind(SOURCE).execute(pool).await.unwrap();
}

fn request(pack: &Value, conflict: &str) -> PackImportRequest {
    serde_json::from_value(json!({"pack": pack, "conflict": conflict})).unwrap()
}

fn fresh_entry_ids_in_pack(pack: &mut Value) {
    let mut remap = HashMap::new();
    for (idx, entry) in pack["entries"].as_array_mut().unwrap().iter_mut().enumerate() {
        let old = entry["entry_id"].as_str().unwrap().to_string();
        let new = format!("kb_import_test_{idx:03}");
        entry["entry_id"] = json!(new);
        remap.insert(old, new);
    }
    for (idx, relation) in pack["relations"].as_array_mut().unwrap().iter_mut().enumerate() {
        relation["relation_id"] = json!(format!("rel_import_test_{idx:03}"));
        for field in ["from_id", "to_id"] {
            let old = relation[field].as_str().unwrap();
            relation[field] = json!(remap[old]);
        }
    }
}

async fn atom_counts(pool: &SqlitePool, world_id: &str) -> (i64, i64) {
    let entries = sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE world_id = ?")
        .bind(world_id).fetch_one(pool).await.unwrap();
    let relations = sqlx::query_scalar("SELECT COUNT(*) FROM kb_relationships WHERE world_id = ?")
        .bind(world_id).fetch_one(pool).await.unwrap();
    (entries, relations)
}

#[tokio::test]
async fn pack_import_skip_cross_world_and_reimport_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(&home, CREATOR, "default")).unwrap();
    std::fs::write(nexus_home.join("config.toml"), format!("active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"default\"\n")).unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, "default");
    let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default()).await.unwrap();
    let pool = guarded.clone_pool();
    seed(&pool).await;
    let core = CoreService::open(CoreOpenOptions { user_home: home.clone(), access: CoreAccess::EngineOwner }).await.unwrap();
    let principal = core.active_principal().await.unwrap();
    let export_request: PackExportRequest = serde_json::from_value(json!({})).unwrap();
    let exported = core.export_world_pack(&principal, SOURCE.to_string(), export_request).await.unwrap();
    let mut pack = serde_json::to_value(exported).unwrap();

    // Foreign-world PKs are not silently stolen or overwritten.
    let foreign_ids = core.import_world_pack(&principal, TARGET.to_string(), request(&pack, "skip")).await.unwrap();
    assert_eq!(foreign_ids.entries.skipped, 3);
    assert_eq!(foreign_ids.relations.skipped, 1);
    assert_eq!(atom_counts(&pool, TARGET).await, (0, 0));
    fresh_entry_ids_in_pack(&mut pack);

    let preview = core.preview_world_pack_import(&principal, TARGET.to_string(), request(&pack, "skip")).await.unwrap();
    assert_eq!(preview.entries.created, 3);
    assert_eq!(preview.relations.created, 1);
    assert_eq!(atom_counts(&pool, TARGET).await, (0, 0));

    // The service rejects a foreign World before parsing or writing the pack.
    let denied = core.import_world_pack(&principal, FOREIGN.to_string(), request(&pack, "skip")).await.unwrap_err();
    assert!(matches!(denied, CoreError::Forbidden { .. }));
    // The old CLI ownership gate accepts this non-bootstrap creator, but its
    // bootstrap-registered pool cannot authenticate that claim for the bridge.
    // It is indistinguishable from a forged creator on the same pool: fail closed.
    assert!(nexus_local_db::narrative_write::is_world_owned(&pool, "other_creator", FOREIGN).await.unwrap());
    let denied = CoreService::import_legacy_world_pack(&pool, "other_creator", FOREIGN, request(&pack, "skip"), false).await.unwrap_err();
    assert!(matches!(denied, CoreError::AuthRequired));
    let denied = CoreService::import_legacy_world_pack(&pool, CREATOR, FOREIGN, request(&pack, "skip"), false).await.unwrap_err();
    assert!(matches!(denied, CoreError::Forbidden { .. }));
    assert_eq!(atom_counts(&pool, FOREIGN).await, (0, 0));

    let reader = nexus_local_db::open_pool_read_only(&db_path).await.unwrap();
    // A read-only connection has no registered writer context. Both explicit
    // query-only pools and SQLite mode=ro pools must fail before any atom write.
    assert!(CoreService::import_legacy_world_pack(&reader, CREATOR, TARGET, request(&pack, "skip"), false).await.is_err());
    assert_eq!(atom_counts(&reader, TARGET).await, (0, 0));
    let readonly = CoreService::open(CoreOpenOptions { user_home: home, access: CoreAccess::ReadOnly }).await.unwrap();
    let read_principal = readonly.active_principal().await.unwrap();
    let denied = readonly.import_world_pack(&read_principal, TARGET.to_string(), request(&pack, "skip")).await.unwrap_err();
    assert!(matches!(denied, CoreError::Forbidden { .. }));

    let first = core.import_world_pack(&principal, TARGET.to_string(), request(&pack, "skip")).await.unwrap();
    assert_eq!(first.entries.created, 3);
    assert_eq!(first.relations.created, 1);
    assert_eq!(first.entries.rejected, 0);
    assert_eq!(first.relations.rejected, 0);
    let second = core.import_world_pack(&principal, TARGET.to_string(), request(&pack, "skip")).await.unwrap();
    assert_eq!(second.entries.created, 0);
    assert_eq!(second.entries.skipped, 3);
    assert_eq!(second.relations.created, 0);
    assert_eq!(second.relations.skipped, 1);
    assert_eq!(atom_counts(&reader, TARGET).await, (3, 1));
    assert_eq!(atom_counts(&reader, SOURCE).await, (3, 1));
    let provenance: Vec<(String, Option<String>)> = sqlx::query_as("SELECT canonical_name, source_provenance_kind FROM kb_key_blocks WHERE world_id = ? ORDER BY canonical_name")
        .bind(TARGET).fetch_all(&reader).await.unwrap();
    assert_eq!(provenance, vec![("Aria".into(), Some("pack_import".into())), ("Kael".into(), Some("pack_import".into())), ("Mira".into(), Some("pack_import".into()))]);
    let bridge = CoreService::import_legacy_world_pack(&pool, CREATOR, TARGET, request(&pack, "skip"), false).await.unwrap();
    assert_eq!(bridge.entries.skipped, 3);
    assert_eq!(bridge.relations.skipped, 1);
    assert_eq!(atom_counts(&reader, TARGET).await, (3, 1));
    readonly.close().await.unwrap();
    reader.close().await;
    core.close().await.unwrap();
    pool.close().await;
}
