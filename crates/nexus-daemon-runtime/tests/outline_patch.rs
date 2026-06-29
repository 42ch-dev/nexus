//! Outline β hardening regression tests (V1.73 P1, B1–B4).
//!
//! Closes the four V1.72 carry-over MEDIUM validation gaps:
//! - B1 `R-V172P0-QC2-001` — slug format / uniqueness
//! - B2 `R-V172P0-QC2-002` — volume existence / pre-creation
//! - B3 `R-V172P0-QC2-003` — foreshadow temporal order
//! - B4 `R-V172P0-QC2-004` — published-chapter structural edit guard (structure)
//!
//! Each rule rejects genuinely-invalid input through the structured
//! `outline_validation_failed` (HTTP 422) channel while existing valid patches
//! continue to pass. Handlers are invoked directly (same pattern as
//! `outline_api.rs`).

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use axum::Json;
use nexus_daemon_runtime::api::errors::NexusApiError;
use nexus_daemon_runtime::api::handlers::{
    outline,
    works::{CreateWorkRequest, PatchWorkRequest},
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::work_chapters::{self, InsertChapterParams, PatchChapterParams};
use serde_json::{json, Value};

struct TestCtx {
    _tmp: test_utils::TestTempRoot,
    state: WorkspaceState,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path, workspace_dir) =
        test_utils::create_initialized_test_workspace().await;
    let state = WorkspaceState::new_for_testing(
        nexus_home,
        db_path,
        Some(workspace_dir.to_string_lossy().to_string()),
    )
    .await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    TestCtx { _tmp: tmp, state }
}

async fn create_work(state: &WorkspaceState) -> String {
    let req = CreateWorkRequest {
        title: "Outline Hardening Novel".to_string(),
        long_term_goal: "Harden outline validation".to_string(),
        initial_idea: "A test story".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        primary_preset_id: None,
        lineage_from_work_id: None,
        client_request_id: None,
        set_pool_active: None,
        work_profile: None,
    };
    let (_status, Json(resp)) =
        nexus_daemon_runtime::api::handlers::works::create_work(State(state.clone()), Json(req))
            .await
            .unwrap();
    resp.work_id
}

async fn set_story_ref(state: &WorkspaceState, work_id: &str, story_ref: &str) {
    let req = PatchWorkRequest {
        title: None,
        long_term_goal: None,
        creative_brief: None,
        intake_status: None,
        status: None,
        world_id: None,
        story_ref: Some(Some(story_ref.to_string())),
        primary_preset_id: None,
        current_stage: None,
        stage_status: None,
        force: None,
        auto_review_master_on_timeout: None,
        auto_chain_interrupted: None,
        work_profile: None,
    };
    let _ = nexus_daemon_runtime::api::handlers::works::patch_work(
        State(state.clone()),
        Path(work_id.to_string()),
        Json(req),
    )
    .await
    .unwrap();
}

async fn seed_chapter(pool: &sqlx::SqlitePool, work_id: &str, chapter: i32) {
    let now = chrono::Utc::now().to_rfc3339();
    let params = InsertChapterParams {
        work_id,
        chapter,
        volume: Some(1),
        slug: Some(&format!("ch{chapter:02}")),
        planned_word_count: 4000,
        outline_path: None,
        body_path: None,
        now: &now,
    };
    work_chapters::insert_chapter(pool, &params)
        .await
        .expect("seed chapter");
}

/// Force a chapter into a status the API itself cannot drive (e.g. `published`).
async fn force_chapter_status(pool: &sqlx::SqlitePool, work_id: &str, chapter: i32, status: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    work_chapters::patch_chapter(
        pool,
        work_id,
        chapter,
        1,
        &PatchChapterParams {
            status: Some(status.to_string()),
            ..Default::default()
        },
        &now,
    )
    .await
    .expect("force chapter status");
}

async fn current_revision(state: &WorkspaceState, work_id: &str) -> u64 {
    let Json(body) = outline::get_work_outline(State(state.clone()), Path(work_id.to_string()))
        .await
        .unwrap();
    body.outline_revision
}

/// Assert a result is the structured 422 outline validation error.
fn assert_validation_failed<T: std::fmt::Debug>(result: Result<T, NexusApiError>) {
    let err = result.unwrap_err();
    assert!(
        matches!(err, NexusApiError::OutlineValidationFailed { .. }),
        "expected OutlineValidationFailed (422), got {err:?}"
    );
}

async fn setup_work(state: &WorkspaceState) -> String {
    let work_id = create_work(state).await;
    set_story_ref(state, &work_id, "outline-hardening-novel").await;
    work_id
}

// ─── B1: slug format + uniqueness ───────────────────────────────────────────

#[tokio::test]
async fn b1_valid_kebab_slug_passes() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let req = json!({
        "work_id": work_id,
        "chapter_id": 1,
        "base_revision": 0,
        "set": { "slug": "opening-scene" }
    });
    let Json(resp) = outline::patch_outline_chapter(
        State(ctx.state.clone()),
        Path((work_id.clone(), "1".to_string())),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .expect("valid kebab slug should pass");
    assert_eq!(resp.new_revision, 1);
}

#[tokio::test]
async fn b1_rejects_uppercase_slug() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let req = json!({
        "work_id": work_id,
        "chapter_id": 1,
        "base_revision": 0,
        "set": { "slug": "Opening-Scene" }
    });
    assert_validation_failed(
        outline::patch_outline_chapter(
            State(ctx.state.clone()),
            Path((work_id.clone(), "1".to_string())),
            Json(serde_json::from_value(req).unwrap()),
        )
        .await,
    );
    // Revision must be untouched on rejection.
    assert_eq!(current_revision(&ctx.state, &work_id).await, 0);
}

#[tokio::test]
async fn b1_rejects_slug_with_spaces() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let req = json!({
        "work_id": work_id,
        "chapter_id": 1,
        "base_revision": 0,
        "set": { "slug": "opening scene" }
    });
    assert_validation_failed(
        outline::patch_outline_chapter(
            State(ctx.state.clone()),
            Path((work_id.clone(), "1".to_string())),
            Json(serde_json::from_value(req).unwrap()),
        )
        .await,
    );
}

#[tokio::test]
async fn b1_rejects_slug_that_is_too_long() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    // 81 chars — one beyond the 80-char ceiling.
    let long_slug = "a".repeat(81);
    let req = json!({
        "work_id": work_id,
        "chapter_id": 1,
        "base_revision": 0,
        "set": { "slug": long_slug }
    });
    assert_validation_failed(
        outline::patch_outline_chapter(
            State(ctx.state.clone()),
            Path((work_id.clone(), "1".to_string())),
            Json(serde_json::from_value(req).unwrap()),
        )
        .await,
    );
}

#[tokio::test]
async fn b1_rejects_duplicate_slug_within_work() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await; // slug "ch01"
    seed_chapter(ctx.state.pool(), &work_id, 2).await; // slug "ch02"

    // Patching chapter 2's slug to chapter 1's slug collides.
    let req = json!({
        "work_id": work_id,
        "chapter_id": 2,
        "base_revision": 0,
        "set": { "slug": "ch01" }
    });
    assert_validation_failed(
        outline::patch_outline_chapter(
            State(ctx.state.clone()),
            Path((work_id.clone(), "2".to_string())),
            Json(serde_json::from_value(req).unwrap()),
        )
        .await,
    );
}

#[tokio::test]
async fn b1_allows_unchanged_slug_on_same_chapter() {
    // Re-asserting a chapter's own slug must not trip the uniqueness check.
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await; // slug "ch01"

    let req = json!({
        "work_id": work_id,
        "chapter_id": 1,
        "base_revision": 0,
        "set": { "slug": "ch01" }
    });
    let Json(resp) = outline::patch_outline_chapter(
        State(ctx.state.clone()),
        Path((work_id.clone(), "1".to_string())),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .expect("re-asserting the same chapter's slug should pass");
    assert_eq!(resp.new_revision, 1);
}

// ─── B2: volume existence / pre-creation ────────────────────────────────────

#[tokio::test]
async fn b2_attach_to_existing_volume_passes() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await; // lands in default Volume 1

    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "attach_to_volume",
        "chapter_id": 1,
        "volume_id": 1
    });
    let Json(resp) = outline::patch_outline_structure(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .expect("attach to existing volume should pass");
    assert_eq!(resp.new_revision, 1);
}

#[tokio::test]
async fn b2_move_to_next_sequential_volume_passes() {
    // The legitimate "create Volume N+1" authoring flow must keep working.
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await; // Volume 1 exists (max=1)

    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "move_chapter",
        "chapter_id": 1,
        "volume_id": 2 // max + 1 → allowed
    });
    let Json(resp) = outline::patch_outline_structure(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .expect("move to the next sequential volume should pass");
    assert_eq!(resp.new_revision, 1);
}

#[tokio::test]
async fn b2_rejects_arbitrary_nonexistent_volume_via_structure_patch() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await; // max volume = 1

    // 999 is far beyond max+1 → a typo, not an explicit author action.
    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "attach_to_volume",
        "chapter_id": 1,
        "volume_id": 999
    });
    assert_validation_failed(
        outline::patch_outline_structure(
            State(ctx.state.clone()),
            Path(work_id.clone()),
            Json(serde_json::from_value(req).unwrap()),
        )
        .await,
    );
    assert_eq!(current_revision(&ctx.state, &work_id).await, 0);
}

#[tokio::test]
async fn b2_rejects_arbitrary_nonexistent_volume_via_chapter_patch() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await; // max volume = 1

    let req = json!({
        "work_id": work_id,
        "chapter_id": 1,
        "base_revision": 0,
        "set": { "volume": 999 }
    });
    assert_validation_failed(
        outline::patch_outline_chapter(
            State(ctx.state.clone()),
            Path((work_id.clone(), "1".to_string())),
            Json(serde_json::from_value(req).unwrap()),
        )
        .await,
    );
}

// ─── B3: foreshadow temporal order ──────────────────────────────────────────

/// Helper: run a timeline patch op, threading `base_revision` and returning the
/// new revision (or the validation error).
async fn timeline_patch(
    state: &WorkspaceState,
    work_id: &str,
    base_revision: i64,
    body: Value,
) -> Result<i64, NexusApiError> {
    let mut req = body;
    req["work_id"] = json!(work_id);
    req["base_revision"] = json!(base_revision);
    outline::patch_timeline_event(
        State(state.clone()),
        Path(work_id.to_string()),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .map(|Json(resp)| resp.new_revision)
}

#[tokio::test]
async fn b3_foreshadow_source_before_target_passes() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;
    seed_chapter(ctx.state.pool(), &work_id, 3).await;

    // Source event realizes ch1; target event realizes ch3 (1 <= 3) → ok.
    let rev = timeline_patch(
        &ctx.state,
        &work_id,
        0,
        json!({ "operation": "add_event", "title": "Plant", "realizes_chapter_id": 1 }),
    )
    .await
    .unwrap();
    // Capture the generated source event id from the projected outline.
    let source_id = event_id_at(&ctx.state, &work_id, 0).await;

    let rev = timeline_patch(
        &ctx.state,
        &work_id,
        rev,
        json!({ "operation": "add_event", "title": "Payoff", "realizes_chapter_id": 3 }),
    )
    .await
    .unwrap();
    let target_id = event_id_at(&ctx.state, &work_id, 1).await;

    let rev = timeline_patch(
        &ctx.state,
        &work_id,
        rev,
        json!({
            "operation": "link_foreshadow",
            "event_id": source_id,
            "foreshadows_event_id": target_id,
        }),
    )
    .await
    .expect("source-before-target foreshadow should pass");
    assert!(rev >= 3);
}

#[tokio::test]
async fn b3_rejects_foreshadow_source_after_target() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;
    seed_chapter(ctx.state.pool(), &work_id, 3).await;

    // Source realizes ch3; target realizes ch1 (3 > 1) → temporal violation.
    let rev = timeline_patch(
        &ctx.state,
        &work_id,
        0,
        json!({ "operation": "add_event", "title": "Late", "realizes_chapter_id": 3 }),
    )
    .await
    .unwrap();
    let source_id = event_id_at(&ctx.state, &work_id, 0).await;

    let rev = timeline_patch(
        &ctx.state,
        &work_id,
        rev,
        json!({ "operation": "add_event", "title": "Early", "realizes_chapter_id": 1 }),
    )
    .await
    .unwrap();
    let target_id = event_id_at(&ctx.state, &work_id, 1).await;

    assert_validation_failed(
        timeline_patch(
            &ctx.state,
            &work_id,
            rev,
            json!({
                "operation": "link_foreshadow",
                "event_id": source_id,
                "foreshadows_event_id": target_id,
            }),
        )
        .await,
    );
}

#[tokio::test]
async fn b3_rejects_foreshadow_when_realization_unscheduled() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    // Source event has no realizing chapter → ordering cannot be established.
    let rev = timeline_patch(
        &ctx.state,
        &work_id,
        0,
        json!({ "operation": "add_event", "title": "Unscheduled" }),
    )
    .await
    .unwrap();
    let source_id = event_id_at(&ctx.state, &work_id, 0).await;

    let rev = timeline_patch(
        &ctx.state,
        &work_id,
        rev,
        json!({ "operation": "add_event", "title": "Realized", "realizes_chapter_id": 1 }),
    )
    .await
    .unwrap();
    let target_id = event_id_at(&ctx.state, &work_id, 1).await;

    assert_validation_failed(
        timeline_patch(
            &ctx.state,
            &work_id,
            rev,
            json!({
                "operation": "link_foreshadow",
                "event_id": source_id,
                "foreshadows_event_id": target_id,
            }),
        )
        .await,
    );
}

/// Read the event_id at `index` from the projected work outline.
async fn event_id_at(state: &WorkspaceState, work_id: &str, index: usize) -> String {
    let Json(body) = outline::get_work_outline(State(state.clone()), Path(work_id.to_string()))
        .await
        .unwrap();
    let body = serde_json::to_value(body).unwrap();
    body["timeline_events"][index]["event_id"]
        .as_str()
        .expect("event id")
        .to_string()
}

// ─── B4: published-chapter structural edit guard (structure route) ──────────

#[tokio::test]
async fn b4_blocks_move_chapter_on_published_chapter() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;
    force_chapter_status(ctx.state.pool(), &work_id, 1, "published").await;

    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "move_chapter",
        "chapter_id": 1,
        "volume_id": 2
    });
    assert_validation_failed(
        outline::patch_outline_structure(
            State(ctx.state.clone()),
            Path(work_id.clone()),
            Json(serde_json::from_value(req).unwrap()),
        )
        .await,
    );
    assert_eq!(current_revision(&ctx.state, &work_id).await, 0);
}

#[tokio::test]
async fn b4_allows_move_chapter_on_draft_chapter() {
    // Non-published chapters are unaffected by the structural guard.
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;
    force_chapter_status(ctx.state.pool(), &work_id, 1, "draft").await;

    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "move_chapter",
        "chapter_id": 1,
        "volume_id": 2
    });
    let Json(resp) = outline::patch_outline_structure(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .expect("moving a draft chapter should pass");
    assert_eq!(resp.new_revision, 1);
}

// ─── V1.75 A2: outline-prose content patch (canvas-pivot parity-close) ───────
//
// The V1.75 `content` patch writes chapter outline prose to the per-chapter
// `outline_path` markdown file and bumps the work-level `outline_revision`
// CAS. It MUST NOT touch `body_path`. These tests prove the parity-close
// behavior + the body-ownership invariant.

/// Invoke `patch_outline_chapter`, returning the new revision or the error.
async fn chapter_patch(
    state: &WorkspaceState,
    work_id: &str,
    chapter: &str,
    base_revision: i64,
    set: Value,
) -> Result<i64, NexusApiError> {
    let req = json!({
        "work_id": work_id,
        "chapter_id": chapter.parse::<i64>().unwrap_or(0),
        "base_revision": base_revision,
        "set": set,
    });
    outline::patch_outline_chapter(
        State(state.clone()),
        Path((work_id.to_string(), chapter.to_string())),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .map(|Json(resp)| resp.new_revision)
}

/// Read a chapter row to inspect its `outline_path` / `body_path` columns.
async fn chapter_row(
    pool: &sqlx::SqlitePool,
    work_id: &str,
    chapter: i32,
) -> nexus_local_db::work_chapters::WorkChapterRecord {
    nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, 1)
        .await
        .expect("chapter exists")
        .expect("chapter row present")
}

#[tokio::test]
async fn v175_content_patch_writes_outline_path_and_bumps_revision() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    // Seed an existing outline_path so we patch a pre-existing file.
    let now = chrono::Utc::now().to_rfc3339();
    let rel_outline = "Works/outline-hardening-novel/Outlines/chapters/ch01-outline.md";
    work_chapters::update_outline_path(ctx.state.pool(), &work_id, 1, 1, Some(rel_outline), &now)
        .await
        .expect("set outline_path");
    let ws_root = std::path::PathBuf::from(ctx.state.workspace_path().expect("workspace path set"));
    let outline_abs = ws_root.join(rel_outline);
    tokio::fs::create_dir_all(outline_abs.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&outline_abs, "# Old outline\n")
        .await
        .unwrap();

    let new_rev = chapter_patch(
        &ctx.state,
        &work_id,
        "1",
        0,
        json!({ "content": "## Scene beats\n\n- Open on the harbor\n- Meet the contact" }),
    )
    .await
    .expect("content patch should succeed");
    assert_eq!(new_rev, 1);

    // outline_revision bump persisted to the work-level outline.md frontmatter.
    assert_eq!(current_revision(&ctx.state, &work_id).await, 1);

    // Per-chapter outline file now holds the patched prose.
    let on_disk = tokio::fs::read_to_string(&outline_abs).await.unwrap();
    assert!(
        on_disk.contains("## Scene beats"),
        "outline_path file should hold patched content; got: {on_disk}"
    );
    assert!(
        !on_disk.contains("# Old outline"),
        "outline_path file should not hold stale content; got: {on_disk}"
    );
}

#[tokio::test]
async fn v175_content_patch_seeds_outline_path_when_missing() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    // No outline_path seeded — the handler must derive the fallback path.
    let before = chapter_row(ctx.state.pool(), &work_id, 1).await;
    assert!(
        before.outline_path.as_deref().map_or(true, str::is_empty),
        "precondition: chapter has no outline_path"
    );

    chapter_patch(
        &ctx.state,
        &work_id,
        "1",
        0,
        json!({ "content": "Fresh prose" }),
    )
    .await
    .expect("content patch should seed the outline_path");

    let after = chapter_row(ctx.state.pool(), &work_id, 1).await;
    let derived = after
        .outline_path
        .as_deref()
        .expect("outline_path should now be populated");
    assert_eq!(
        derived,
        "Works/outline-hardening-novel/Outlines/chapters/ch01-outline.md"
    );

    // The derived file was actually written.
    let ws_root = std::path::PathBuf::from(ctx.state.workspace_path().expect("workspace path set"));
    let on_disk = tokio::fs::read_to_string(ws_root.join(derived))
        .await
        .expect("seeded outline file should exist");
    assert!(on_disk.contains("Fresh prose"));
}

#[tokio::test]
async fn v175_content_patch_on_stale_base_revision_returns_conflict() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    // First patch bumps revision to 1.
    chapter_patch(&ctx.state, &work_id, "1", 0, json!({ "content": "first" }))
        .await
        .unwrap();

    // Stale base_revision (0) must conflict (409 OutlineConflictError).
    let err = chapter_patch(
        &ctx.state,
        &work_id,
        "1",
        0, // stale
        json!({ "content": "second" }),
    )
    .await
    .expect_err("stale base_revision should conflict");
    assert!(
        matches!(err, NexusApiError::OutlineConflict { .. }),
        "expected OutlineConflict (409), got {err:?}"
    );
}

#[tokio::test]
async fn v175_content_patch_does_not_touch_body_path() {
    // Body-ownership invariant (architect-LOCKED): a content patch writes ONLY
    // to outline_path. The body_path column AND the body file bytes must be
    // byte-identical before and after.
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let ws_root = std::path::PathBuf::from(ctx.state.workspace_path().expect("workspace path set"));

    // Seed a distinct outline_path and body_path, each with sentinel content.
    let now = chrono::Utc::now().to_rfc3339();
    let rel_outline = "Works/outline-hardening-novel/Outlines/chapters/ch01-outline.md";
    let rel_body = "Works/outline-hardening-novel/Stories/ch01-body.md";
    work_chapters::update_outline_path(ctx.state.pool(), &work_id, 1, 1, Some(rel_outline), &now)
        .await
        .unwrap();
    // Seed the body_path column directly (insert_chapter took None).
    sqlx::query("UPDATE work_chapters SET body_path = ? WHERE work_id = ? AND chapter = ?")
        .bind(rel_body)
        .bind(&work_id)
        .bind(1)
        .execute(ctx.state.pool())
        .await
        .expect("seed body_path");

    let body_abs = ws_root.join(rel_body);
    tokio::fs::create_dir_all(body_abs.parent().unwrap())
        .await
        .unwrap();
    let body_sentinel = "# Chapter body\n\nThe AI owns this prose. It must not change.\n";
    tokio::fs::write(&body_abs, body_sentinel).await.unwrap();
    let outline_abs = ws_root.join(rel_outline);
    tokio::fs::create_dir_all(outline_abs.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&outline_abs, "old outline\n")
        .await
        .unwrap();

    let body_bytes_before = tokio::fs::read(&body_abs).await.unwrap();

    // Patch the outline prose.
    chapter_patch(
        &ctx.state,
        &work_id,
        "1",
        0,
        json!({ "content": "## New outline prose\n\n- Beat one\n- Beat two" }),
    )
    .await
    .expect("content patch should succeed");

    // body_path column unchanged.
    let after = chapter_row(ctx.state.pool(), &work_id, 1).await;
    assert_eq!(after.body_path.as_deref(), Some(rel_body));

    // body file bytes unchanged.
    let body_bytes_after = tokio::fs::read(&body_abs).await.unwrap();
    assert_eq!(
        body_bytes_before, body_bytes_after,
        "body file must be byte-identical after an outline content patch"
    );

    // And the outline file WAS updated (sanity — confirms the patch ran).
    let outline_after = tokio::fs::read_to_string(&outline_abs).await.unwrap();
    assert!(outline_after.contains("New outline prose"));
}

#[tokio::test]
async fn v175_content_patch_rejects_oversized_content() {
    let ctx = test_ctx().await;
    let work_id = setup_work(&ctx.state).await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    // 10 MiB + 1 byte exceeds the OUTLINE_FILE_MAX_BYTES cap.
    let oversized = "x".repeat((10 * 1024 * 1024) + 1);
    let err = chapter_patch(
        &ctx.state,
        &work_id,
        "1",
        0,
        json!({ "content": oversized }),
    )
    .await
    .expect_err("oversized content should be rejected");
    assert!(
        matches!(
            err,
            NexusApiError::BadRequest { ref code, .. } if code == "chapter_outline_content_too_large"
        ),
        "expected BadRequest(chapter_outline_content_too_large), got {err:?}"
    );
}
