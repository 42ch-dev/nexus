//! P0-T2 service-layer migration of the retained cross-world skip/reimport
//! regression. Also proves dry-run and the transitional bridge's admission.
#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use nexus_contracts::daemon_api::kb::{PackExportRequest, PackImportRequest};
use nexus_contracts::BlockType;
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, HolderMapping, HolderMappingSelector,
    QuarantineReason,
};
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryRecord, DISCLOSURE_OWNER_PRIVATE};
use nexus_knowledge::world_kb::KbStore;
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
        // Production materialization: a stored Creator always has its holder
        // registry row, which the admitted read selections resolve.
        nexus_local_db::ensure_creator_row(pool, owner, "Test")
            .await
            .unwrap();
    }
    for (world, owner) in [
        (SOURCE, CREATOR),
        (TARGET, CREATOR),
        (FOREIGN, "other_creator"),
    ] {
        sqlx::query("INSERT INTO narrative_worlds (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) VALUES (?, 'ws', ?, 'Pack World', 'pack-world', 'active', 'private', 'manual', '{}')")
            .bind(world).bind(owner).execute(pool).await.unwrap();
    }
    for (id, name) in [
        ("kb_pack_a", "Aria"),
        ("kb_pack_b", "Kael"),
        ("kb_pack_c", "Mira"),
    ] {
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
    for (idx, entry) in pack["entries"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        let old = entry["entry_id"].as_str().unwrap().to_string();
        let new = format!("kb_import_test_{idx:03}");
        entry["entry_id"] = json!(new);
        remap.insert(old, new);
    }
    for (idx, relation) in pack["relations"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        relation["relation_id"] = json!(format!("rel_import_test_{idx:03}"));
        for field in ["from_id", "to_id"] {
            let old = relation[field].as_str().unwrap();
            relation[field] = json!(remap[old]);
        }
    }
}

async fn atom_counts(pool: &SqlitePool, world_id: &str) -> (i64, i64) {
    let entries = sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE world_id = ?")
        .bind(world_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let relations = sqlx::query_scalar("SELECT COUNT(*) FROM kb_relationships WHERE world_id = ?")
        .bind(world_id)
        .fetch_one(pool)
        .await
        .unwrap();
    (entries, relations)
}

#[tokio::test]
async fn pack_import_skip_cross_world_and_reimport_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &home, CREATOR, "default",
    ))
    .unwrap();
    std::fs::write(nexus_home.join("config.toml"), format!("active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"default\"\n")).unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, "default");
    let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .unwrap();
    let pool = guarded.clone_pool();
    seed(&pool).await;
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.clone(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let export_request: PackExportRequest = serde_json::from_value(json!({})).unwrap();
    let exported = core
        .export_world_pack(&principal, SOURCE.to_string(), export_request, false)
        .await
        .unwrap();
    let mut pack = serde_json::to_value(exported).unwrap();

    // Foreign-world PKs are not silently stolen or overwritten.
    let foreign_ids = core
        .import_world_pack(
            &principal,
            TARGET.to_string(),
            request(&pack, "skip"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert_eq!(foreign_ids.entries.skipped, 3);
    assert_eq!(foreign_ids.relations.skipped, 1);
    assert_eq!(atom_counts(&pool, TARGET).await, (0, 0));
    fresh_entry_ids_in_pack(&mut pack);

    let preview = core
        .preview_world_pack_import(
            &principal,
            TARGET.to_string(),
            request(&pack, "skip"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert_eq!(preview.entries.created, 3);
    assert_eq!(preview.relations.created, 1);
    assert_eq!(atom_counts(&pool, TARGET).await, (0, 0));

    // The service denies a foreign World before parsing or writing the pack.
    let denied = core
        .import_world_pack(
            &principal,
            FOREIGN.to_string(),
            request(&pack, "skip"),
            Vec::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(denied, CoreError::WorldOwnerDenied { .. }));
    // Bridge admission is verified against core stored state: an unknown
    // (forged) creator is rejected on identity before ownership is even
    // consulted, and a known creator on a foreign World is rejected by the
    // ownership guard.
    let denied = CoreService::import_legacy_world_pack(
        &pool,
        "ctr_forged",
        FOREIGN,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(denied, CoreError::AuthRequired));
    let denied = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        FOREIGN,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(denied, CoreError::WorldOwnerDenied { .. }));
    assert_eq!(atom_counts(&pool, FOREIGN).await, (0, 0));

    let reader = nexus_local_db::open_pool_read_only(&db_path).await.unwrap();
    // A read-only connection must fail before any atom write: the query-only
    // pool is refused by the bridge's admission, and SQLite mode=ro pools by
    // the writer-protocol fencing.
    assert!(CoreService::import_legacy_world_pack(
        &reader,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        Vec::new(),
        false
    )
    .await
    .is_err());
    assert_eq!(atom_counts(&reader, TARGET).await, (0, 0));
    let readonly = CoreService::open(CoreOpenOptions {
        user_home: home,
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let read_principal = readonly.active_principal().await.unwrap();
    let denied = readonly
        .import_world_pack(
            &read_principal,
            TARGET.to_string(),
            request(&pack, "skip"),
            Vec::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(denied, CoreError::Forbidden { .. }));

    let first = core
        .import_world_pack(
            &principal,
            TARGET.to_string(),
            request(&pack, "skip"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert_eq!(first.entries.created, 3);
    assert_eq!(first.relations.created, 1);
    assert_eq!(first.entries.rejected, 0);
    assert_eq!(first.relations.rejected, 0);
    let second = core
        .import_world_pack(
            &principal,
            TARGET.to_string(),
            request(&pack, "skip"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert_eq!(second.entries.created, 0);
    assert_eq!(second.entries.skipped, 3);
    assert_eq!(second.relations.created, 0);
    assert_eq!(second.relations.skipped, 1);
    assert_eq!(atom_counts(&reader, TARGET).await, (3, 1));
    assert_eq!(atom_counts(&reader, SOURCE).await, (3, 1));
    let provenance: Vec<(String, Option<String>)> = sqlx::query_as("SELECT canonical_name, source_provenance_kind FROM kb_key_blocks WHERE world_id = ? ORDER BY canonical_name")
        .bind(TARGET).fetch_all(&reader).await.unwrap();
    assert_eq!(
        provenance,
        vec![
            ("Aria".into(), Some("pack_import".into())),
            ("Kael".into(), Some("pack_import".into())),
            ("Mira".into(), Some("pack_import".into()))
        ]
    );
    let bridge = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(bridge.entries.skipped, 3);
    assert_eq!(bridge.relations.skipped, 1);
    assert_eq!(atom_counts(&reader, TARGET).await, (3, 1));
    readonly.close().await.unwrap();
    reader.close().await;
    core.close().await.unwrap();
    pool.close().await;
}

/// qc1-F-001 regression (v1.190 P0 fix wave): the retained CLI open path
/// (`nexus_local_db::init_pool`) registers its pool as the `bootstrap`
/// writer while the configured active creator is a real non-bootstrap
/// owner. The bridge must admit that owner through verified stored-state
/// admission (creator exists in the stored creator state + owns the World)
/// instead of hard-failing `AuthRequired`, and must still reject
/// forged/unknown creators and foreign Worlds.
#[tokio::test]
async fn bridge_admits_non_bootstrap_owner_on_bootstrap_pool() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &home, CREATOR, "default",
    ))
    .unwrap();
    std::fs::write(nexus_home.join("config.toml"), format!("active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"default\"\n")).unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, "default");
    let cli_pool = nexus_local_db::init_pool(&db_path).await.unwrap();
    seed(&cli_pool).await;

    // The retained CLI pool's registered writer identity is `bootstrap`,
    // not the configured active creator.
    let identities: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT creator_id FROM core_writer_registration")
            .fetch_all(&cli_pool)
            .await
            .unwrap();
    assert!(!identities.is_empty());
    assert!(
        identities.iter().all(|c| c == "bootstrap"),
        "retained CLI pool must be bootstrap-registered: {identities:?}"
    );

    // Export through the read-only service (export is a read path) so the
    // pack bytes come from the real export owner; the bridge then imports
    // on the bootstrap pool only — no second writer registration.
    let reader_core = CoreService::open(CoreOpenOptions {
        user_home: home,
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let reader_principal = reader_core.active_principal().await.unwrap();
    let export_request: PackExportRequest = serde_json::from_value(json!({})).unwrap();
    let exported = reader_core
        .export_world_pack(&reader_principal, SOURCE.to_string(), export_request, false)
        .await
        .unwrap();
    let mut pack = serde_json::to_value(exported).unwrap();
    // Atom ids are globally unique PKs; the SOURCE originals would collide
    // (skip policy) against the same DB's SOURCE rows. Mint fresh target ids.
    fresh_entry_ids_in_pack(&mut pack);
    let plan = CoreService::import_legacy_world_pack(
        &cli_pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        Vec::new(),
        true,
    )
    .await
    .unwrap();
    assert_eq!(plan.entries.created, 3);
    assert_eq!(plan.relations.created, 1);
    assert_eq!(atom_counts(&cli_pool, TARGET).await, (0, 0));

    // The real non-bootstrap active creator import succeeds.
    let summary = CoreService::import_legacy_world_pack(
        &cli_pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(summary.entries.created, 3);
    assert_eq!(summary.relations.created, 1);
    assert_eq!(atom_counts(&cli_pool, TARGET).await, (3, 1));

    // Forged/unknown creators are still rejected, before any write.
    let denied = CoreService::import_legacy_world_pack(
        &cli_pool,
        "ctr_forged",
        TARGET,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(denied, CoreError::AuthRequired));
    assert_eq!(atom_counts(&cli_pool, TARGET).await, (3, 1));

    // A known creator on a foreign World is still rejected by ownership.
    let denied = CoreService::import_legacy_world_pack(
        &cli_pool,
        CREATOR,
        FOREIGN,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(denied, CoreError::WorldOwnerDenied { .. }));
    assert_eq!(atom_counts(&cli_pool, FOREIGN).await, (0, 0));
    reader_core.close().await.unwrap();
    cli_pool.close().await;
}

// ── v1.191 P1 T10: pack import/export identity (durable §6) ───────────────

/// One governed pack entry: `owner` is the wire holder id, `disclosure` the
/// wire governance value, both carried exactly as the pack states them.
fn governed_pack(entries: &[(&str, &str, Option<&str>, Option<&str>)]) -> Value {
    let entries: Vec<Value> = entries
        .iter()
        .map(|(id, name, owner, disclosure)| {
            let mut entry = json!({
                "schema_version": 1,
                "entry_id": id,
                "entry_type": "character",
                "canonical_name": name,
                "status": "confirmed",
                "body": { "summary": format!("{name} summary") },
                "extensions": { "nexus": { "world_id": SOURCE } }
            });
            if let Some(owner) = owner {
                entry["owner"] = json!(owner);
            }
            if let Some(disclosure) = disclosure {
                entry["disclosure"] = json!(disclosure);
            }
            entry
        })
        .collect();
    json!({
        "modules": { "pack": { "title": "Governed", "version": "0.1.0", "creator": "packAuthor" } },
        "entries": entries,
        "relations": []
    })
}

async fn quarantine_rows(pool: &SqlitePool) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT quarantine_reason, json_extract(original_entry_json, '$.entry_id'), \
                json_extract(original_entry_json, '$.owner') \
         FROM knowledge_import_quarantine ORDER BY 2",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn stored_governance(pool: &SqlitePool, entry_id: &str) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT holder_entry_id, disclosure FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Boots an `EngineOwner` core over a seeded workspace.
async fn pack_test_core() -> (tempfile::TempDir, sqlx::SqlitePool, CoreService) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_path_buf();
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &home, CREATOR, "default",
    ))
    .unwrap();
    std::fs::write(nexus_home.join("config.toml"), format!("active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"default\"\n")).unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, "default");
    let pool = nexus_local_db::init_pool(&db_path).await.unwrap();
    seed(&pool).await;
    let core = CoreService::open(CoreOpenOptions {
        user_home: home,
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    (tmp, pool, core)
}

/// A foreign holder id is never adopted by equality, and an unmapped one is
/// held outside the KB stores — the exact string of the local Creator holder
/// included.
#[tokio::test]
async fn v1191_holder_pack_unmapped_and_colliding_holders_stay_quarantined() {
    let (_tmp, pool, core) = pack_test_core().await;
    let principal = core.active_principal().await.unwrap();
    let local_holder = nexus_local_db::holders::creator_holder_entry_id(CREATOR);
    let pack = governed_pack(&[
        ("kb_gov_shared", "Shared Row", None, None),
        (
            "kb_gov_collide",
            "Colliding Row",
            Some(local_holder.as_str()),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        (
            "kb_gov_foreign",
            "Foreign Row",
            Some("hld_foreign_peer"),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
    ]);

    let summary = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap();

    assert_eq!(
        summary.entries.created, 1,
        "only the shared atom is storable"
    );
    assert_eq!(summary.entries.rejected, 2);
    assert_eq!(summary.quarantined.len(), 2);
    assert!(summary
        .quarantined
        .iter()
        .all(|atom| atom.reason == QuarantineReason::UnresolvedHolder));
    assert!(summary
        .quarantined
        .iter()
        .all(|atom| atom.quarantine_id.starts_with("qrn_")));
    assert_eq!(
        quarantine_rows(&pool).await,
        vec![
            (
                "unresolved_holder".into(),
                "kb_gov_collide".into(),
                local_holder.clone()
            ),
            (
                "unresolved_holder".into(),
                "kb_gov_foreign".into(),
                "hld_foreign_peer".into()
            ),
        ]
    );
    // The colliding atom did NOT claim the local identity: nothing was
    // stored for it at all (neither the local holder nor any other).
    let colliding_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = ?")
            .bind("kb_gov_collide")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(colliding_rows, 0, "no adoption by string equality");
    assert_eq!(atom_counts(&pool, TARGET).await, (1, 0));
    let (shared_holder, shared_disclosure) = stored_governance(&pool, "kb_gov_shared").await;
    assert_eq!((shared_holder, shared_disclosure), (None, None));

    // The bounded review arm reads the batch back, owner-only, with the
    // immutable original JSON and never a knowledge view.
    let batch_id = summary.quarantined[0].import_batch_id.clone();
    let review = core
        .review_world_pack_import(&principal, TARGET.to_string(), batch_id.clone())
        .await
        .unwrap();
    assert!(!review.truncated);
    assert_eq!(review.batch_id, batch_id);
    assert_eq!(review.atoms.len(), 2);
    let colliding = review
        .atoms
        .iter()
        .find(|atom| atom.entry_id == "kb_gov_collide")
        .expect("colliding atom is reviewable");
    assert_eq!(
        colliding.original_owner.as_deref(),
        Some(local_holder.as_str())
    );
    assert_eq!(
        colliding.original_disclosure.as_deref(),
        Some(DISCLOSURE_OWNER_PRIVATE)
    );
    // The reviewed original is the **pack document's** atom JSON verbatim, not
    // a re-serialization of the typed entry.
    let document_atom = pack["entries"][1].to_string();
    assert_eq!(
        colliding.original_entry.as_deref(),
        Some(document_atom.as_str()),
        "review returns the document's atom JSON byte-for-byte"
    );
    // A foreign World's batch is not reviewable by this Creator.
    let denied = core
        .review_world_pack_import(&principal, FOREIGN.to_string(), batch_id)
        .await
        .unwrap_err();
    assert!(matches!(denied, CoreError::WorldOwnerDenied { .. }));
    core.close().await.unwrap();
    pool.close().await;
}

/// An explicit mapping adopts the atom natively in one transaction with its
/// quarantine removal; a repeat import re-adopts nothing and adds nothing.
#[tokio::test]
async fn v1191_holder_pack_mapped_adoption_removes_quarantine_atomically() {
    let (_tmp, pool, core) = pack_test_core().await;
    let local_holder = nexus_local_db::holders::creator_holder_entry_id(CREATOR);
    let pack = governed_pack(&[(
        "kb_gov_foreign",
        "Foreign Row",
        Some("hld_foreign_peer"),
        Some(DISCLOSURE_OWNER_PRIVATE),
    )]);

    let held = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        Vec::new(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(held.quarantined.len(), 1);
    assert_eq!(quarantine_rows(&pool).await.len(), 1);
    assert_eq!(atom_counts(&pool, TARGET).await, (0, 0));

    // Inadmissible mapping: a Character selector for a non-existent Character
    // refuses the whole import with zero writes.
    let refused = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        vec![HolderMapping {
            foreign_holder_id: "hld_foreign_peer".to_string(),
            selector: HolderMappingSelector::CharacterPrivate("chr_missing".to_string()),
        }],
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(refused, CoreError::InvalidInput { .. }));
    assert_eq!(
        quarantine_rows(&pool).await.len(),
        1,
        "refusal keeps the row"
    );

    // The admitted mapping adopts the atom and drops its quarantine row.
    let adopted = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        vec![HolderMapping {
            foreign_holder_id: "hld_foreign_peer".to_string(),
            selector: HolderMappingSelector::AuthorOnly,
        }],
        false,
    )
    .await
    .unwrap();
    assert_eq!(adopted.entries.created, 1);
    assert!(adopted.quarantined.is_empty());
    assert!(
        quarantine_rows(&pool).await.is_empty(),
        "quarantine row released with the adoption"
    );
    assert_eq!(
        stored_governance(&pool, "kb_gov_foreign").await,
        (
            Some(local_holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string())
        ),
        "adopted atom carries the mapped local holder and its original disclosure"
    );

    // Repeat import: nothing re-created, nothing re-quarantined.
    let repeat = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        vec![HolderMapping {
            foreign_holder_id: "hld_foreign_peer".to_string(),
            selector: HolderMappingSelector::AuthorOnly,
        }],
        false,
    )
    .await
    .unwrap();
    assert_eq!(repeat.entries.created, 0);
    assert_eq!(repeat.entries.skipped, 1);
    assert!(repeat.quarantined.is_empty());
    assert!(quarantine_rows(&pool).await.is_empty());
    assert_eq!(atom_counts(&pool, TARGET).await, (1, 0));
    core.close().await.unwrap();
    pool.close().await;
}

/// Unknown disclosure vocabulary is not native and stays quarantined even when
/// its holder is mapped.
#[tokio::test]
async fn v1191_holder_pack_unknown_disclosure_stays_quarantined_when_mapped() {
    let (_tmp, pool, core) = pack_test_core().await;
    let pack = governed_pack(&[(
        "kb_gov_unknown",
        "Unknown Disclosure Row",
        Some("hld_foreign_peer"),
        Some("team-shared"),
    )]);

    let summary = CoreService::import_legacy_world_pack(
        &pool,
        CREATOR,
        TARGET,
        request(&pack, "skip"),
        vec![HolderMapping {
            foreign_holder_id: "hld_foreign_peer".to_string(),
            selector: HolderMappingSelector::AuthorOnly,
        }],
        false,
    )
    .await
    .unwrap();
    assert_eq!(summary.entries.created, 0);
    assert_eq!(summary.quarantined.len(), 1);
    assert_eq!(
        summary.quarantined[0].reason,
        QuarantineReason::UnknownDisclosure
    );
    assert_eq!(
        summary.quarantined[0].original_disclosure.as_deref(),
        Some("team-shared")
    );
    assert_eq!(
        quarantine_rows(&pool).await,
        vec![(
            "unknown_disclosure".into(),
            "kb_gov_unknown".into(),
            "hld_foreign_peer".into()
        )]
    );
    assert_eq!(atom_counts(&pool, TARGET).await, (0, 0));
    core.close().await.unwrap();
    pool.close().await;
}

// ── MIGRATED (daemon `world_kb_pack.rs` retirement, v1.193 P2-T3) ─────────
//
// The retired HTTP fixture drove these through `POST .../kb/pack/{export,import}`.
// The domain cases below are the ones `pack_import_skip_cross_world_and_reimport_is_idempotent`
// and the `v1191_holder_pack_*` cases did not already cover: the `rename` /
// `overwrite` conflict policies, the pack envelope's default title, the
// owner-only review bound, and the stable `qrn_` / `pib_` identifier shapes.
// The route-level 200/403 envelopes, `Axum` extractor rejections and
// `holder_map`/`review_import` wire parsing stay retired with the host.

/// A pack of `count` foreign-governed atoms — every one of them is held, so
/// no KB row is written and the run yields one review batch.
fn foreign_atoms_pack(count: usize) -> Value {
    let entries: Vec<Value> = (0..count)
        .map(|idx| {
            json!({
                "schema_version": 1,
                "entry_id": format!("kb_gov_bulk_{idx:03}"),
                "entry_type": "character",
                "canonical_name": format!("Bulk Row {idx:03}"),
                "status": "confirmed",
                "body": { "summary": "bulk" },
                "owner": "hld_foreign_peer",
                "disclosure": DISCLOSURE_OWNER_PRIVATE,
                "extensions": { "nexus": { "world_id": SOURCE } }
            })
        })
        .collect();
    json!({
        "modules": { "pack": { "title": "Bulk", "version": "0.1.0", "creator": "packAuthor" } },
        "entries": entries,
        "relations": []
    })
}

async fn provenance_of(pool: &SqlitePool, world_id: &str, name: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT source_provenance_kind FROM kb_key_blocks WHERE world_id = ? AND canonical_name = ?",
    )
    .bind(world_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .unwrap()
    .flatten()
}

/// The retained pack-import conflict policies and the bounded owner-only
/// review. Migrated from `world_kb_pack.rs`
/// (`pack_import_rename_creates_disambiguated_entry`,
/// `pack_import_overwrite_replaces_body_preserves_status`,
/// `pack_import_same_world_reimport_overwrite_updates_body`,
/// `v1191_holder_pack_http_review_is_bounded`,
/// `v1191_holder_pack_http_unmapped_and_colliding_holders_stay_quarantined`,
/// `pack_export_owned_world_returns_pack_envelope`) — surviving variants only.
#[tokio::test]
async fn retained_pack_conflict_policies_and_review_bound() {
    let (_tmp, pool, core) = pack_test_core().await;
    let principal = core.active_principal().await.unwrap();

    // ── Export envelope: the pack title defaults to the World title ───────
    let export_request: PackExportRequest = serde_json::from_value(json!({})).unwrap();
    let exported = core
        .export_world_pack(&principal, SOURCE.to_string(), export_request, false)
        .await
        .unwrap();
    let mut pack = serde_json::to_value(exported).unwrap();
    assert_eq!(
        pack["modules"]["pack"]["title"], "Pack World",
        "a defaulted pack title is the World title: {pack}"
    );
    // The pristine envelope keeps the source World's own atom ids for the
    // same-world re-import arm below.
    let same_world_pack = pack.clone();
    fresh_entry_ids_in_pack(&mut pack);

    // ── rename: a canonical-name collision creates a disambiguated row and
    //    the imported relation endpoints follow the renamed id ─────────────
    sqlx::query("INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, created_at, updated_at) VALUES ('kb_target_kael', ?, 'character', 'Kael', 'confirmed', 0, ?, datetime('now'), datetime('now'))")
        .bind(TARGET)
        .bind(json!({"summary": "Pre-existing Kael"}).to_string())
        .execute(&pool)
        .await
        .unwrap();

    let renamed = core
        .import_world_pack(
            &principal,
            TARGET.to_string(),
            request(&pack, "rename"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert!(renamed.entries.renamed >= 1, "one collision renamed");
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT canonical_name FROM kb_key_blocks WHERE world_id = ? ORDER BY canonical_name",
    )
    .bind(TARGET)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        names.len(),
        4,
        "Aria + Mira + pre-existing Kael + renamed Kael: {names:?}"
    );
    let renamed_name = names
        .iter()
        .find(|name| name.contains("imported"))
        .expect("renamed member carries the disambiguating suffix")
        .clone();
    let renamed_id: String = sqlx::query_scalar(
        "SELECT key_block_id FROM kb_key_blocks WHERE world_id = ? AND canonical_name = ?",
    )
    .bind(TARGET)
    .bind(&renamed_name)
    .fetch_one(&pool)
    .await
    .unwrap();
    let aria_id: String = sqlx::query_scalar(
        "SELECT key_block_id FROM kb_key_blocks WHERE world_id = ? AND canonical_name = 'Aria'",
    )
    .bind(TARGET)
    .fetch_one(&pool)
    .await
    .unwrap();
    let endpoints: (String, String) = sqlx::query_as(
        "SELECT source_entity_id, target_entity_id FROM kb_relationships WHERE world_id = ? LIMIT 1",
    )
    .bind(TARGET)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        endpoints,
        (aria_id, renamed_id),
        "relation endpoints remap onto the imported ids"
    );
    assert_eq!(
        provenance_of(&pool, TARGET, "Aria").await.as_deref(),
        Some("pack_import")
    );
    assert_eq!(
        provenance_of(&pool, TARGET, &renamed_name).await.as_deref(),
        Some("pack_import")
    );
    assert_ne!(
        provenance_of(&pool, TARGET, "Kael").await.as_deref(),
        Some("pack_import"),
        "the pre-existing collision row is never stamped"
    );

    // ── overwrite: the pack body replaces the target body but the target's
    //    own status survives ───────────────────────────────────────────────
    sqlx::query(
        "UPDATE kb_key_blocks SET status = 'provisional' WHERE key_block_id = 'kb_target_kael'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let overwritten = core
        .import_world_pack(
            &principal,
            TARGET.to_string(),
            request(&pack, "overwrite"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert!(
        overwritten.entries.overwritten >= 1,
        "the collision overwrites instead of skipping: {overwritten:?}"
    );
    let (status, body): (String, String) = sqlx::query_as(
        "SELECT status, body_json FROM kb_key_blocks WHERE key_block_id = 'kb_target_kael'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "provisional", "target status preserved");
    assert!(
        body.contains("Kael from pack"),
        "target body replaced by the pack content, got {body}"
    );

    // ── same-world re-import: overwrite restores the pack body ───────────
    sqlx::query("UPDATE kb_key_blocks SET body_json = ? WHERE key_block_id = 'kb_pack_b'")
        .bind(json!({"summary": "Stale Kael body"}).to_string())
        .execute(&pool)
        .await
        .unwrap();
    let same_world: Value = same_world_pack;
    let restored = core
        .import_world_pack(
            &principal,
            SOURCE.to_string(),
            request(&same_world, "overwrite"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert!(
        restored.entries.overwritten >= 1,
        "same-world re-import overwrites rather than skips: {restored:?}"
    );
    let stale: String =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = 'kb_pack_b'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        stale.contains("Kael from pack"),
        "overwrite restores the pack body, got {stale}"
    );

    // ── the owner-only review is bounded and carries stable id shapes ─────
    let held = core
        .import_world_pack(
            &principal,
            TARGET.to_string(),
            request(&foreign_atoms_pack(101), "skip"),
            Vec::new(),
        )
        .await
        .unwrap();
    assert_eq!(held.entries.rejected, 101);
    assert_eq!(held.quarantined.len(), 101);
    let batch_id = held.quarantined[0].batch_id.clone();
    assert!(
        batch_id.starts_with("pib_"),
        "one import batch id per run: {batch_id}"
    );
    assert!(
        held.quarantined
            .iter()
            .all(|atom| atom.batch_id == batch_id),
        "every held atom of one run shares its batch id"
    );
    let digest = held.quarantined[0]
        .quarantine_id
        .strip_prefix("qrn_")
        .expect("quarantine ids carry the qrn_ prefix");
    assert_eq!(digest.len(), 64, "32-byte atom digest: {digest}");
    assert!(
        digest
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "the digest is lowercase hex: {digest}"
    );

    let review = core
        .review_world_pack_import(&principal, TARGET.to_string(), batch_id.clone())
        .await
        .unwrap();
    assert!(review.truncated, "a 101-atom batch exceeds the review cap");
    assert_eq!(
        review.atoms.len(),
        100,
        "the bounded review returns exactly its cap"
    );

    // ── export never reads through a World the caller does not own ────────
    let denied = core
        .export_world_pack(
            &principal,
            FOREIGN.to_string(),
            serde_json::from_value(json!({})).unwrap(),
            false,
        )
        .await
        .unwrap_err();
    assert!(matches!(denied, CoreError::WorldOwnerDenied { .. }));

    core.close().await.unwrap();
    pool.close().await;
}

/// The export reads through the exporting Creator's admitted selection:
/// owned private material only under explicit author intent, governance
/// preserved exactly.
#[tokio::test]
async fn v1191_holder_pack_export_filters_private_rows_without_explicit_intent() {
    let (_tmp, pool, core) = pack_test_core().await;
    let principal = core.active_principal().await.unwrap();
    let local_holder = nexus_local_db::holders::creator_holder_entry_id(CREATOR);
    let store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let mut private = KnowledgeEntryRecord::new(SOURCE, BlockType::Character, "Private Row");
    private.holder_entry_id = Some(local_holder.clone());
    private.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
    store.insert_knowledge_entry(private).await.unwrap();

    let export_request: PackExportRequest = serde_json::from_value(json!({})).unwrap();
    let filtered = core
        .export_world_pack(&principal, SOURCE.to_string(), export_request, false)
        .await
        .unwrap();
    let names: Vec<String> = filtered
        .entries
        .iter()
        .map(|entry| entry["canonical_name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["Aria", "Kael", "Mira"],
        "owned private row excluded without explicit intent"
    );

    let export_request: PackExportRequest = serde_json::from_value(json!({})).unwrap();
    let with_intent = core
        .export_world_pack(&principal, SOURCE.to_string(), export_request, true)
        .await
        .unwrap();
    assert_eq!(with_intent.entries.len(), 4, "explicit intent includes it");
    let exported_private = with_intent
        .entries
        .iter()
        .find(|entry| entry["canonical_name"] == "Private Row")
        .expect("private row exported");
    assert_eq!(exported_private["owner"], json!(local_holder));
    assert_eq!(
        exported_private["disclosure"],
        json!(DISCLOSURE_OWNER_PRIVATE)
    );
    core.close().await.unwrap();
    pool.close().await;
}
