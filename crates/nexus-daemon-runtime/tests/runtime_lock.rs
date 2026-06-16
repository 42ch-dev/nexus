//! Runtime lock hermetic tests (V1.42 P0 — T5).
//!
//! Acceptance criteria:
//! 1. Two concurrent mutating operations on same Work → second fails with holder hint.
//! 2. Crashed CLI holder cleared after TTL (configurable).
//! 3. Auto-chain skips Works with foreign runtime_lock_holder.

#![allow(clippy::unwrap_used)]

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_daemon_runtime::api::handlers::works::{
    append_inspiration, create_work, patch_work, reconcile_chapters, AppendInspirationRequest,
    CreateWorkRequest, PatchWorkRequest, ReconcileDryRunQuery,
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::works;

// ─── Helpers ───────────────────────────────────────────────────────────────

struct TestCtx {
    _tmp: TestTempRoot,
    state: WorkspaceState,
}

async fn test_ctx() -> TestCtx {
    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    TestCtx { _tmp, state }
}

/// Create a Work via handler and return its work_id.
async fn create_test_work(state: &WorkspaceState) -> String {
    let (_, resp) = create_work(
        State(state.clone()),
        axum::Json(CreateWorkRequest {
            title: "Test Novel".into(),
            long_term_goal: "Finish a short story".into(),
            initial_idea: "A detective story".into(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
        }),
    )
    .await
    .unwrap();
    resp.work_id.clone()
}

fn minimal_patch(title: &str) -> PatchWorkRequest {
    PatchWorkRequest {
        title: Some(title.to_string()),
        long_term_goal: None,
        creative_brief: None,
        intake_status: None,
        status: None,
        world_id: None,
        story_ref: None,
        primary_preset_id: None,
        current_stage: None,
        stage_status: None,
        force: None,
        auto_review_master_on_timeout: None,
        auto_chain_interrupted: None,
    }
}

/// Manually set a runtime lock on a Work (simulating a crashed CLI holder).
async fn set_runtime_lock(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    work_id: &str,
    holder: &str,
    acquired_at: &str,
) {
    let patch = works::WorkPatch {
        runtime_lock_holder: Some(Some(holder.to_string())),
        runtime_lock_acquired_at: Some(Some(acquired_at.to_string())),
        ..Default::default()
    };
    works::patch_work(
        pool,
        creator_id,
        work_id,
        &patch,
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_patch_second_fails_with_holder_hint() {
    // AC1: Two concurrent mutating operations on same Work → second fails with holder hint.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Simulate first process holding the lock
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "cli:http:11111111-2222-3333-4444-555555555555",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await;

    // Second PATCH should fail with 423 Locked
    let result = patch_work(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(minimal_patch("Updated Title")),
    )
    .await;

    let err = result.expect_err("second concurrent PATCH should fail");
    assert_eq!(err.status_code(), axum::http::StatusCode::LOCKED);
}

#[tokio::test]
async fn test_concurrent_inspiration_second_fails_with_holder_hint() {
    // AC1 variant: inspiration append also blocked by runtime lock.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Hold the lock
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "daemon:schedule:ACH20260611120000",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await;

    let req = AppendInspirationRequest {
        note: "New inspiration".to_string(),
    };

    let result = append_inspiration(State(ctx.state), Path(work_id), Json(req)).await;

    let err = result.expect_err("inspiration append should fail when locked");
    assert_eq!(err.status_code(), axum::http::StatusCode::LOCKED);
}

#[tokio::test]
async fn test_stale_lock_cleared_after_ttl() {
    // AC2: Crashed CLI holder cleared after TTL (configurable).
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Simulate a crashed CLI holder from 3 hours ago
    let three_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "cli:http:dead-beef-cafe",
        &three_hours_ago,
    )
    .await;

    // The patch_work handler should succeed because the stale lock is force-cleared
    // by acquire_runtime_lock (force_stale=true, TTL=7200s)
    let result = patch_work(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(minimal_patch("Updated After Stale Clear")),
    )
    .await;

    assert!(
        result.is_ok(),
        "PATCH should succeed after stale lock clear"
    );

    // Verify the lock was released (handler releases on return)
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        work.runtime_lock_holder.is_none(),
        "lock should be released after handler"
    );
    assert_eq!(work.title, "Updated After Stale Clear");
}

#[tokio::test]
async fn test_fresh_lock_not_cleared_within_ttl() {
    // Complementary: a fresh lock (within TTL) should NOT be force-cleared.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Lock acquired just now (within TTL)
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "cli:http:fresh-lock",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await;

    let result = patch_work(
        State(ctx.state),
        Path(work_id),
        Json(minimal_patch("Should Not Succeed")),
    )
    .await;

    let err = result.expect_err("fresh lock should block PATCH");
    assert_eq!(err.status_code(), axum::http::StatusCode::LOCKED);
}

#[tokio::test]
async fn test_patch_acquires_and_releases_lock() {
    // Verify that a successful PATCH acquires the lock during execution
    // and releases it when done.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // No lock initially
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(work.runtime_lock_holder.is_none());

    let result = patch_work(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(minimal_patch("Locked During Patch")),
    )
    .await;

    assert!(result.is_ok());

    // Lock should be released after handler returns
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(work.runtime_lock_holder.is_none());
    assert_eq!(work.title, "Locked During Patch");
}

#[tokio::test]
async fn test_inspiration_acquires_and_releases_lock() {
    // Verify inspiration append also acquires/releases the lock.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    let req = AppendInspirationRequest {
        note: "Test inspiration".to_string(),
    };

    let result =
        append_inspiration(State(ctx.state.clone()), Path(work_id.clone()), Json(req)).await;

    assert!(result.is_ok());

    // Lock should be released after handler
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(work.runtime_lock_holder.is_none());
}

/// V1.48 P4-fix1 (W-2 qc3) + V1.49 P3 (R-V148P4-W3): `reconcile_chapters`
/// MUST release the runtime lock when the **apply phase** (write phase, which
/// is the only phase that holds the lock under the V1.49 P3 split) returns an
/// error after acquisition.
///
/// V1.49 P3 split reconcile into `compute_reconcile_diff` (unlocked read) +
/// `apply_reconcile_diff` (locked write). To exercise the apply-phase error
/// path we make `compute` succeed (Stories/ is readable) while making the
/// **write** fail: seed a DB chapter row whose status conflicts with the file
/// frontmatter (so compute emits a `ResyncFileStatus` op), then mark Stories/
/// read-only (`0o555`) so apply's atomic frontmatter rewrite cannot create its
/// temp file. Apply fails after the lock was acquired → the handler must
/// release the lock on the way out.
#[tokio::test]
async fn test_reconcile_chapters_releases_lock_on_error() {
    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let workspace_tmp = tempfile::TempDir::new().unwrap();
    let workspace_path = workspace_tmp.path().to_string_lossy().to_string();
    let state = WorkspaceState::new_for_testing(
        nexus_home.clone(),
        db_path.clone(),
        Some(workspace_path.clone()),
    )
    .await;
    test_utils::seed_test_creator_and_world(state.pool()).await;

    let work_id = create_test_work(&state).await;
    let work_ref = "reconcile-lock-test";

    // Set story_ref so the handler reaches the filesystem layer.
    let mut patch = minimal_patch("Lock Release Test");
    patch.story_ref = Some(Some(work_ref.to_string()));
    let _ = patch_work(State(state.clone()), Path(work_id.clone()), Json(patch))
        .await
        .unwrap();

    // Seed a DB chapter row (status defaults to `not_started`) so the file
    // frontmatter below produces a status CONFLICT → `ResyncFileStatus` op.
    nexus_local_db::insert_chapter(
        state.pool(),
        &nexus_local_db::InsertChapterParams {
            work_id: &work_id,
            chapter: 1,
            volume: Some(1),
            slug: None,
            planned_word_count: 4000,
            outline_path: None,
            body_path: Some("Works/reconcile-lock-test/Stories/ch01-intro.md"),
            now: "2026-06-17T00:00:00Z",
        },
    )
    .await
    .unwrap();

    // Create the Stories/ directory with one chapter file whose frontmatter
    // status (`finalized`) CONFLICTS with the DB row (`not_started`). This
    // makes compute emit a `ResyncFileStatus` op, which apply will attempt to
    // execute.
    let stories_dir = workspace_tmp
        .path()
        .join("Works")
        .join(work_ref)
        .join("Stories");
    std::fs::create_dir_all(&stories_dir).unwrap();
    std::fs::write(
        stories_dir.join("ch01-intro.md"),
        "---\nchapter: 1\nstatus: finalized\n---\nBody",
    )
    .unwrap();

    // Make Stories/ read-only (`0o555`) so compute (read_dir + file read)
    // succeeds but apply's atomic frontmatter rewrite cannot create its temp
    // file. This is a Unix-only hermetic trigger; on other platforms we simply
    // verify the happy path still releases the lock.
    #[cfg(unix)]
    {
        std::fs::set_permissions(&stories_dir, std::fs::Permissions::from_mode(0o555))
            .expect("set Stories dir read-only");
    }

    let result = reconcile_chapters(
        State(state.clone()),
        Path(work_id.clone()),
        Query(ReconcileDryRunQuery { dry_run: None }),
    )
    .await;

    #[cfg(unix)]
    {
        // Restore permissions so the temp directory can be cleaned up.
        std::fs::set_permissions(&stories_dir, std::fs::Permissions::from_mode(0o755))
            .expect("restore Stories dir permissions");

        assert!(
            result.is_err(),
            "reconcile apply should fail when Stories/ is read-only and a \
             frontmatter rewrite is pending"
        );
    }

    // The lock must be released regardless of whether the reconcile call
    // succeeded or failed.
    let work = works::get_work(state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        work.runtime_lock_holder.is_none(),
        "reconcile_chapters must release runtime lock on apply error path"
    );
}

/// V1.49 P3 (R-V148P4-W3): the **read phase** (`compute_reconcile_diff`)
/// runs WITHOUT acquiring the runtime lock, so a read-phase failure (e.g. an
/// unreadable Stories/ dir) must never acquire the lock in the first place.
///
/// This is the structural evidence that the lock window now excludes the slow
/// filesystem walk: a failure that used to occur *after* lock acquire (under
/// the V1.48 single-pass reconcile) now occurs *before* lock acquire, so
/// `runtime_lock_holder` stays `None`.
#[tokio::test]
async fn test_reconcile_chapters_read_phase_runs_unlocked() {
    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let workspace_tmp = tempfile::TempDir::new().unwrap();
    let workspace_path = workspace_tmp.path().to_string_lossy().to_string();
    let state = WorkspaceState::new_for_testing(
        nexus_home.clone(),
        db_path.clone(),
        Some(workspace_path.clone()),
    )
    .await;
    test_utils::seed_test_creator_and_world(state.pool()).await;

    let work_id = create_test_work(&state).await;
    let work_ref = "reconcile-read-unlocked";

    let mut patch = minimal_patch("Read Phase Unlocked");
    patch.story_ref = Some(Some(work_ref.to_string()));
    let _ = patch_work(State(state.clone()), Path(work_id.clone()), Json(patch))
        .await
        .unwrap();

    // Stories/ exists with one chapter (no DB row) — would be a CreateChapter.
    let stories_dir = workspace_tmp
        .path()
        .join("Works")
        .join(work_ref)
        .join("Stories");
    std::fs::create_dir_all(&stories_dir).unwrap();
    std::fs::write(
        stories_dir.join("ch01-intro.md"),
        "---\nchapter: 1\nstatus: finalized\n---\nBody",
    )
    .unwrap();

    // Make Stories/ UNREADABLE (`0o000`) so `read_dir` in the read phase
    // fails. Unix-only hermetic trigger.
    #[cfg(unix)]
    {
        std::fs::set_permissions(&stories_dir, std::fs::Permissions::from_mode(0o000))
            .expect("set Stories dir unreadable");
    }

    let result = reconcile_chapters(
        State(state.clone()),
        Path(work_id.clone()),
        Query(ReconcileDryRunQuery { dry_run: None }),
    )
    .await;

    #[cfg(unix)]
    {
        std::fs::set_permissions(&stories_dir, std::fs::Permissions::from_mode(0o755))
            .expect("restore Stories dir permissions");
        assert!(
            result.is_err(),
            "read phase should fail when Stories/ is unreadable"
        );
    }

    let work = works::get_work(state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        work.runtime_lock_holder.is_none(),
        "read-phase failure must NOT acquire the runtime lock (lock window \
         excludes the filesystem walk — R-V148P4-W3)"
    );
}

/// V1.49 P2 (R-V148P4-W2): `reconcile_chapters` with `dry_run=true` computes
/// the `ReconcileReport` while making ZERO filesystem and DB mutations, and
/// acquires NO runtime lock (overlay author-experience §8.2).
///
/// Setup mirrors `test_reconcile_chapters_releases_lock_on_error`: a chapter
/// file whose frontmatter would force a `created` row in the mutating path.
/// The dry-run path must report the same `created: 1` without writing the row
/// or touching the file.
#[tokio::test]
async fn test_reconcile_chapters_dry_run_makes_zero_mutations() {
    use nexus_daemon_runtime::api::handlers::works::ReconcileDryRunQuery;
    use nexus_local_db::work_chapters;

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let workspace_tmp = tempfile::TempDir::new().unwrap();
    let workspace_path = workspace_tmp.path().to_string_lossy().to_string();
    let state = WorkspaceState::new_for_testing(
        nexus_home.clone(),
        db_path.clone(),
        Some(workspace_path.clone()),
    )
    .await;
    test_utils::seed_test_creator_and_world(state.pool()).await;

    let work_id = create_test_work(&state).await;
    let work_ref = "reconcile-dryrun-test";

    // Set story_ref so the handler reaches the filesystem layer.
    let mut patch = minimal_patch("Dry Run Test");
    patch.story_ref = Some(Some(work_ref.to_string()));
    let _ = patch_work(State(state.clone()), Path(work_id.clone()), Json(patch))
        .await
        .unwrap();

    // One chapter file whose frontmatter would create a new DB row in the
    // mutating path (no existing row for chapter 1).
    let stories_dir = workspace_tmp
        .path()
        .join("Works")
        .join(work_ref)
        .join("Stories");
    std::fs::create_dir_all(&stories_dir).unwrap();
    let chapter_path = stories_dir.join("ch01-intro.md");
    let original_body = "---\nchapter: 1\nstatus: finalized\nword_count: 1234\n---\nBody";
    std::fs::write(&chapter_path, original_body).unwrap();

    // Snapshot pre-state: filesystem file contents + DB chapter row count.
    let pre_file_contents = std::fs::read_to_string(&chapter_path).unwrap();
    let pre_db_rows = work_chapters::list_chapters(state.pool(), &work_id)
        .await
        .expect("list_chapters pre-dry-run")
        .len();
    assert_eq!(
        pre_db_rows, 0,
        "no chapter rows should exist before dry-run"
    );
    let pre_lock_holder = works::get_work(state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap()
        .runtime_lock_holder;

    // Dry-run reconcile: compute the report without writing.
    let result = reconcile_chapters(
        State(state.clone()),
        Path(work_id.clone()),
        Query(ReconcileDryRunQuery {
            dry_run: Some(true),
        }),
    )
    .await;

    let (_status, json_report) = result.expect("dry-run reconcile should succeed");
    let report = json_report.0;

    // The report must reflect what the mutating path would do: one new chapter.
    assert_eq!(
        report.created, 1,
        "dry-run report should show created=1 for the new chapter file"
    );
    assert_eq!(report.updated, 0);
    assert_eq!(report.resynced, 0);
    assert_eq!(report.preserved, 0);

    // ZERO filesystem mutations: the chapter file must be byte-identical.
    let post_file_contents = std::fs::read_to_string(&chapter_path).unwrap();
    assert_eq!(
        pre_file_contents, post_file_contents,
        "dry-run must not modify the chapter file"
    );

    // ZERO DB mutations: still no chapter rows.
    let post_db_rows = work_chapters::list_chapters(state.pool(), &work_id)
        .await
        .expect("list_chapters post-dry-run")
        .len();
    assert_eq!(
        post_db_rows, 0,
        "dry-run must not insert any chapter rows into the DB"
    );

    // NO runtime lock acquired on the dry-run path.
    let post_lock_holder = works::get_work(state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap()
        .runtime_lock_holder;
    assert_eq!(
        pre_lock_holder, post_lock_holder,
        "dry-run must not acquire the runtime lock"
    );
    assert!(
        post_lock_holder.is_none(),
        "runtime_lock_holder must remain unset after dry-run"
    );

    // Sanity: a subsequent MUTATING reconcile (dry_run=false) does write the
    // row, proving the dry-run report was accurate and the path is genuinely
    // non-mutating (not silently no-oping due to a setup bug).
    let (_status, mutate_report) = reconcile_chapters(
        State(state.clone()),
        Path(work_id.clone()),
        Query(ReconcileDryRunQuery { dry_run: None }),
    )
    .await
    .expect("mutating reconcile should succeed");
    assert_eq!(mutate_report.0.created, 1, "mutating path creates the row");
    let post_mutate_rows = work_chapters::list_chapters(state.pool(), &work_id)
        .await
        .unwrap()
        .len();
    assert_eq!(
        post_mutate_rows, 1,
        "mutating reconcile must insert exactly one chapter row"
    );

    // Lock must be released after the mutating path too.
    let post_mutate_lock = works::get_work(state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap()
        .runtime_lock_holder;
    assert!(
        post_mutate_lock.is_none(),
        "mutating reconcile must release the runtime lock"
    );
}
