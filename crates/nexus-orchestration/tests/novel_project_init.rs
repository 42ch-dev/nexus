//! Hermetic integration tests for novel-project-init scaffold (T7a–T7e).
//!
//! Covers:
//! - T7a: scaffold tree + template content (regex checks)
//! - T7b: idempotent re-init preserves existing files
//! - T7c: `work_chapters` rows seeded correctly
//! - T7d: works table `PATCHed` with correct `world_id`
//! - T7e: gate-pass verification for downstream `novel-writing`
//!
//! All tests use `tempfile::TempDir` for hermetic workspace and in-memory
//! `SQLite` (via `nexus_local_db::open_pool`) for DB operations.

use std::path::Path;

use nexus_orchestration::capability::Capability;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a fresh `SQLite` pool with all migrations applied.
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tmpdir");
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .expect("open_pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("migrations");
    (pool, dir)
}

/// Insert a minimal Work row for testing.
async fn insert_test_work(pool: &sqlx::SqlitePool, work_id: &str) {
    // SAFETY: INSERT against works — runtime query.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title,
         long_term_goal, initial_idea, intake_status, world_id, story_ref,
         inspiration_log, primary_preset_id, schedule_ids, created_at, updated_at,
         current_stage, stage_status, work_profile, work_ref,
         total_planned_chapters, current_chapter)
         VALUES (?, 'ctr_test', 'default', 'draft', 'Test Work', 'Goal',
         'Initial idea', 'pending', NULL, NULL, '[]', 'novel-writing', '[]',
         '2026-06-07T10:00:00Z', '2026-06-07T10:00:00Z',
         'intake', 'pending', NULL, NULL, NULL, 0)",
    )
    .bind(work_id)
    .execute(pool)
    .await
    .expect("insert test work");
}

/// Build a `NovelProjectScaffold` capability with a real pool and temp works root.
fn make_cap(
    pool: sqlx::SqlitePool,
    works_root: &Path,
) -> nexus_orchestration::capability::builtins::NovelProjectScaffold {
    nexus_orchestration::capability::builtins::NovelProjectScaffold::new_with_root(
        pool,
        works_root.to_path_buf(),
    )
}

/// Build scaffold input JSON.
fn scaffold_input(
    work_id: &str,
    work_ref: &str,
    title: &str,
    world_id: Option<&str>,
    total_chapters: i32,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "creator_id": "ctr_test",
        "work_id": work_id,
        "work_ref": work_ref,
        "title": title,
        "total_planned_chapters": total_chapters,
    });
    if let Some(wid) = world_id {
        obj["world_id"] = serde_json::Value::String(wid.to_string());
    } else {
        obj["world_id"] = serde_json::Value::Null;
    }
    obj
}

// ---------------------------------------------------------------------------
// T7a: scaffold tree + template content
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t7a_scaffold_tree_all_files_exist_with_correct_content() {
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7a").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input("wrk_t7a", "my-novel", "My Test Novel", None, 3);

    let out = cap.run(input).await.expect("scaffold should succeed");
    let scaffold = out["scaffold_root"].as_str().expect("scaffold_root");
    let root = Path::new(scaffold);

    // ── Directories ────────────────────────────────────────────────
    assert!(root.join("Outlines").is_dir(), "Outlines/ must exist");
    assert!(
        root.join("Outlines/chapters").is_dir(),
        "Outlines/chapters/ must exist"
    );
    assert!(root.join("Stories").is_dir(), "Stories/ must exist");
    assert!(root.join("Logs").is_dir(), "Logs/ must exist");

    // ── Files ──────────────────────────────────────────────────────
    assert!(root.join("README.md").is_file(), "README.md must exist");
    assert!(
        root.join("Outlines/foreshadowing.md").is_file(),
        "Outlines/foreshadowing.md must exist"
    );
    assert!(
        root.join("Outlines/event-index.md").is_file(),
        "Outlines/event-index.md must exist"
    );
    assert!(
        root.join("Outlines/volume-outline.md").is_file(),
        "Outlines/volume-outline.md must exist"
    );

    // ── Content regex checks ───────────────────────────────────────
    let readme = std::fs::read_to_string(root.join("README.md")).expect("read README");
    assert!(
        readme.contains("my-novel"),
        "README must contain work_ref: {readme}"
    );
    assert!(
        readme.contains("My Test Novel"),
        "README must contain title: {readme}"
    );
    // Worldless: should contain the "none (worldless)" placeholder
    assert!(
        readme.contains("worldless") || readme.contains("none"),
        "README should indicate worldless for null world_id: {readme}"
    );

    let foreshadow = std::fs::read_to_string(root.join("Outlines/foreshadowing.md"))
        .expect("read foreshadowing");
    assert!(
        foreshadow.contains('F'),
        "foreshadowing.md should contain F### header: {foreshadow}"
    );

    let event_idx =
        std::fs::read_to_string(root.join("Outlines/event-index.md")).expect("read event-index");
    assert!(
        event_idx.contains('E'),
        "event-index.md should contain E### header: {event_idx}"
    );

    let vol_outline = std::fs::read_to_string(root.join("Outlines/volume-outline.md"))
        .expect("read volume-outline");
    assert!(
        vol_outline.contains("my-novel"),
        "volume-outline should contain work_ref: {vol_outline}"
    );

    // ── Verify no Stories/<story_ref> or work-status.md ────────────
    assert!(
        !root.join("work-status.md").exists(),
        "work-status.md must NOT be created (replaced by work_chapters)"
    );
}

// ---------------------------------------------------------------------------
// T7b: idempotent re-init preserves existing files
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t7b_idempotent_reinit_preserves_existing_files() {
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7b").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input("wrk_t7b", "idem-novel", "Idem Novel", None, 2);

    // First run
    let _out1 = cap.run(input.clone()).await.expect("first run");
    let scaffold_root = works_root.join("idem-novel");
    let readme_path = scaffold_root.join("README.md");

    // Overwrite README with custom content
    let custom = "CUSTOM CONTENT THAT MUST SURVIVE RE-INIT";
    std::fs::write(&readme_path, custom).expect("write custom README");

    // Second run (re-init)
    let _out2 = cap.run(input).await.expect("second run");

    // Custom content preserved
    let content = std::fs::read_to_string(&readme_path).expect("read README after re-init");
    assert_eq!(
        content, custom,
        "T6/T7b: existing files must not be overwritten on re-init"
    );
}

// ---------------------------------------------------------------------------
// T7c: work_chapters rows seeded correctly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t7c_work_chapters_rows_seeded_correctly() {
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7c").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let total = 4;
    let input = scaffold_input("wrk_t7c", "chapter-test", "Chapter Test", None, total);

    let out = cap.run(input).await.expect("scaffold should succeed");
    assert_eq!(out["chapters_seeded"], total, "chapters_seeded must match");

    // Verify DB rows
    let chapters = nexus_local_db::work_chapters::list_chapters(&pool, "wrk_t7c")
        .await
        .expect("list chapters");

    assert_eq!(
        chapters.len(),
        usize::try_from(total).unwrap(),
        "must have exactly {total} rows"
    );

    for (i, ch) in chapters.iter().enumerate() {
        let ch_num = i32::try_from(i).unwrap() + 1;
        assert_eq!(ch.chapter, ch_num, "chapter number should be {ch_num}");
        assert_eq!(ch.status, "not_started", "all chapters must be not_started");
        assert!(
            ch.outline_path.is_some(),
            "outline_path must be set for ch{ch_num}"
        );
        assert!(
            ch.body_path.is_some(),
            "body_path must be set for ch{ch_num}"
        );

        // Verify path format per spec §5.4.3
        let outline = ch.outline_path.as_deref().unwrap();
        assert!(
            outline.starts_with("Works/chapter-test/Outlines/chapters/ch"),
            "outline_path format wrong: {outline}"
        );
        assert!(
            outline.ends_with("-outline.md"),
            "outline_path must end with -outline.md: {outline}"
        );

        let body = ch.body_path.as_deref().unwrap();
        assert!(
            body.starts_with("Works/chapter-test/Stories/ch"),
            "body_path format wrong: {body}"
        );
        assert!(
            std::path::Path::new(body)
                .extension()
                .is_some_and(|e| e == "md"),
            "body_path must end with .md: {body}"
        );
    }
}

// ---------------------------------------------------------------------------
// T7d: works table PATCHed with correct world_id (3 branches)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t7d_works_patch_world_id_existing() {
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7d_exist").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let world_id = "wld_existing_123";
    let input = scaffold_input(
        "wrk_t7d_exist",
        "world-exist",
        "World Existing",
        Some(world_id),
        2,
    );

    cap.run(input).await.expect("scaffold");

    let work = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7d_exist")
        .await
        .expect("get work")
        .expect("work must exist");

    assert_eq!(work.work_profile.as_deref(), Some("novel"));
    assert_eq!(work.work_ref.as_deref(), Some("world-exist"));
    assert_eq!(work.total_planned_chapters, Some(2));
    assert_eq!(work.world_id.as_deref(), Some(world_id));
    assert_eq!(work.current_chapter, 0);
}

#[tokio::test]
async fn t7d_works_patch_world_id_none_worldless() {
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7d_none").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input("wrk_t7d_none", "world-none", "World None", None, 1);

    cap.run(input).await.expect("scaffold");

    let work = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7d_none")
        .await
        .expect("get work")
        .expect("work must exist");

    assert_eq!(work.work_profile.as_deref(), Some("novel"));
    assert!(
        work.world_id.is_none(),
        "world_id must be NULL for worldless Work"
    );
}

#[tokio::test]
async fn t7d_works_patch_world_id_new_placeholder() {
    // V1.36 limitation: "create new World" is a placeholder. The test
    // verifies that a world_id string (simulating what a future
    // `creator world create` command would return) is correctly set on
    // the works table.
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7d_new").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let new_world_id = "wld_newly_created_456";
    let input = scaffold_input(
        "wrk_t7d_new",
        "world-new",
        "World New",
        Some(new_world_id),
        3,
    );

    cap.run(input).await.expect("scaffold");

    let work = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7d_new")
        .await
        .expect("get work")
        .expect("work must exist");

    assert_eq!(
        work.world_id.as_deref(),
        Some(new_world_id),
        "V1.36: world_id from 'create new' placeholder must be set on works table"
    );
    assert_eq!(work.work_profile.as_deref(), Some("novel"));
}

// ---------------------------------------------------------------------------
// T7e: gate-pass verification (init enables downstream novel-writing gate)
// ---------------------------------------------------------------------------
//
// The novel-writing gate (§5.3.2) requires:
//   - work_profile == novel
//   - work_ref non-null
//   - intake_status == complete  (set by creative-brief-intake, not init)
//   - filesystem: Works/<work_ref>/ exists
//   - filesystem: Works/<work_ref>/Outlines/ exists
//   - filesystem: Works/<work_ref>/Stories/ exists
//   - previous_preset: novel-project-init, status: complete
//
// The gate evaluator for `previous_preset` and `filesystem` checks is not
// yet fully implemented in the engine as of P1. This test verifies what IS
// available: the scaffold creates the filesystem structure and patches the
// work fields correctly. The previous_preset gate requires engine-side
// tracking of preset completion history, which is TBD in a future plan.
//
// For T7e, we verify the state that novel-writing's gates would check:
//   (a) filesystem directories exist  (scaffold creates them)
//   (b) work fields are set           (PATCH updates them)
//   (c) intake_status is NOT set by init (init does not claim to complete intake)

#[tokio::test]
async fn t7e_gate_pass_init_enables_novel_writing_filesystem_gates() {
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7e").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input("wrk_t7e", "gate-test", "Gate Test", None, 2);

    cap.run(input).await.expect("scaffold");

    // Verify §5.3.2 gate conditions that init IS responsible for:

    // (1) filesystem: Works/<work_ref>/ must exist
    let scaffold_root = works_root.join("gate-test");
    assert!(
        scaffold_root.is_dir(),
        "§5.3.2 gate: Works/gate-test/ must exist"
    );

    // (2) filesystem: Works/<work_ref>/Outlines/ must exist
    assert!(
        scaffold_root.join("Outlines").is_dir(),
        "§5.3.2 gate: Outlines/ must exist"
    );

    // (3) filesystem: Works/<work_ref>/Stories/ must exist
    assert!(
        scaffold_root.join("Stories").is_dir(),
        "§5.3.2 gate: Stories/ must exist"
    );

    // (4) work_profile == novel
    let work = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7e")
        .await
        .expect("get work")
        .expect("work must exist");
    assert_eq!(
        work.work_profile.as_deref(),
        Some("novel"),
        "§5.3.2 gate: work_profile must be 'novel'"
    );

    // (5) work_ref non-null
    assert!(
        work.work_ref.as_deref().is_some_and(|s| !s.is_empty()),
        "§5.3.2 gate: work_ref must be set"
    );

    // (6) intake_status is NOT changed by init (remains "pending")
    //     This verifies that init does NOT falsely claim intake completion.
    assert_eq!(
        work.intake_status, "pending",
        "§5.3.2 gate: init must not change intake_status — creative-brief-intake owns that"
    );

    // NOTE: The `previous_preset` gate (novel-project-init complete) is NOT
    // verifiable here because the engine does not yet track preset completion
    // history per-Work. That mechanism is TBD in a future plan. When it is
    // implemented, a follow-up test should verify:
    //   - After init session completes, `novel-writing` schedule admission passes
    //     the previous_preset gate.
}

// ---------------------------------------------------------------------------
// T7e stub: stage gate advance passes after init + intake
// ---------------------------------------------------------------------------

#[test]
fn t7e_stage_advance_produce_passes_when_intake_complete() {
    // Verify the FL-E stage advance gate for produce (novel-writing)
    // passes when the Work is at intake complete with intake_status complete.
    // This is a synchronous gate check (stage_gates.rs) — not the full
    // preset gate evaluator.
    use nexus_orchestration::stage_gates::{check_stage_advance, WorkStageState};

    let work = WorkStageState {
        current_stage: "intake".to_string(),
        stage_status: "complete".to_string(),
        intake_status: "complete".to_string(),
    };

    let result = check_stage_advance(&work, "research", false);
    assert!(
        result.is_ok(),
        "stage advance intake→research should pass when intake complete"
    );

    // From research→produce (novel-writing) also passes
    let at_research = WorkStageState {
        current_stage: "research".to_string(),
        stage_status: "complete".to_string(),
        intake_status: "complete".to_string(),
    };
    let result = check_stage_advance(&at_research, "produce", false);
    assert!(
        result.is_ok(),
        "stage advance research→produce should pass when research complete"
    );

    // Verify produce resolves to novel-writing
    let preset = nexus_orchestration::stage_gates::preset_for_stage("produce");
    assert_eq!(
        preset,
        Some("novel-writing"),
        "produce stage must resolve to novel-writing preset"
    );
}

// ---------------------------------------------------------------------------
// T7a-bis: Input sanitization (F1 — fixes C-1, C-4, W-2)
// ---------------------------------------------------------------------------
//
// Verifies that the scaffold capability rejects untrusted grill-me values
// that would break filesystem path semantics (path traversal, separators,
// uppercase, oversize) or exceed the documented chapter-count range.

async fn run_invalid_input(input: serde_json::Value) -> Result<serde_json::Value, String> {
    let (pool, dir) = fresh_pool().await;
    insert_test_work(&pool, "wrk_t7abis").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);
    cap.run(input).await.map_err(|e| e.to_string())
}

#[tokio::test]
async fn t7a_bis_rejects_dotdot_work_ref() {
    let input = scaffold_input("wrk_t7abis", "..", "Bad", None, 1);
    let err = run_invalid_input(input).await.unwrap_err();
    assert!(
        err.contains("invalid input") || err.contains("path-traversal") || err.contains("work_ref"),
        "C-1: dotdot work_ref must be rejected, got: {err}"
    );
}

#[tokio::test]
async fn t7a_bis_rejects_slash_work_ref() {
    let input = scaffold_input("wrk_t7abis", "foo/bar", "Bad", None, 1);
    let err = run_invalid_input(input).await.unwrap_err();
    assert!(err.contains("invalid input"), "C-1: slash rejected: {err}");
}

#[tokio::test]
async fn t7a_bis_rejects_empty_work_ref() {
    let input = scaffold_input("wrk_t7abis", "", "Bad", None, 1);
    let err = run_invalid_input(input).await.unwrap_err();
    assert!(err.contains("invalid input"), "C-1: empty rejected: {err}");
}

#[tokio::test]
async fn t7a_bis_rejects_uppercase_work_ref() {
    let input = scaffold_input("wrk_t7abis", "MyNovel", "Bad", None, 1);
    let err = run_invalid_input(input).await.unwrap_err();
    assert!(
        err.contains("lowercase") || err.contains("invalid"),
        "C-4: uppercase rejected: {err}"
    );
}

#[tokio::test]
async fn t7a_bis_rejects_oversize_work_ref() {
    let big = "a".repeat(65);
    let input = scaffold_input("wrk_t7abis", &big, "Bad", None, 1);
    let err = run_invalid_input(input).await.unwrap_err();
    assert!(err.contains("invalid input"), "C-1: oversize rejected: {err}");
}

#[tokio::test]
async fn t7a_bis_chapters_zero_rejected() {
    let input = scaffold_input("wrk_t7abis", "ok", "Bad", None, 0);
    let err = run_invalid_input(input).await.unwrap_err();
    assert!(
        err.contains("total_planned_chapters"),
        "W-2: 0 chapters rejected: {err}"
    );
}

#[tokio::test]
async fn t7a_bis_chapters_over_max_rejected() {
    let input = scaffold_input("wrk_t7abis", "ok", "Bad", None, 101);
    let err = run_invalid_input(input).await.unwrap_err();
    assert!(
        err.contains("total_planned_chapters"),
        "W-2: 101 chapters rejected: {err}"
    );
}

#[tokio::test]
async fn t7a_bis_chapters_bounds_accepted() {
    // boundary check: 1 and 100 must be accepted
    for n in [1, 100] {
        let (pool, dir) = fresh_pool().await;
        let wid = format!("wrk_bnd_{n}");
        insert_test_work(&pool, &wid).await;
        let works_root = dir.path().join("Works");
        let cap = make_cap(pool.clone(), &works_root);
        let input = scaffold_input(&wid, "bounded-ok", "OK", None, n);
        cap.run(input).await.unwrap_or_else(|e| {
            panic!("W-2: {n} chapters must be accepted, got error: {e}");
        });
    }
}

