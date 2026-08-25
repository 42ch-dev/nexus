//! Hermetic CLI integration tests — `creator world fork create|list`
//! (V1.175 P1 Task 1, group 5): `create` over the existing
//! `POST /v1/daemon/worlds/:world_id/forks` route and `list` as a **pure
//! projection** of the timeline-events read (`extensions.fork_lineage`,
//! branch-scoped per V1.162 carrier B), end-to-end against a live daemon
//! fixture with hermetic `HOME` (AR-83 #6 / AR-84 group 5 — no new read
//! route).

mod common;

use common::LiveDaemon;
use nexus_local_db::narrative_write;
use serde_json::Value;
use std::process::Output;

/// Seed an owned World with one parent-branch event; returns
/// `(world_id, parent_branch_id, fork_point_event_id)`.
async fn seed_world_with_fork_point(d: &LiveDaemon) -> (String, String, String) {
    let w = narrative_write::create_world(
        &d.pool,
        "test_creator",
        "Fork CLI Test",
        "fork-cli-test",
        "private",
        "manual",
    )
    .await
    .expect("create world");
    let evt = narrative_write::append_event(
        &d.pool,
        &w.world_id,
        &w.root_fork_branch_id,
        "story_advance",
        Some("Parent event"),
        None,
        None,
    )
    .await
    .expect("append parent event");
    (w.world_id, w.root_fork_branch_id, evt.event_id)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_and_list_projection() {
    let d = LiveDaemon::start().await;
    let (world_id, parent_branch, fork_point) = seed_world_with_fork_point(&d).await;

    // Create a fork with an explicit label; the parent branch is derived
    // from the fork-point event's branch via the timeline-events read.
    let create = d
        .cli(&[
            "creator",
            "world",
            "fork",
            "create",
            &world_id,
            "--fork-point",
            &fork_point,
            "--label",
            "alt-ending",
        ])
        .await;
    assert!(
        create.status.success(),
        "fork create failed: {}",
        stderr(&create)
    );
    let created = stdout(&create);
    assert!(created.contains("Fork created"), "{created}");
    assert!(created.contains(&parent_branch), "{created}");
    assert!(created.contains(&fork_point), "{created}");
    // The new fork branch id is printed by create — read it back.
    let branch_id = created
        .lines()
        .find_map(|line| line.trim().strip_prefix("branch_id:"))
        .map(str::trim)
        .expect("branch_id in create output")
        .to_string();
    assert!(branch_id.starts_with("fbk_"), "{branch_id}");

    // List with --branch = pure projection of the fork_created marker.
    let list = d
        .cli(&[
            "creator", "world", "fork", "list", &world_id, "--branch", &branch_id, "--json",
        ])
        .await;
    assert!(list.status.success(), "fork list failed: {}", stderr(&list));
    let json: Value = serde_json::from_str(&stdout(&list)).expect("json fork list");
    let markers = json.as_array().expect("markers array");
    assert_eq!(markers.len(), 1, "one fork marker: {json}");
    let marker = &markers[0];
    assert_eq!(marker["branch_id"], branch_id);
    assert_eq!(marker["parent_branch_id"], parent_branch);
    assert_eq!(marker["forked_from_event_id"], fork_point);
    assert_eq!(marker["label"], "alt-ending");

    // Human list renders the same projection.
    let human = d
        .cli(&[
            "creator", "world", "fork", "list", &world_id, "--branch", &branch_id,
        ])
        .await;
    assert!(human.status.success());
    let text = stdout(&human);
    assert!(text.contains(&parent_branch), "{text}");
    assert!(text.contains("alt-ending"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_list_empty_world() {
    let d = LiveDaemon::start().await;
    let w = narrative_write::create_world(
        &d.pool,
        "test_creator",
        "Empty Fork World",
        "empty-fork-world",
        "private",
        "manual",
    )
    .await
    .expect("create world");

    let out = d
        .cli(&["creator", "world", "fork", "list", &w.world_id])
        .await;
    assert!(
        out.status.success(),
        "fork list empty failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(
        text.contains("No fork marker") && text.contains("--branch"),
        "root branch carries no marker and help must name --branch: {text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_bad_fork_point_surfaces_daemon_422() {
    let d = LiveDaemon::start().await;
    let (world_id, parent_branch, _) = seed_world_with_fork_point(&d).await;

    // Pass an explicit --parent-branch so the CLI derivation is skipped and
    // the daemon's fork-point validation rejects the unknown event (422).
    let out = d
        .cli(&[
            "creator",
            "world",
            "fork",
            "create",
            &world_id,
            "--fork-point",
            "evt_does_not_exist",
            "--parent-branch",
            &parent_branch,
        ])
        .await;
    assert!(!out.status.success(), "bad fork-point must fail");
    let err = stderr(&out);
    assert!(
        err.contains("422") || err.contains("invalid_input"),
        "stderr should surface the daemon 422: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_unknown_fork_point_derivation_fails_closed() {
    let d = LiveDaemon::start().await;
    let (world_id, _, _) = seed_world_with_fork_point(&d).await;

    // No --parent-branch: the CLI derives it from the timeline projection,
    // cannot find the event, and fails closed with remediation.
    let out = d
        .cli(&[
            "creator",
            "world",
            "fork",
            "create",
            &world_id,
            "--fork-point",
            "evt_missing",
        ])
        .await;
    assert!(!out.status.success(), "unresolvable fork-point must fail");
    let err = stderr(&out);
    assert!(
        err.contains("--parent-branch"),
        "stderr should name the remediation flag: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_foreign_world_rejected_403() {
    let d = LiveDaemon::start().await;
    let (_owner_world, _, owner_fork_point) = seed_world_with_fork_point(&d).await;

    // A foreign World owned by another creator (ownership-gate fixture).
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
           VALUES ('wld_fork_foreign', 'ws', 'other_creator', 'Foreign', 'foreign', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(&d.pool)
    .await
    .expect("seed foreign world");

    let out = d
        .cli(&[
            "creator",
            "world",
            "fork",
            "create",
            "wld_fork_foreign",
            "--fork-point",
            &owner_fork_point,
            "--parent-branch",
            "fbk_any",
        ])
        .await;
    assert!(!out.status.success(), "foreign world must fail");
    let err = stderr(&out);
    assert!(
        err.contains("403") || err.to_lowercase().contains("forbidden"),
        "stderr should surface the 403: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_list_unknown_world_surfaces_daemon_error() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&["creator", "world", "fork", "list", "wld_does_not_exist"])
        .await;
    assert!(!out.status.success(), "unknown world must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the daemon 404: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_json_emits_dto() {
    let d = LiveDaemon::start().await;
    let (world_id, parent_branch, fork_point) = seed_world_with_fork_point(&d).await;

    let out = d
        .cli(&[
            "creator",
            "world",
            "fork",
            "create",
            &world_id,
            "--fork-point",
            &fork_point,
            "--label",
            "alt-ending",
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "fork create --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json fork create");
    let branch_id = json["branch_id"].as_str().expect("branch_id");
    assert!(branch_id.starts_with("fbk_"), "{branch_id}");
    assert_eq!(json["parent_branch_id"], parent_branch);
    assert_eq!(json["forked_from_event_id"], fork_point);
}
