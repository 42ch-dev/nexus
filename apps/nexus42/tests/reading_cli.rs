//! Hermetic CLI integration tests — `creator reading` (V1.175 P1 Task 1,
//! group 3): progress `get|set|clear` + annotation `list|add|patch|remove`
//! end-to-end against a live daemon fixture with hermetic `HOME` (AR-83 #6).
//!
//! Each test seeds one owned Work, then drives the REAL `nexus42` binary
//! against the in-process daemon. Failure paths: out-of-range `--scroll`,
//! invalid `--color`, and a non-existent Work (daemon 404).

mod common;

use axum::extract::State;
use axum::Json;
use common::LiveDaemon;
use nexus_daemon_runtime::api::handlers::works::{create_work, CreateWorkRequest};
use serde_json::Value;
use std::process::Output;

/// Seed one owned Work (bound to the fixture's `wld_test_world`) and return
/// its `work_id`.
async fn seed_work(d: &LiveDaemon) -> String {
    let (_, Json(resp)) = create_work(
        State(d.state.clone()),
        Json(CreateWorkRequest {
            title: "Reading Test Novel".to_string(),
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

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ── Progress ───────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_set_get_round_trip() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let set = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            &work_id,
            "--chapter",
            "3",
            "--scroll",
            "7500",
        ])
        .await;
    assert!(set.status.success(), "set failed: {}", stderr(&set));
    assert!(
        stdout(&set).contains("Saved reading progress."),
        "{}",
        stdout(&set)
    );
    assert!(stdout(&set).contains("7500"), "{}", stdout(&set));

    let get = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "get",
            &work_id,
            "--chapter",
            "3",
        ])
        .await;
    assert!(get.status.success(), "get failed: {}", stderr(&get));
    assert!(stdout(&get).contains("7500"), "{}", stdout(&get));
    assert!(stdout(&get).contains(&work_id), "{}", stdout(&get));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_set_json_emits_dto_verbatim() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            &work_id,
            "--chapter",
            "2",
            "--scroll",
            "1234",
            "--json",
        ])
        .await;
    assert!(out.status.success(), "set --json failed: {}", stderr(&out));
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["work_id"], work_id);
    assert_eq!(json["chapter"], 2);
    assert_eq!(json["scroll_progress"], 1234);
    assert!(json["updated_at"].is_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_clear_removes_row() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let _ = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            &work_id,
            "--chapter",
            "1",
            "--scroll",
            "500",
        ])
        .await;
    let clear = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "clear",
            &work_id,
            "--chapter",
            "1",
        ])
        .await;
    assert!(clear.status.success(), "clear failed: {}", stderr(&clear));
    assert!(stdout(&clear).contains("Cleared"), "{}", stdout(&clear));

    // After the clear the daemon reports the default (0) progress.
    let get = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "get",
            &work_id,
            "--chapter",
            "1",
            "--json",
        ])
        .await;
    assert!(
        get.status.success(),
        "get after clear failed: {}",
        stderr(&get)
    );
    let json: Value = serde_json::from_str(&stdout(&get)).expect("json output");
    assert_eq!(json["scroll_progress"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_clear_json_prints_empty_stdout() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let _ = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            &work_id,
            "--chapter",
            "2",
            "--scroll",
            "1000",
        ])
        .await;
    let out = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "clear",
            &work_id,
            "--chapter",
            "2",
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "clear --json failed: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).trim().is_empty(),
        "--json 204 delete should print empty stdout: {}",
        stdout(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_clear_unknown_work_surfaces_daemon_error() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "clear",
            "wrk_does_not_exist",
            "--chapter",
            "1",
        ])
        .await;
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the daemon 404: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_set_rejects_out_of_range_scroll() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "set",
            &work_id,
            "--chapter",
            "1",
            "--scroll",
            "20000",
        ])
        .await;
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
async fn progress_get_unknown_work_surfaces_daemon_error() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "progress",
            "get",
            "wrk_does_not_exist",
            "--chapter",
            "1",
        ])
        .await;
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the daemon 404: {err}"
    );
}

// ── Annotations ────────────────────────────────────────────────────────────

// The full add→list→patch→list→remove→list journey is one linear scenario;
// splitting it would hide the cross-verb state transitions it pins.
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_lifecycle_round_trip() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let add = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &work_id,
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
        ])
        .await;
    assert!(add.status.success(), "add failed: {}", stderr(&add));
    let added = stdout(&add);
    assert!(added.contains("Created annotation"), "{added}");

    let list = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            &work_id,
            "--chapter",
            "5",
            "--json",
        ])
        .await;
    assert!(list.status.success(), "list failed: {}", stderr(&list));
    let json: Value = serde_json::from_str(&stdout(&list)).expect("json list");
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "one annotation: {json}");
    let annotation_id = items[0]["annotation_id"].as_str().expect("annotation id");
    assert!(annotation_id.starts_with("ann_"), "{annotation_id}");
    assert_eq!(items[0]["selected_text"], "the gate groaned");
    assert_eq!(items[0]["color"], "yellow");
    assert_eq!(items[0]["note"], "check pacing");

    let patch = d
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
        ])
        .await;
    assert!(patch.status.success(), "patch failed: {}", stderr(&patch));

    let list2 = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            &work_id,
            "--chapter",
            "5",
            "--json",
        ])
        .await;
    let json2: Value = serde_json::from_str(&stdout(&list2)).expect("json list 2");
    let item2 = &json2["items"][0];
    assert_eq!(item2["color"], "pink");
    assert_eq!(item2["note"], "rewritten");

    let remove = d
        .cli(&["creator", "reading", "annotation", "remove", annotation_id])
        .await;
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

    let list3 = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            &work_id,
            "--chapter",
            "5",
        ])
        .await;
    assert!(list3.status.success());
    assert!(
        stdout(&list3).contains("No annotations"),
        "{}",
        stdout(&list3)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_add_json_emits_dto() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &work_id,
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
        ])
        .await;
    assert!(out.status.success(), "add --json failed: {}", stderr(&out));
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json annotation");
    let annotation_id = json["annotation_id"].as_str().expect("annotation id");
    assert!(annotation_id.starts_with("ann_"), "{annotation_id}");
    assert_eq!(json["color"], "blue");
    assert_eq!(json["selected_text"], "text");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_patch_json_emits_dto() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let add = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &work_id,
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
        ])
        .await;
    assert!(add.status.success(), "add failed: {}", stderr(&add));
    let added: Value = serde_json::from_str(&stdout(&add)).expect("json added");
    let annotation_id = added["annotation_id"].as_str().expect("annotation id");

    let out = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "patch",
            annotation_id,
            "--color",
            "green",
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "patch --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json patched");
    assert_eq!(json["annotation_id"], annotation_id);
    assert_eq!(json["color"], "green");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_remove_json_prints_empty_stdout() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let add = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &work_id,
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
        ])
        .await;
    assert!(add.status.success(), "add failed: {}", stderr(&add));
    let added: Value = serde_json::from_str(&stdout(&add)).expect("json added");
    let annotation_id = added["annotation_id"].as_str().expect("annotation id");

    let out = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "remove",
            annotation_id,
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "remove --json failed: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).trim().is_empty(),
        "--json 204 delete should print empty stdout: {}",
        stdout(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_add_rejects_invalid_color() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "add",
            &work_id,
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
        ])
        .await;
    assert!(!out.status.success(), "invalid color must fail");
    assert!(
        stderr(&out).contains("yellow, blue, green, pink"),
        "stderr should name the valid colors: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_remove_unknown_id_surfaces_daemon_error() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&["creator", "reading", "annotation", "remove", "ann_missing"])
        .await;
    assert!(!out.status.success(), "unknown annotation must fail");
    assert!(
        stderr(&out).contains("404") || stderr(&out).to_lowercase().contains("not found"),
        "stderr should name the daemon 404: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_list_unknown_work_surfaces_daemon_error() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "list",
            "wrk_does_not_exist",
            "--chapter",
            "1",
        ])
        .await;
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the daemon 404: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn annotation_patch_unknown_id_surfaces_daemon_error() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&[
            "creator",
            "reading",
            "annotation",
            "patch",
            "ann_missing",
            "--color",
            "pink",
        ])
        .await;
    assert!(!out.status.success(), "unknown annotation must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the daemon 404: {err}"
    );
}
