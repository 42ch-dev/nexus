//! Direct-core CLI tests — `creator reading` (V1.175 P1 Task 1, group 3;
//! direct-core retarget v1.193 P0-T8): progress `get|set|clear` + annotation
//! `list|add|patch|remove` end-to-end against a hermetic direct-core home —
//! no daemon, no Node child (`common/direct.rs` precedent).
//!
//! Each test seeds one owned Work through the core, releases every seed
//! writer, then drives the REAL `nexus42` binary. Failure paths: out-of-range
//! `--scroll`, invalid `--color`, and a non-existent Work (core 404).

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_contracts::CreateWorkRequest;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_home_layout::{nexus_root_from_home, operational_workspace_dir, workspace_state_db_path};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::path::Path;
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";
/// Deterministic `story_ref` for the seeded Work.
const STORY_REF: &str = "reading-test-novel";

/// A hermetic direct-core home with one owned Work and no other rows.
struct ReadingEnv {
    fixture: DirectFixture,
    work_id: String,
}

impl ReadingEnv {
    /// Run the real `nexus42` binary against this fixture's hermetic `HOME`.
    fn cli(&self, args: &[&str]) -> Output {
        self.fixture
            .command()
            .args(args)
            .output()
            .expect("spawn nexus42")
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Seed one owned novel Work through the core, then release the seed writer so
/// the CLI child can admit its own.
async fn fresh_env() -> ReadingEnv {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(fixture.home.path());
    write_workspace_meta(fixture.home.path(), &creator_id);
    let db_path = workspace_state_db_path(fixture.home.path(), &creator_id, WORKSPACE_SLUG);

    let core = CoreService::open(CoreOpenOptions {
        user_home: fixture.home.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("seed core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    // A Work binds to an owned World (V1.40+), so the World comes first.
    let world_id = core
        .create_world(
            &principal,
            serde_json::from_value(serde_json::json!({
                "title": "Reading Test World"
            }))
            .expect("world request shape"),
        )
        .await
        .expect("seed world")
        .world_id;
    let work_id = core
        .create_work(
            &principal,
            CreateWorkRequest {
                client_request_id: None,
                initial_idea: "A test story".to_string(),
                lineage_from_work_id: None,
                long_term_goal: "Test reading progress".to_string(),
                primary_preset_id: None,
                set_pool_active: None,
                story_ref: Some(STORY_REF.to_string()),
                title: "Reading Test Novel".to_string(),
                work_profile: Some("novel".to_string()),
                world_id: Some(world_id),
            },
        )
        .await
        .expect("seed work")
        .work_id;
    core.close().await.expect("seed core closes");

    // The selection above retains the writer admission in THIS process; a CLI
    // child must not be handed a home another writer still holds.
    release_retained_writer_guards(&db_path);

    ReadingEnv { fixture, work_id }
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under the nexus root's `creators/`.
fn fixture_creator_id(home: &Path) -> String {
    let creators_root = nexus_root_from_home(home).join("creators");
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

/// The core resolves workspace files against the operational `meta.json`
/// `local_root` — the same key the CLI's own workspace registration writes.
fn write_workspace_meta(home: &Path, creator_id: &str) {
    let creative_root = home.join("creative");
    std::fs::create_dir_all(&creative_root).expect("materialize creative root");
    std::fs::write(
        operational_workspace_dir(home, creator_id, WORKSPACE_SLUG).join("meta.json"),
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "creator_id": creator_id,
            "workspace_slug": WORKSPACE_SLUG,
            "local_root": creative_root,
            "created_at": "2020-01-01T00:00:00Z"
        }))
        .expect("meta json"),
    )
    .expect("write meta.json");
}

// ── Progress ───────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_set_get_round_trip() {
    let env = fresh_env().await;
    let work_id = env.work_id.as_str();

    let set = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            work_id,
            "--chapter",
            "3",
            "--scroll",
            "7500",
        ]);
    assert!(set.status.success(), "set failed: {}", stderr(&set));
    assert!(
        stdout(&set).contains("Saved reading progress."),
        "{}",
        stdout(&set)
    );
    assert!(stdout(&set).contains("7500"), "{}", stdout(&set));

    let get = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "get",
            work_id,
            "--chapter",
            "3",
        ]);
    assert!(get.status.success(), "get failed: {}", stderr(&get));
    assert!(stdout(&get).contains("7500"), "{}", stdout(&get));
    assert!(stdout(&get).contains(work_id), "{}", stdout(&get));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_set_json_emits_dto_verbatim() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            &env.work_id,
            "--chapter",
            "2",
            "--scroll",
            "1234",
            "--json",
        ]);
    assert!(out.status.success(), "set --json failed: {}", stderr(&out));
    let json: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["work_id"], env.work_id);
    assert_eq!(json["chapter"], 2);
    assert_eq!(json["scroll_progress"], 1234);
    assert!(json["updated_at"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_clear_removes_row() {
    let env = fresh_env().await;
    let work_id = env.work_id.as_str();

    let _ = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            work_id,
            "--chapter",
            "1",
            "--scroll",
            "500",
        ]);
    let clear = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "clear",
            work_id,
            "--chapter",
            "1",
        ]);
    assert!(clear.status.success(), "clear failed: {}", stderr(&clear));
    assert!(stdout(&clear).contains("Cleared"), "{}", stdout(&clear));

    // After the clear the core reports the default (0) progress.
    let get = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "get",
            work_id,
            "--chapter",
            "1",
            "--json",
        ]);
    assert!(
        get.status.success(),
        "get after clear failed: {}",
        stderr(&get)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout(&get)).expect("json output");
    assert_eq!(json["scroll_progress"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_clear_json_prints_empty_stdout() {
    let env = fresh_env().await;
    let work_id = env.work_id.as_str();

    let _ = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            work_id,
            "--chapter",
            "2",
            "--scroll",
            "1000",
        ]);
    let out = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "clear",
            work_id,
            "--chapter",
            "2",
            "--json",
        ]);
    assert!(
        out.status.success(),
        "clear --json failed: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).trim().is_empty(),
        "--json delete prints empty stdout: {}",
        stdout(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_clear_unknown_work_surfaces_core_error() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "clear",
            "wrk_does_not_exist",
            "--chapter",
            "1",
        ]);
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the 404 not_found: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_set_rejects_out_of_range_scroll() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            &env.work_id,
            "--chapter",
            "1",
            "--scroll",
            "20000",
        ]);
    assert!(!out.status.success(), "out-of-range scroll must fail");
    assert!(
        stderr(&out).contains("--scroll"),
        "stderr should name --scroll: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("0..=10000"),
        "stderr should name the valid range: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_get_unknown_work_surfaces_core_error() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "progress",
            "get",
            "wrk_does_not_exist",
            "--chapter",
            "1",
        ]);
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the 404 not_found: {err}"
    );
}

// ── Annotations ────────────────────────────────────────────────────────────

// The full add→list→patch→list→remove→list journey is one linear scenario;
// splitting it would hide the cross-verb state transitions it pins.
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_lifecycle_round_trip() {
    let env = fresh_env().await;
    let work_id = env.work_id.as_str();

    let add = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            work_id,
            "--chapter",
            "5",
            "--start",
            "10",
            "--end",
            "22",
            "--selected-text",
            "the gate groaned",
            "--color",
            "yellow",
            "--note",
            "check pacing",
        ]);
    assert!(add.status.success(), "add failed: {}", stderr(&add));
    let added = stdout(&add);
    assert!(added.contains("Created annotation"), "{added}");

    let list = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            work_id,
            "--chapter",
            "5",
            "--json",
        ]);
    assert!(list.status.success(), "list failed: {}", stderr(&list));
    let json: serde_json::Value = serde_json::from_str(&stdout(&list)).expect("json list");
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "one annotation: {json}");
    let annotation_id = items[0]["annotation_id"].as_str().expect("annotation id");
    assert!(annotation_id.starts_with("ann_"), "{annotation_id}");
    assert_eq!(items[0]["selected_text"], "the gate groaned");
    assert_eq!(items[0]["color"], "yellow");
    assert_eq!(items[0]["note"], "check pacing");

    let patch = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "patch",
            annotation_id,
            "--color",
            "pink",
            "--note",
            "rewritten",
        ]);
    assert!(patch.status.success(), "patch failed: {}", stderr(&patch));

    let list2 = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            work_id,
            "--chapter",
            "5",
            "--json",
        ]);
    let json2: serde_json::Value = serde_json::from_str(&stdout(&list2)).expect("json list 2");
    let item2 = &json2["items"][0];
    assert_eq!(item2["color"], "pink");
    assert_eq!(item2["note"], "rewritten");

    let remove = env
        .cli(&["creator", "reading", "annotation", "remove", annotation_id]);
    assert!(
        remove.status.success(),
        "remove failed: {}",
        stderr(&remove)
    );
    assert!(
        stdout(&remove).contains("Removed annotation"),
        "{}",
        stdout(&remove)
    );

    let list3 = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            work_id,
            "--chapter",
            "5",
        ]);
    assert!(list3.status.success());
    assert!(
        stdout(&list3).contains("No annotations"),
        "{}",
        stdout(&list3)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_add_json_emits_dto() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &env.work_id,
            "--chapter",
            "1",
            "--start",
            "0",
            "--end",
            "5",
            "--selected-text",
            "text",
            "--color",
            "blue",
            "--json",
        ]);
    assert!(out.status.success(), "add --json failed: {}", stderr(&out));
    let json: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("json annotation");
    let annotation_id = json["annotation_id"].as_str().expect("annotation id");
    assert!(annotation_id.starts_with("ann_"), "{annotation_id}");
    assert_eq!(json["color"], "blue");
    assert_eq!(json["selected_text"], "text");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_patch_json_emits_dto() {
    let env = fresh_env().await;

    let add = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &env.work_id,
            "--chapter",
            "1",
            "--start",
            "0",
            "--end",
            "5",
            "--selected-text",
            "text",
            "--color",
            "blue",
            "--json",
        ]);
    assert!(add.status.success(), "add failed: {}", stderr(&add));
    let added: serde_json::Value = serde_json::from_str(&stdout(&add)).expect("json added");
    let annotation_id = added["annotation_id"].as_str().expect("annotation id");

    let out = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "patch",
            annotation_id,
            "--color",
            "green",
            "--json",
        ]);
    assert!(
        out.status.success(),
        "patch --json failed: {}",
        stderr(&out)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("json patched");
    assert_eq!(json["annotation_id"], annotation_id);
    assert_eq!(json["color"], "green");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_remove_json_prints_empty_stdout() {
    let env = fresh_env().await;

    let add = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &env.work_id,
            "--chapter",
            "1",
            "--start",
            "0",
            "--end",
            "5",
            "--selected-text",
            "text",
            "--color",
            "blue",
            "--json",
        ]);
    assert!(add.status.success(), "add failed: {}", stderr(&add));
    let added: serde_json::Value = serde_json::from_str(&stdout(&add)).expect("json added");
    let annotation_id = added["annotation_id"].as_str().expect("annotation id");

    let out = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "remove",
            annotation_id,
            "--json",
        ]);
    assert!(
        out.status.success(),
        "remove --json failed: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).trim().is_empty(),
        "--json delete prints empty stdout: {}",
        stdout(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_add_rejects_invalid_color() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &env.work_id,
            "--chapter",
            "1",
            "--start",
            "0",
            "--end",
            "4",
            "--selected-text",
            "text",
            "--color",
            "purple",
        ]);
    assert!(!out.status.success(), "invalid color must fail");
    assert!(
        stderr(&out).contains("yellow, blue, green, pink"),
        "stderr should name the valid colors: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_remove_unknown_id_surfaces_core_error() {
    let env = fresh_env().await;

    let out = env
        .cli(&["creator", "reading", "annotation", "remove", "ann_missing"]);
    assert!(!out.status.success(), "unknown annotation must fail");
    assert!(
        stderr(&out).contains("404") || stderr(&out).to_lowercase().contains("not found"),
        "stderr should name the 404 not_found: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_list_unknown_work_surfaces_core_error() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            "wrk_does_not_exist",
            "--chapter",
            "1",
        ]);
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the 404 not_found: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_patch_unknown_id_surfaces_core_error() {
    let env = fresh_env().await;

    let out = env
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "patch",
            "ann_missing",
            "--color",
            "pink",
        ]);
    assert!(!out.status.success(), "unknown annotation must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the 404 not_found: {err}"
    );
}
