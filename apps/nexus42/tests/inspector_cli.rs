//! Hermetic CLI integration tests — `creator inspector` (V1.175 P1 Task 1,
//! group 6): hidden debug group; `moment` prints the observe-only inspector
//! packet / `--json` DTO against a live daemon fixture with hermetic `HOME`
//! (AR-83 #6 / AR-84 group 6). Also pins PL-6: absent from `creator --help`.

mod common;

use axum::extract::State;
use axum::Json;
use common::LiveDaemon;
use nexus_daemon_runtime::api::handlers::works::{create_work, CreateWorkRequest};
use serde_json::Value;
use std::process::Output;

/// Seed one owned Work bound to the fixture's `wld_test_world`.
async fn seed_work(d: &LiveDaemon) -> String {
    let (_, Json(resp)) = create_work(
        State(d.state.clone()),
        Json(CreateWorkRequest {
            title: "Inspector Test Novel".to_string(),
            long_term_goal: "Write".to_string(),
            initial_idea: "Idea".to_string(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
            work_profile: Some("novel".to_string()),
        }),
    )
    .await
    .expect("seed work via daemon handler");
    resp.work_id
}

/// Seed the deterministic inspector happy path: an owned World plus one
/// `constant`-activation lore row (always fires → non-empty placement).
async fn seed_inspector_world(d: &LiveDaemon) -> String {
    // SAFETY: test-only seed against the known kb_key_blocks schema
    // (same fixture shape as nexus-daemon-runtime/tests/inspector_api.rs).
    sqlx::query(
        "INSERT OR IGNORE INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, created_at, modules_json) \
           VALUES (?, 'wld_test_world', 'character', 'Harbor Master', 'confirmed', datetime('now'), \
             '{\"activation\":{\"keys\":[],\"constant\":true}}')",
    )
    .bind("kbl_inspector_lore")
    .execute(&d.pool)
    .await
    .expect("seed inspector lore");
    "wld_test_world".to_string()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_moment_prints_human_packet() {
    let d = LiveDaemon::start().await;
    let world_id = seed_inspector_world(&d).await;

    let out = d.cli(&["creator", "inspector", "moment", &world_id]).await;
    assert!(
        out.status.success(),
        "inspector moment failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("Inspector moment"), "{text}");
    assert!(text.contains("modules: placement="), "{text}");
    assert!(text.contains("budget: primary="), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_moment_json_emits_dto() {
    let d = LiveDaemon::start().await;
    let world_id = seed_inspector_world(&d).await;

    let out = d
        .cli(&["creator", "inspector", "moment", &world_id, "--json"])
        .await;
    assert!(
        out.status.success(),
        "inspector moment --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json packet");
    assert!(json["budget"].is_object(), "{json}");
    assert!(json["modules"].is_object(), "{json}");
    assert!(json["moment_directive"].is_object(), "{json}");
    assert!(json["slot_map"].is_array(), "{json}");
    assert_eq!(
        json["modules"]["placement"][0]["canonical_name"],
        "Harbor Master"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_moment_supports_work_and_stage_flags() {
    let d = LiveDaemon::start().await;
    let world_id = seed_inspector_world(&d).await;
    let work_id = seed_work(&d).await;

    let out = d
        .cli(&[
            "creator",
            "inspector",
            "moment",
            &world_id,
            "--work",
            &work_id,
            "--stage",
            "produce",
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "inspector with --work/--stage failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json packet");
    assert!(json["modules"].is_object(), "{json}");
    // No directive is seeded, so the packet's moment_directive is the
    // "none" status shape — success itself proves the daemon accepted the
    // work→world binding (a mismatch surfaces 400).
    assert_eq!(json["moment_directive"]["status"], "none", "{json}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_invalid_stage_rejected() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&[
            "creator",
            "inspector",
            "moment",
            "wld_test_world",
            "--stage",
            "bogus",
        ])
        .await;
    assert!(!out.status.success(), "invalid --stage must fail");
    assert!(
        stderr(&out).contains("--stage"),
        "stderr should name --stage: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_foreign_world_rejected_403() {
    let d = LiveDaemon::start().await;
    // Seed a World owned by a *different* creator (ownership-gate fixture).
    // SAFETY: test-only seed against the known creators/narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(&d.pool)
    .await
    .expect("seed other creator");
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES ('wld_foreign', 'ws', 'other_creator', 'Foreign', 'foreign', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(&d.pool)
    .await
    .expect("seed foreign world");

    let out = d
        .cli(&["creator", "inspector", "moment", "wld_foreign"])
        .await;
    assert!(!out.status.success(), "foreign world must fail");
    let err = stderr(&out);
    assert!(
        err.contains("403") || err.to_lowercase().contains("forbidden"),
        "stderr should surface the 403: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_hidden_from_creator_help_but_usable() {
    let d = LiveDaemon::start().await;

    // PL-6: `creator --help` must NOT list the hidden inspector group.
    let help = d.cli(&["creator", "--help"]).await;
    assert!(help.status.success());
    let text = stdout(&help);
    assert!(
        !text.to_lowercase().contains("inspector"),
        "inspector must be hidden from creator --help:\n{text}"
    );
    assert!(
        text.contains("reading"),
        "reading should be visible:\n{text}"
    );

    // But the hidden group still resolves its own help.
    let hidden = d.cli(&["creator", "inspector", "--help"]).await;
    assert!(
        hidden.status.success(),
        "creator inspector --help must work"
    );
    assert!(stdout(&hidden).contains("moment"));
}
