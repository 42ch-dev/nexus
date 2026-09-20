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
//! `anonymous_selection_is_refused_before_any_storage_write` defends the open
//! side of the same seam: a selected identity with no materialized workspace
//! (the anonymous bootstrap) is refused with the declared selection class
//! before the writer pool migrates anything.
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

/// The creator id the identity bootstrap reports
/// (`Created anonymous identity: ctr_anon…`).
fn anonymous_creator_id(out: &str) -> String {
    out.lines()
        .find_map(|line| line.split_once("anonymous identity: "))
        .map(|(_, id)| id.trim().to_string())
        .expect("the identity bootstrap reports the minted creator id")
}

/// A selection that names no materialized workspace is refused by the direct
/// core open seam with the declared selection refusal — never a raw storage
/// error, and never an implicitly materialized workspace.
///
/// `system identity create --kind anonymous` activates an ephemeral creator
/// without initializing a workspace (AR-88 #5). Every direct-core leaf opens
/// the core before it resolves anything else, so the open itself owes the
/// declared refusal ([`CoreError::AuthRequired`], mapped to the established
/// CLI selection class): reaching the writer pool's migration step would leak
/// `state.db.migration.lock: No such file or directory` and create the
/// workspace the identity was never given.
#[test]
fn anonymous_selection_is_refused_before_any_storage_write() {
    let home = tempfile::tempdir().expect("temp home");

    let created = assert_cmd::Command::cargo_bin("nexus42")
        .expect("nexus42 binary")
        .args(["system", "identity", "create", "--kind", "anonymous"])
        .env("HOME", home.path())
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn nexus42 identity create");
    assert!(
        created.status.success(),
        "the anonymous identity must be created: {}",
        stderr(&created)
    );
    let creator_id = anonymous_creator_id(&stdout(&created));

    let refused = assert_cmd::Command::cargo_bin("nexus42")
        .expect("nexus42 binary")
        .args(["creator", "world", "create", "--title", "Anon World"])
        .env("HOME", home.path())
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn nexus42 world create");
    assert!(
        !refused.status.success(),
        "an unmaterialized selection must be refused: {}",
        stdout(&refused)
    );
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));

    let refusal = stderr(&refused);
    assert!(
        refusal.contains("Creator not selected"),
        "the declared selection refusal: {refusal}"
    );
    for leak in ["migration.lock", "database_error", "No such file or directory"] {
        assert!(
            !refusal.contains(leak),
            "no raw storage I/O may leak ({leak}): {refusal}"
        );
    }

    // AR-88 #5: `select_workspace` is the only path that materializes a
    // workspace, so the refused command must leave no workspace behind.
    let workspace_db =
        nexus_home_layout::workspace_state_db_path(home.path(), &creator_id, "default");
    let workspace_dir = workspace_db.parent().expect("workspace dir");
    assert!(
        !workspace_dir.exists(),
        "no workspace may be materialized: {}",
        workspace_dir.display()
    );
}
