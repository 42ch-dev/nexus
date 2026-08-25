//! Hermetic CLI integration tests — `creator world kb entity patch` + `kb graph`
//! (V1.175 P1 Task 4, group 4): daemon OCC entity patch over the existing
//! V1.73 route, end-to-end against a live daemon fixture with hermetic `HOME`
//! (AR-83 #6 / AR-85).
//!
//! Each test seeds one owned World + a `WorldKbEntry` row (revision 0), then
//! drives the REAL `nexus42` binary. Failure paths: the stale-revision path
//! (stale `--expected-version` → 409 `world_kb_conflict` rendering
//! `current_version` + `entity_id` + recovery hint) and the empty-patch
//! fast-fail. `kb graph` reads the entity projection (version + canonical
//! name) and `--json` emits the DTO verbatim.

mod common;

use common::LiveDaemon;
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use std::process::Output;

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Seed one owned World (the fixture's `wld_test_world`) + a `WorldKbEntry`
/// with a valid novel body at revision 0, and return the `entry_id`.
async fn seed_entity(d: &LiveDaemon) -> String {
    let store = SqliteKbStore::new(d.pool.clone());
    let mut kb = WorldKbEntry::new("wld_test_world", BlockType::Character, "Hero");
    kb.body = Some(WorldKbBody {
        summary: Some("Original summary".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: Some(vec!["novel".to_string()]),
        ..Default::default()
    });
    let result = store.insert_knowledge_entry(kb).await.expect("seed entity");
    result.entry_id
}

// ── entity patch ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_updates_title_and_bumps_version() {
    let d = LiveDaemon::start().await;
    let entity_id = seed_entity(&d).await;

    let out = d
        .cli(&[
            "creator",
            "world",
            "kb",
            "entity",
            "patch",
            "--world-id",
            "wld_test_world",
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "Renamed Hero",
        ])
        .await;
    assert!(out.status.success(), "patch failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("new version 1"), "{text}");
    assert!(text.contains("Renamed Hero"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_json_emits_dto_verbatim() {
    let d = LiveDaemon::start().await;
    let entity_id = seed_entity(&d).await;

    let out = d
        .cli(&[
            "creator",
            "world",
            "kb",
            "entity",
            "patch",
            "--world-id",
            "wld_test_world",
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "Json Hero",
            "--json",
        ])
        .await;
    assert!(out.status.success(), "patch failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["entity"]["key_block_id"], entity_id);
    assert_eq!(parsed["entity"]["canonical_name"], "Json Hero");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_stale_version_surfaces_conflict() {
    let d = LiveDaemon::start().await;
    let entity_id = seed_entity(&d).await;

    // Bump the entity to revision 1 via a first patch.
    let first = d
        .cli(&[
            "creator",
            "world",
            "kb",
            "entity",
            "patch",
            "--world-id",
            "wld_test_world",
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "First",
        ])
        .await;
    assert!(
        first.status.success(),
        "first patch failed: {}",
        stderr(&first)
    );

    // Replay with the stale version 0 → 409 world_kb_conflict.
    let out = d
        .cli(&[
            "creator",
            "world",
            "kb",
            "entity",
            "patch",
            "--world-id",
            "wld_test_world",
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "Stale",
        ])
        .await;
    assert!(!out.status.success(), "stale patch should fail");
    let err = stderr(&out);
    assert!(err.contains("world_kb_conflict"), "code missing: {err}");
    assert!(
        err.contains("current_version: 1"),
        "current_version missing: {err}"
    );
    assert!(err.contains(&entity_id), "entity_id missing: {err}");
    assert!(
        err.contains("recovery_hint"),
        "recovery hint missing: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_empty_patch_fails_fast() {
    let d = LiveDaemon::start().await;
    let entity_id = seed_entity(&d).await;

    let out = d
        .cli(&[
            "creator",
            "world",
            "kb",
            "entity",
            "patch",
            "--world-id",
            "wld_test_world",
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
        ])
        .await;
    assert!(!out.status.success(), "empty patch should fail");
    let err = stderr(&out);
    assert!(
        err.contains("at least one of"),
        "named fast-fail message missing: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_help_documents_expected_version_retry() {
    let d = LiveDaemon::start().await;
    let out = d
        .cli(&["creator", "world", "kb", "entity", "patch", "--help"])
        .await;
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("--expected-version"), "{text}");
    assert!(text.contains("world_kb_conflict"), "{text}");
    assert!(text.contains("refetch the graph"), "{text}");
}

// ── kb graph ──────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kb_graph_lists_entity_with_version() {
    let d = LiveDaemon::start().await;
    let entity_id = seed_entity(&d).await;

    let out = d
        .cli(&[
            "creator",
            "world",
            "kb",
            "graph",
            "--world-id",
            "wld_test_world",
        ])
        .await;
    assert!(out.status.success(), "graph failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(&entity_id), "{text}");
    assert!(text.contains("Hero"), "{text}");
    assert!(text.contains("1 entities"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kb_graph_json_emits_dto_verbatim() {
    let d = LiveDaemon::start().await;
    let entity_id = seed_entity(&d).await;

    let out = d
        .cli(&[
            "creator",
            "world",
            "kb",
            "graph",
            "--world-id",
            "wld_test_world",
            "--json",
        ])
        .await;
    assert!(out.status.success(), "graph failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let entities = parsed["entities"].as_array().expect("entities array");
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["key_block_id"], entity_id);
    assert_eq!(entities[0]["version"], 0);
}
