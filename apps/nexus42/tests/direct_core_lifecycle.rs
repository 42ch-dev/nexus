//! Direct-core lifetime tests for `nexus42` (v1.193 P0-T1).
//!
//! These run the REAL binary against an isolated raw `HOME` seeded through the
//! direct core seam only — no daemon, no Node, no live provider (the fixture in
//! `common/direct.rs` is deliberately server-free).
//!
//! `direct_writer_reopens_after_rejected_mutation` defends the shared
//! `finish_direct` lifetime: a rejected mutation must still close the direct
//! writer, so the next CLI writer opens and commits instead of finding the
//! workspace held or the entity half-written.
//!
//! Every mutation here runs in its own short-lived child, so process exit would
//! release a writer the seam forgot to close. The in-process half of the same
//! contract — the writer is released before `finish_direct` returns — lives in
//! `src/core.rs` (`direct_writer_lifetime`).

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_contracts::world_kb_patch_entity_request::{
    NexusWorldKbEntityPatch, NexusWorldKbEntityPatchBlockType, NexusWorldKbEntityPatchTitle,
};
use nexus_contracts::{
    CreateWorldRequest, WorldKbPatchEntityRequest, WorldKbPatchEntityResponse,
};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use std::path::Path;
use std::process::Output;

const WORLD_TITLE: &str = "Direct Core Lifecycle";
const ENTITY_ID: &str = "kb_0f1e2d3c4b5a";
const SEEDED_TITLE: &str = "Seeded Hero";
const STALE_TITLE: &str = "Stale Overwrite";
const REOPENED_TITLE: &str = "Reopened Hero";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Seed one owned World plus one KB entity through the direct core seam, then
/// close that seed core so no writer survives into the CLI children. Returns
/// the world id and the revision the created entity settled at.
async fn seed_world_and_entity(home: &Path) -> (String, u64) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.to_path_buf(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("seed core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    let world_id = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({ "title": WORLD_TITLE }))
                .expect("world request shape"),
        )
        .await
        .expect("create world")
        .world_id;

    let patch = NexusWorldKbEntityPatch {
        title: Some(
            NexusWorldKbEntityPatchTitle::try_from(SEEDED_TITLE.to_string())
                .expect("valid canonical name"),
        ),
        block_type: Some(NexusWorldKbEntityPatchBlockType::Character),
        ..NexusWorldKbEntityPatch::default()
    };
    let seeded: WorldKbPatchEntityResponse = core
        .patch_world_kb_entity(
            &principal,
            world_id.clone(),
            WorldKbPatchEntityRequest {
                entity_id: ENTITY_ID.to_string(),
                expected_version: 0,
                patch,
            },
        )
        .await
        .expect("seed entity");
    let seeded_version = seeded.version;
    assert!(
        seeded_version > 0,
        "a created entity settles at a replayable revision"
    );

    core.close().await.expect("seed core closes");
    (world_id, seeded_version)
}

/// One `key_block_id` / `version` / `canonical_name` snapshot of the graph.
fn graph_entity(fixture: &DirectFixture, world_id: &str) -> (String, u64, String) {
    let out = fixture
        .command()
        .args([
            "creator",
            "world",
            "kb",
            "graph",
            "--world-id",
            world_id,
            "--json",
        ])
        .output()
        .expect("spawn nexus42 graph");
    assert!(out.status.success(), "graph failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid graph JSON");
    let entities = parsed["entities"].as_array().expect("entities array");
    assert_eq!(entities.len(), 1, "exactly the seeded entity: {parsed}");
    let entity = &entities[0];
    (
        entity["key_block_id"]
            .as_str()
            .expect("key_block_id")
            .to_string(),
        entity["version"].as_u64().expect("version"),
        entity["canonical_name"]
            .as_str()
            .expect("canonical_name")
            .to_string(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_writer_reopens_after_rejected_mutation() {
    let fixture = DirectFixture::new().await;
    let (world_id, seeded_version) = seed_world_and_entity(fixture.home.path()).await;
    let stale_version = seeded_version - 1;

    // ── 1. A real CLI mutation with a stale CAS is rejected. ──────────────
    let rejected = fixture
        .command()
        .args([
            "creator",
            "world",
            "kb",
            "entity",
            "patch",
            "--world-id",
            &world_id,
            "--entity-id",
            ENTITY_ID,
            "--expected-version",
            &stale_version.to_string(),
            "--title",
            STALE_TITLE,
        ])
        .output()
        .expect("spawn nexus42 patch");
    assert!(
        !rejected.status.success(),
        "stale patch must be rejected: {}",
        stdout(&rejected)
    );
    assert_eq!(
        rejected.status.code(),
        Some(76),
        "world_kb_conflict exit code: {}",
        stderr(&rejected)
    );
    let conflict = stderr(&rejected);
    assert!(conflict.contains("world_kb_conflict"), "{conflict}");
    assert!(
        conflict.contains(&format!("expected_version: {stale_version}")),
        "{conflict}"
    );

    // ── 2. Rejected without change: the graph still shows the seeded row. ──
    let (entity_id, version, canonical_name) = graph_entity(&fixture, &world_id);
    assert_eq!(entity_id, ENTITY_ID);
    assert_eq!(
        version, seeded_version,
        "rejected mutation must not move the revision"
    );
    assert_eq!(
        canonical_name, SEEDED_TITLE,
        "rejected mutation must not overwrite the canonical name"
    );

    // ── 3. The next CLI writer reopens and commits. ───────────────────────
    let committed_version = seeded_version + 1;
    let reopened = fixture
        .command()
        .args([
            "creator",
            "world",
            "kb",
            "entity",
            "patch",
            "--world-id",
            &world_id,
            "--entity-id",
            ENTITY_ID,
            "--expected-version",
            &seeded_version.to_string(),
            "--title",
            REOPENED_TITLE,
        ])
        .output()
        .expect("spawn nexus42 patch");
    assert!(
        reopened.status.success(),
        "a writer must reopen after a rejected mutation: {}",
        stderr(&reopened)
    );
    let text = stdout(&reopened);
    assert!(
        text.contains(&format!("new version {committed_version}")),
        "{text}"
    );
    assert!(text.contains(REOPENED_TITLE), "{text}");

    // ── 4. The committed change is the replayed next revision. ────────────
    let (entity_id, version, canonical_name) = graph_entity(&fixture, &world_id);
    assert_eq!(entity_id, ENTITY_ID);
    assert_eq!(version, committed_version, "the reopened writer commits");
    assert_eq!(canonical_name, REOPENED_TITLE);
}
