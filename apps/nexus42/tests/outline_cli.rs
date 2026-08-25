//! Hermetic CLI integration tests — `creator works outline|chapter|timeline`
//! (V1.175 P1 Task 3, group 2): CAS-guarded V1.72 canvas outline+timeline
//! writes over the existing daemon routes, end-to-end against a live daemon
//! fixture with hermetic `HOME` (AR-83 #6 / AR-84 group 2).
//!
//! Each test seeds one owned Work (with `story_ref` set so the outline file
//! path is deterministic) + a chapter row, then drives the REAL `nexus42`
//! binary. Failure paths: one conflict path per leaf (stale
//! `--base-revision` → 409 `outline_conflict` rendering current revision +
//! conflicting path + recovery hint), the invalid-field path (bad slug →
//! 422 `outline_validation_failed`), and 404 `not_found` for an unknown
//! work.

mod common;

use axum::extract::State;
use axum::Json;
use common::LiveDaemon;
use nexus_daemon_runtime::api::handlers::works::{create_work, CreateWorkRequest};
use nexus_local_db::work_chapters::{self, InsertChapterParams};
use serde_json::Value;
use std::process::Output;

/// Seed one owned Work (bound to the fixture's `wld_test_world`) with a
/// deterministic `story_ref` and one chapter row, and return the `work_id`.
async fn seed_work(d: &LiveDaemon) -> String {
    let (_, Json(resp)) = create_work(
        State(d.state.clone()),
        Json(CreateWorkRequest {
            title: "Outline Test Novel".to_string(),
            long_term_goal: "Test the outline canvas".to_string(),
            initial_idea: "A test story".to_string(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: Some("outline-test-novel".to_string()),
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
            work_profile: Some("novel".to_string()),
        }),
    )
    .await
    .expect("seed work via daemon handler");
    let work_id = resp.work_id;

    let now = chrono::Utc::now().to_rfc3339();
    work_chapters::insert_chapter(
        &d.pool,
        &InsertChapterParams {
            work_id: &work_id,
            chapter: 1,
            volume: Some(1),
            slug: Some("ch01"),
            planned_word_count: 4000,
            outline_path: None,
            body_path: None,
            now: &now,
        },
    )
    .await
    .expect("seed chapter");
    work_id
}

/// Write the work-level outline file at revision 0 with one volume holding
/// chapter 1 (the default frontmatter shape the canvas handlers read).
fn write_outline_file(d: &LiveDaemon) {
    let ws_root = d.home.path().join("workspace");
    let rel = "Works/outline-test-novel/Outlines/outline.md";
    let outline_path = ws_root.join(rel);
    std::fs::create_dir_all(outline_path.parent().expect("outline parent"))
        .expect("create outline dirs");
    std::fs::write(
        &outline_path,
        "---\n\
         outline_revision: 0\n\
         volumes:\n\
         \x20 - volume_id: 1\n\
         \x20   label: Volume 1\n\
         \x20   chapter_ids: [1]\n\
         timeline_events: []\n\
         foreshadows: []\n\
         chapter_titles: {}\n\
         updated_at: \"2024-01-01T00:00:00Z\"\n\
         ---\nbody\n",
    )
    .expect("write outline file");
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ── outline show ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outline_show_prints_revision_and_volumes() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    let out = d
        .cli(&["creator", "works", "outline", "show", &work_id])
        .await;
    assert!(out.status.success(), "show failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("revision 0"), "{text}");
    assert!(text.contains("Volume 1"), "{text}");
    assert!(text.contains(&work_id), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outline_show_json_emits_dto_verbatim() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    let out = d
        .cli(&["creator", "works", "outline", "show", &work_id, "--json"])
        .await;
    assert!(out.status.success(), "show --json failed: {}", stderr(&out));
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["work_id"], work_id);
    assert_eq!(json["outline_revision"], 0);
    assert!(json["volumes"].is_array());
    assert!(json["timeline_events"].is_array());
    assert!(json["foreshadows"].is_array());
    assert!(json["chapter_titles"].is_object());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outline_show_unknown_work_surfaces_404() {
    let d = LiveDaemon::start_with_workspace().await;

    let out = d
        .cli(&["creator", "works", "outline", "show", "wrk_does_not_exist"])
        .await;
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(
        err.contains("[not_found]"),
        "stderr should name the code: {err}"
    );
    assert!(err.contains("404"), "stderr should carry HTTP 404: {err}");
}

// ── outline patch (structure) ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outline_patch_move_chapter_bumps_revision() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    let out = d
        .cli(&[
            "creator",
            "works",
            "outline",
            "patch",
            &work_id,
            "--base-revision",
            "0",
            "--op",
            "move_chapter",
            "--chapter",
            "1",
            "--volume",
            "2",
        ])
        .await;
    assert!(
        out.status.success(),
        "move_chapter failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("new_revision: 1"), "{text}");

    // The outline file on disk now has the chapter in volume 2.
    let ws_root = d.home.path().join("workspace");
    let on_disk =
        std::fs::read_to_string(ws_root.join("Works/outline-test-novel/Outlines/outline.md"))
            .unwrap();
    assert!(on_disk.contains("outline_revision: 1"), "{on_disk}");
    assert!(on_disk.contains("volume_id: 2"), "{on_disk}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outline_patch_stale_revision_surfaces_conflict() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    // base_revision 5 vs current 0 → 409 outline_conflict.
    let out = d
        .cli(&[
            "creator",
            "works",
            "outline",
            "patch",
            &work_id,
            "--base-revision",
            "5",
            "--op",
            "move_chapter",
            "--chapter",
            "1",
            "--volume",
            "2",
        ])
        .await;
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("outline_conflict"), "stderr: {err}");
    assert!(err.contains("409"), "stderr should carry HTTP 409: {err}");
    // All three conflict fields render: current revision, conflicting path,
    // recovery hint.
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outline_patch_missing_required_flag_fails_fast() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    // move_chapter without --volume → CLI fail-fast.
    let out = d
        .cli(&[
            "creator",
            "works",
            "outline",
            "patch",
            &work_id,
            "--base-revision",
            "0",
            "--op",
            "move_chapter",
            "--chapter",
            "1",
        ])
        .await;
    assert!(!out.status.success(), "missing --volume must fail");
    assert!(
        stderr(&out).contains("--volume"),
        "stderr should name --volume: {}",
        stderr(&out)
    );
}

// ── chapter patch (outline node) ───────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapter_patch_updates_metadata_and_bumps_revision() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    let out = d
        .cli(&[
            "creator",
            "works",
            "chapter",
            "patch",
            &work_id,
            "--n",
            "1",
            "--base-revision",
            "0",
            "--title",
            "Chapter One",
            "--slug",
            "ch01",
            "--planned-word-count",
            "5000",
            "--status",
            "outlined",
        ])
        .await;
    assert!(
        out.status.success(),
        "chapter patch failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("new_revision: 1"), "{text}");

    // The outline file on disk carries the title + revision bump.
    let ws_root = d.home.path().join("workspace");
    let on_disk =
        std::fs::read_to_string(ws_root.join("Works/outline-test-novel/Outlines/outline.md"))
            .unwrap();
    assert!(on_disk.contains("outline_revision: 1"), "{on_disk}");
    assert!(on_disk.contains("Chapter One"), "{on_disk}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapter_patch_json_emits_dto_verbatim() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    let out = d
        .cli(&[
            "creator",
            "works",
            "chapter",
            "patch",
            &work_id,
            "--n",
            "1",
            "--base-revision",
            "0",
            "--title",
            "JSON Chapter",
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "chapter patch --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["new_revision"], 1);
    assert!(json["validation_summary"]["errors"].is_array());
    assert!(json["validation_summary"]["warnings"].is_array());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapter_patch_stale_revision_surfaces_conflict() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    // base_revision 3 vs current 0 → 409 outline_conflict.
    let out = d
        .cli(&[
            "creator",
            "works",
            "chapter",
            "patch",
            &work_id,
            "--n",
            "1",
            "--base-revision",
            "3",
            "--title",
            "stale",
        ])
        .await;
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("outline_conflict"), "stderr: {err}");
    assert!(err.contains("409"), "stderr should carry HTTP 409: {err}");
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapter_patch_invalid_slug_surfaces_validation_failed() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    // Uppercase slug violates the kebab-case rule → 422
    // outline_validation_failed (the invalid-field path).
    let out = d
        .cli(&[
            "creator",
            "works",
            "chapter",
            "patch",
            &work_id,
            "--n",
            "1",
            "--base-revision",
            "0",
            "--slug",
            "Bad Slug!",
        ])
        .await;
    assert!(!out.status.success(), "invalid slug must fail");
    let err = stderr(&out);
    assert!(
        err.contains("outline_validation_failed"),
        "stderr should name the code: {err}"
    );
    assert!(err.contains("422"), "stderr should carry HTTP 422: {err}");
    assert!(
        err.contains("kebab-case"),
        "stderr should name the rule: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapter_patch_no_set_field_fails_fast() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    let out = d
        .cli(&[
            "creator",
            "works",
            "chapter",
            "patch",
            &work_id,
            "--n",
            "1",
            "--base-revision",
            "0",
        ])
        .await;
    assert!(!out.status.success(), "empty set must fail");
    assert!(
        stderr(&out).contains("--title"),
        "stderr should list the set flags: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapter_patch_content_file_writes_outline_prose() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    // Seed an existing per-chapter outline file so the content patch has a
    // target (the daemon derives `Works/<ref>/Outlines/chapters/ch01-outline.md`).
    let ws_root = d.home.path().join("workspace");
    let chapter_outline =
        ws_root.join("Works/outline-test-novel/Outlines/chapters/ch01-outline.md");
    std::fs::create_dir_all(chapter_outline.parent().expect("chapter outline parent"))
        .expect("create chapter outline dirs");
    std::fs::write(&chapter_outline, "# Old outline\n").expect("write chapter outline");

    let content_file = d.home.path().join("content.md");
    std::fs::write(&content_file, "## Scene beats\n\n- Open on the harbor\n")
        .expect("write content file");

    let out = d
        .cli(&[
            "creator",
            "works",
            "chapter",
            "patch",
            &work_id,
            "--n",
            "1",
            "--base-revision",
            "0",
            "--content-file",
            content_file.to_str().expect("content file path"),
        ])
        .await;
    assert!(
        out.status.success(),
        "content patch failed: {}",
        stderr(&out)
    );
    assert!(stdout(&out).contains("new_revision: 1"), "{}", stdout(&out));

    let on_disk = std::fs::read_to_string(&chapter_outline).unwrap();
    assert!(
        on_disk.contains("## Scene beats"),
        "chapter outline should hold patched prose; got: {on_disk}"
    );
    assert!(
        !on_disk.contains("# Old outline"),
        "chapter outline should not hold stale prose; got: {on_disk}"
    );
}

// ── timeline patch ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeline_patch_add_event_bumps_revision() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    let out = d
        .cli(&[
            "creator",
            "works",
            "timeline",
            "patch",
            &work_id,
            "--base-revision",
            "0",
            "--op",
            "add_event",
            "--title",
            "The storm",
            "--description",
            "A storm hits the harbor",
            "--realizes-chapter",
            "1",
        ])
        .await;
    assert!(out.status.success(), "add_event failed: {}", stderr(&out));
    assert!(stdout(&out).contains("new_revision: 1"), "{}", stdout(&out));

    // The outline file on disk now carries the event.
    let ws_root = d.home.path().join("workspace");
    let on_disk =
        std::fs::read_to_string(ws_root.join("Works/outline-test-novel/Outlines/outline.md"))
            .unwrap();
    assert!(on_disk.contains("outline_revision: 1"), "{on_disk}");
    assert!(on_disk.contains("The storm"), "{on_disk}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeline_patch_stale_revision_surfaces_conflict() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    // base_revision 2 vs current 0 → 409 outline_conflict.
    let out = d
        .cli(&[
            "creator",
            "works",
            "timeline",
            "patch",
            &work_id,
            "--base-revision",
            "2",
            "--op",
            "add_event",
            "--title",
            "stale",
        ])
        .await;
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("outline_conflict"), "stderr: {err}");
    assert!(err.contains("409"), "stderr should carry HTTP 409: {err}");
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeline_patch_missing_required_flag_fails_fast() {
    let d = LiveDaemon::start_with_workspace().await;
    let work_id = seed_work(&d).await;
    write_outline_file(&d);

    // add_event without --title → CLI fail-fast.
    let out = d
        .cli(&[
            "creator",
            "works",
            "timeline",
            "patch",
            &work_id,
            "--base-revision",
            "0",
            "--op",
            "add_event",
        ])
        .await;
    assert!(!out.status.success(), "missing --title must fail");
    assert!(
        stderr(&out).contains("--title"),
        "stderr should name --title: {}",
        stderr(&out)
    );
}

// ── help ───────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn outline_help_documents_route_family_and_retry() {
    let d = LiveDaemon::start_with_workspace().await;

    let out = d.cli(&["creator", "works", "outline", "--help"]).await;
    assert!(
        out.status.success(),
        "outline --help failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("show"), "{text}");
    assert!(text.contains("patch"), "{text}");
    assert!(text.contains("outline_conflict"), "{text}");
    assert!(text.contains("reapply"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chapter_help_pins_outline_node_route_distinction() {
    let d = LiveDaemon::start_with_workspace().await;

    let out = d
        .cli(&["creator", "works", "chapter", "patch", "--help"])
        .await;
    assert!(
        out.status.success(),
        "chapter patch --help failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    // The route-family guard: the leaf names the outline node route and the
    // V1.65 chapter-content PATCH distinction (AR-84).
    assert!(text.contains("chapters/:n/patch"), "{text}");
    assert!(text.contains("outline"), "{text}");
    assert!(text.contains("outline_conflict"), "{text}");
    assert!(text.contains("reapply"), "{text}");
}
