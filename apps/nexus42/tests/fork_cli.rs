//! Server-free CLI integration tests — `creator world fork create|list`
//! (V1.175 P1 Task 1, group 5; direct-core retarget v1.193 P0-T3).
//!
//! `create` runs on `CoreService::create_fork` and `list` is a **pure
//! projection** of the core timeline-events read (`extensions.fork_lineage`,
//! branch-scoped per V1.162 carrier B). Both run end-to-end against the
//! real `nexus42` binary over an isolated raw `HOME` — no daemon fixture, no
//! Node child, no live provider (AR-83 #6 / AR-84 group 5 — no new read).
//!
//! The family seeds its own World through the CLI itself: `creator world
//! create` for the World and the retained local narrative writer behind
//! `creator world event-add` for the fork-point event. Only the foreign-World
//! ownership row needs a direct state-DB seed.

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_home_layout::{nexus_root_from_home, workspace_state_db_path};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::path::PathBuf;
use std::process::Output;

/// The fixture's workspace slug (mirrors `common/direct.rs`).
const WORKSPACE_SLUG: &str = "default";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The payload of the first output line carrying `label`.
fn labelled(out: &str, label: &str) -> String {
    out.lines()
        .find_map(|line| line.split_once(label).map(|(_, rest)| rest.trim()))
        .map_or_else(
            || panic!("no '{label}' line in output:\n{out}"),
            str::to_string,
        )
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under `~/.nexus42/creators/`.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
    let mut entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .pop()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}

/// The fixture's workspace state DB (`.../creators/<id>/workspaces/default/state.db`).
fn fixture_state_db(fixture: &DirectFixture) -> PathBuf {
    workspace_state_db_path(
        fixture.home.path(),
        &fixture_creator_id(fixture),
        WORKSPACE_SLUG,
    )
}

/// Seed an owned World with one parent-branch event through the REAL CLI
/// (create + the retained local narrative writer); returns
/// `(world_id, parent_branch_id, fork_point_event_id)`.
fn seed_world_with_fork_point(fixture: &DirectFixture) -> (String, String, String) {
    let create = fixture
        .command()
        .args(["creator", "world", "create", "--title", "Fork CLI Test"])
        .output()
        .expect("spawn nexus42 world create");
    assert!(
        create.status.success(),
        "world create failed: {}",
        stderr(&create)
    );
    let world_id = labelled(&stdout(&create), "World created:");
    assert!(world_id.starts_with("wld_"), "{world_id}");

    let add = fixture
        .command()
        .args([
            "creator",
            "world",
            "event-add",
            "--world-id",
            &world_id,
            "--event-type",
            "story_advance",
            "--title",
            "Parent event",
        ])
        .output()
        .expect("spawn nexus42 event-add");
    assert!(add.status.success(), "event-add failed: {}", stderr(&add));
    let text = stdout(&add);
    let fork_point = labelled(&text, "Event added:");
    let parent_branch = labelled(&text, "Branch:");
    assert!(parent_branch.starts_with("fbk_"), "{parent_branch}");
    (world_id, parent_branch, fork_point)
}

/// Seed a World owned by another creator into the fixture's workspace DB.
///
/// The core ownership guard compares `narrative_worlds.owner_creator_id`
/// against the admitted principal, so the foreign row must live in the SAME
/// state DB. The seed writer is released before any CLI child is spawned —
/// drop alone is not proof of release.
async fn seed_foreign_world(fixture: &DirectFixture, world_id: &str) {
    let db_path = fixture_state_db(fixture);
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("open fixture state db")
        .clone_pool();
    nexus_local_db::kb_store::seed::world(
        &pool,
        world_id,
        "ctr_foreign_owner",
        "Foreign Fork World",
        "foreign-fork-world",
        "private",
        "manual",
    )
    .await;
    pool.close().await;
    release_retained_writer_guards(&db_path);
}

/// Run one `creator world fork` invocation against the fixture home.
fn fork_cli(fixture: &DirectFixture, args: &[&str]) -> Output {
    let mut full = vec!["creator", "world", "fork"];
    full.extend_from_slice(args);
    fixture
        .command()
        .args(&full)
        .output()
        .expect("spawn nexus42 fork")
}

/// `create` then `list --branch`: the projection returns the created fork
/// marker with its derived parent branch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_and_list_projection() {
    let fixture = DirectFixture::new().await;
    let (world_id, parent_branch, fork_point) = seed_world_with_fork_point(&fixture);

    // Create a fork with an explicit label; the parent branch is derived
    // from the fork-point event's branch via the timeline-events read.
    let create = fork_cli(
        &fixture,
        &[
            "create",
            &world_id,
            "--fork-point",
            &fork_point,
            "--label",
            "alt-ending",
        ],
    );
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
    let list = fork_cli(
        &fixture,
        &["list", &world_id, "--branch", &branch_id, "--json"],
    );
    assert!(list.status.success(), "fork list failed: {}", stderr(&list));
    let json: serde_json::Value = serde_json::from_str(&stdout(&list)).expect("json fork list");
    let markers = json.as_array().expect("markers array");
    assert_eq!(markers.len(), 1, "one fork marker: {json}");
    let marker = &markers[0];
    assert_eq!(marker["branch_id"], branch_id);
    assert_eq!(marker["parent_branch_id"], parent_branch);
    assert_eq!(marker["forked_from_event_id"], fork_point);
    assert_eq!(marker["label"], "alt-ending");

    // Human list renders the same projection.
    let human = fork_cli(&fixture, &["list", &world_id, "--branch", &branch_id]);
    assert!(human.status.success());
    let text = stdout(&human);
    assert!(text.contains(&parent_branch), "{text}");
    assert!(text.contains("alt-ending"), "{text}");
}

/// A World with no fork carries no marker on its current branch, and the
/// message names `--branch` as the way to read a fork branch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_list_empty_world() {
    let fixture = DirectFixture::new().await;
    let (world_id, _, _) = seed_world_with_fork_point(&fixture);

    let out = fork_cli(&fixture, &["list", &world_id]);
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

/// A fork point that is not on the given parent branch is rejected by the
/// core's typed input validation (non-zero exit, no marker written).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_bad_fork_point_rejected_as_input_error() {
    let fixture = DirectFixture::new().await;
    let (world_id, parent_branch, _) = seed_world_with_fork_point(&fixture);

    // Pass an explicit --parent-branch so the CLI derivation is skipped and
    // the core's fork-point validation rejects the unknown event.
    let out = fork_cli(
        &fixture,
        &[
            "create",
            &world_id,
            "--fork-point",
            "evt_does_not_exist",
            "--parent-branch",
            &parent_branch,
        ],
    );
    assert!(!out.status.success(), "bad fork-point must fail");
    let err = stderr(&out);
    assert!(
        err.contains("invalid input (fork_point)"),
        "stderr should carry the core's typed fork-point refusal: {err}"
    );
    assert!(
        !err.to_lowercase().contains("daemon"),
        "no daemon transport may be involved: {err}"
    );
}

/// Without `--parent-branch` the CLI derives it from the timeline projection,
/// cannot find the event, and fails closed naming the remediation flag.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_unknown_fork_point_derivation_fails_closed() {
    let fixture = DirectFixture::new().await;
    let (world_id, _, _) = seed_world_with_fork_point(&fixture);

    let out = fork_cli(
        &fixture,
        &["create", &world_id, "--fork-point", "evt_missing"],
    );
    assert!(!out.status.success(), "unresolvable fork-point must fail");
    let err = stderr(&out);
    assert!(
        err.contains("--parent-branch"),
        "stderr should name the remediation flag: {err}"
    );
}

/// A foreign World is refused by the core ownership guard (403 family) — the
/// held classification survives without an HTTP status transport.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_foreign_world_rejected_403() {
    let fixture = DirectFixture::new().await;
    let (_owner_world, _, owner_fork_point) = seed_world_with_fork_point(&fixture);
    seed_foreign_world(&fixture, "wld_fork_foreign").await;

    let out = fork_cli(
        &fixture,
        &[
            "create",
            "wld_fork_foreign",
            "--fork-point",
            &owner_fork_point,
            "--parent-branch",
            "fbk_any",
        ],
    );
    assert!(!out.status.success(), "foreign world must fail");
    let err = stderr(&out);
    assert!(
        err.contains("403") || err.to_lowercase().contains("forbidden"),
        "stderr should surface the 403 family: {err}"
    );
    assert!(
        err.contains("wld_fork_foreign"),
        "the refusal names the World: {err}"
    );
}

/// `fork list` on an unknown World surfaces the core's `not_found` (404)
/// family.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_list_unknown_world_surfaces_not_found() {
    let fixture = DirectFixture::new().await;

    let out = fork_cli(&fixture, &["list", "wld_does_not_exist"]);
    assert!(!out.status.success(), "unknown world must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the 404 family: {err}"
    );
}

/// `fork create --json` emits the created fork DTO verbatim.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_json_emits_dto() {
    let fixture = DirectFixture::new().await;
    let (world_id, parent_branch, fork_point) = seed_world_with_fork_point(&fixture);

    let out = fork_cli(
        &fixture,
        &[
            "create",
            &world_id,
            "--fork-point",
            &fork_point,
            "--label",
            "alt-ending",
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "fork create --json failed: {}",
        stderr(&out)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("json fork create");
    let branch_id = json["branch_id"].as_str().expect("branch_id");
    assert!(branch_id.starts_with("fbk_"), "{branch_id}");
    assert_eq!(json["parent_branch_id"], parent_branch);
    assert_eq!(json["forked_from_event_id"], fork_point);
}
