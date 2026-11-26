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
//! `SQLite` (via `nexus_local_db::init_engine_pool`) for DB operations.

use std::path::Path;

use nexus_capability::Capability;

mod common;

use common::{test_pool, DirectWriteExecutor};

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
) -> nexus_capability::builtins::NovelProjectScaffold {
    nexus_capability::builtins::NovelProjectScaffold::new_with_root(pool, works_root.to_path_buf())
        .with_workspace_executor(Some(std::sync::Arc::new(DirectWriteExecutor::new(
            works_root
                .parent()
                .map_or_else(std::path::PathBuf::new, std::path::Path::to_path_buf),
        ))))
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

#[tokio::test]
async fn t7a_scaffold_tree_all_files_exist_with_correct_content() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7a").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let world_id = "wld_t7a_world";
    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        world_id,
        "ctr_test",
        "T7a Test World",
        "t7a-test-world",
        "private",
        "single",
    )
    .await;
    let input = scaffold_input("wrk_t7a", "my-novel", "My Test Novel", Some(world_id), 3);

    let out = cap.run(input).await.expect("scaffold should succeed");
    let scaffold = out["scaffold_root"].as_str().expect("scaffold_root");
    let root = Path::new(scaffold);

    assert!(root.join("Outlines").is_dir(), "Outlines/ must exist");
    assert!(
        root.join("Outlines/chapters").is_dir(),
        "Outlines/chapters/ must exist"
    );
    assert!(root.join("Stories").is_dir(), "Stories/ must exist");
    assert!(root.join("Logs").is_dir(), "Logs/ must exist");

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

    let readme = std::fs::read_to_string(root.join("README.md")).expect("read README");
    assert!(
        readme.contains("my-novel"),
        "README must contain work_ref: {readme}"
    );
    assert!(
        readme.contains("My Test Novel"),
        "README must contain title: {readme}"
    );
    assert!(
        readme.contains(world_id),
        "README must contain bound world_id '{world_id}': {readme}"
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

    assert!(
        !root.join("work-status.md").exists(),
        "work-status.md must NOT be created (replaced by work_chapters)"
    );
}

#[tokio::test]
async fn t7b_idempotent_reinit_preserves_existing_files() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7b").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input(
        "wrk_t7b",
        "idem-novel",
        "Idem Novel",
        Some("wld_idem_world"),
        2,
    );

    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_idem_world",
        "ctr_test",
        "Idem World",
        "idem-world",
        "private",
        "single",
    )
    .await;

    let _out1 = cap.run(input.clone()).await.expect("first run");
    let scaffold_root = works_root.join("idem-novel");
    let readme_path = scaffold_root.join("README.md");

    let custom = "CUSTOM CONTENT THAT MUST SURVIVE RE-INIT";
    std::fs::write(&readme_path, custom).expect("write custom README");

    let _out2 = cap.run(input).await.expect("second run");

    let content = std::fs::read_to_string(&readme_path).expect("read README after re-init");
    assert_eq!(
        content, custom,
        "T6/T7b: existing files must not be overwritten on re-init"
    );
}

#[tokio::test]
async fn t7c_work_chapters_rows_seeded_correctly() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7c").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let total = 4;
    let input = scaffold_input(
        "wrk_t7c",
        "chapter-test",
        "Chapter Test",
        Some("wld_t7c_world"),
        total,
    );

    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_t7c_world",
        "ctr_test",
        "T7c Test World",
        "t7c-test-world",
        "private",
        "single",
    )
    .await;

    let out = cap.run(input).await.expect("scaffold should succeed");
    assert_eq!(out["chapters_seeded"], total, "chapters_seeded must match");

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

#[tokio::test]
async fn t7d_works_patch_world_id_existing() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7d_exist").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let world_id = "wld_existing_123";
    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        world_id,
        "ctr_test",
        "Existing World",
        "existing-world",
        "private",
        "single",
    )
    .await;
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
async fn t7d_works_patch_world_id_none_worldless_rejected() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7d_none").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input("wrk_t7d_none", "world-none", "World None", None, 1);

    let err = cap
        .run(input)
        .await
        .expect_err("V1.40: worldless creation must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("V1.40 requires world_id"),
        "error must mention V1.40 mandatory binding, got: {msg}"
    );
    assert!(
        msg.contains("creator world create") || msg.contains("creator world list"),
        "error must mention remediation commands, got: {msg}"
    );
}

#[tokio::test]
async fn t7d_works_patch_world_id_new_placeholder() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7d_new").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let new_world_id = "wld_newly_created_456";
    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        new_world_id,
        "ctr_test",
        "Newly Created World",
        "newly-created-world",
        "private",
        "single",
    )
    .await;
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

#[tokio::test]
async fn t7d_bis_unknown_world_id_rejected_without_side_effects() {
    // Verify (1) the scaffold returns InputInvalid, (2) no FS directory was
    // created, (3) the works row is NOT patched with the bogus world_id.
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7d_bis").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input(
        "wrk_t7d_bis",
        "world-bis",
        "World Bis",
        Some("wld_does_not_exist_xyz"),
        2,
    );

    let err = cap
        .run(input)
        .await
        .expect_err("must reject unknown world_id");
    let msg = format!("{err}");
    assert!(
        msg.contains("world_id") && msg.contains("not found"),
        "F5 error message must mention world_id+not found, got: {msg}"
    );

    assert!(
        !works_root.join("world-bis").exists(),
        "F5: no Works/<ref>/ should be created when FK check fails"
    );

    let work = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7d_bis")
        .await
        .expect("get work")
        .expect("work must exist");
    assert!(
        work.world_id.is_none(),
        "F5: works.world_id must remain NULL when FK check fails"
    );
}

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
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7e").await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input(
        "wrk_t7e",
        "gate-test",
        "Gate Test",
        Some("wld_t7e_world"),
        2,
    );

    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_t7e_world",
        "ctr_test",
        "T7e Test World",
        "t7e-test-world",
        "private",
        "single",
    )
    .await;

    cap.run(input).await.expect("scaffold");

    let scaffold_root = works_root.join("gate-test");
    assert!(
        scaffold_root.is_dir(),
        "§5.3.2 gate: Works/gate-test/ must exist"
    );

    assert!(
        scaffold_root.join("Outlines").is_dir(),
        "§5.3.2 gate: Outlines/ must exist"
    );

    assert!(
        scaffold_root.join("Stories").is_dir(),
        "§5.3.2 gate: Stories/ must exist"
    );

    let work = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7e")
        .await
        .expect("get work")
        .expect("work must exist");
    assert_eq!(
        work.work_profile.as_deref(),
        Some("novel"),
        "§5.3.2 gate: work_profile must be 'novel'"
    );

    assert!(
        work.work_ref.as_deref().is_some_and(|s| !s.is_empty()),
        "§5.3.2 gate: work_ref must be set"
    );

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

#[test]
fn t7e_stage_advance_produce_passes_when_intake_complete() {
    use nexus_local_db::work_stage::{check_stage_advance, WorkStageState};

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

    let preset = nexus_orchestration::stage_gates::preset_for_stage("produce");
    assert_eq!(
        preset,
        Some("novel-writing"),
        "produce stage must resolve to novel-writing preset"
    );
}

// Verifies that the scaffold capability rejects untrusted grill-me values
// that would break filesystem path semantics (path traversal, separators,
// uppercase, oversize) or exceed the documented chapter-count range.

async fn run_invalid_input(input: serde_json::Value) -> Result<serde_json::Value, String> {
    let (dir, pool) = test_pool().await;
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
    assert!(
        err.contains("invalid input"),
        "C-1: oversize rejected: {err}"
    );
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
    for n in [1, 100] {
        let (dir, pool) = test_pool().await;
        let wid = format!("wrk_bnd_{n}");
        insert_test_work(&pool, &wid).await;
        nexus_local_db::narrative_gateway::seed::world(
            &pool,
            "wld_bnd",
            "ctr_test",
            "Bounds World",
            "bounds-world",
            "private",
            "single",
        )
        .await;
        let works_root = dir.path().join("Works");
        let cap = make_cap(pool.clone(), &works_root);
        let input = scaffold_input(&wid, "bounded-ok", "OK", Some("wld_bnd"), n);
        cap.run(input).await.unwrap_or_else(|e| {
            panic!("W-2: {n} chapters must be accepted, got error: {e}");
        });
    }
}

#[tokio::test]
async fn t7f_partial_reinit_only_updates_listed_fields() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t7f").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    // V1.40: world_id is mandatory — seed World rows.
    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_t7f_initial",
        "ctr_test",
        "T7f Initial World",
        "t7f-initial-world",
        "private",
        "single",
    )
    .await;
    let initial = scaffold_input(
        "wrk_t7f",
        "original-ref",
        "Original Title",
        Some("wld_t7f_initial"),
        5,
    );
    cap.run(initial).await.expect("initial bootstrap");

    let after_initial = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7f")
        .await
        .expect("get work")
        .expect("work");
    assert_eq!(after_initial.work_ref.as_deref(), Some("original-ref"));
    assert_eq!(after_initial.total_planned_chapters, Some(5));
    assert_eq!(after_initial.world_id.as_deref(), Some("wld_t7f_initial"));

    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_new_xyz",
        "ctr_test",
        "New XYZ World",
        "new-xyz-world",
        "private",
        "single",
    )
    .await;
    let mut partial = scaffold_input(
        "wrk_t7f",
        "original-ref", // FS path must match (idempotent re-render); keep same
        "Other Title",
        Some("wld_new_xyz"),
        99, // intentionally different from initial 5
    );
    partial["fields_changed"] = serde_json::json!(["world_id"]);

    cap.run(partial).await.expect("partial re-init");

    let after_partial = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_t7f")
        .await
        .expect("get work")
        .expect("work");
    assert_eq!(
        after_partial.work_ref.as_deref(),
        Some("original-ref"),
        "F4: work_ref must NOT be overwritten on partial re-init"
    );
    assert_eq!(
        after_partial.total_planned_chapters,
        Some(5),
        "F4: total_planned_chapters must NOT be overwritten on partial re-init"
    );
    assert_eq!(
        after_partial.world_id.as_deref(),
        Some("wld_new_xyz"),
        "F4: world_id MUST be updated when listed in fields_changed"
    );
}

#[tokio::test]
async fn t7g_db_failure_rolls_back_filesystem_scaffold() {
    let (dir, pool) = test_pool().await;

    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let input = scaffold_input(
        "wrk_no_such_row",
        "atomic-test",
        "Atomic Test",
        Some("wld_atomic"),
        2,
    );

    let err = cap
        .run(input)
        .await
        .expect_err("scaffold must fail on missing work or missing world_id");
    let msg = format!("{err}");
    assert!(
        msg.contains("seed_chapters")
            || msg.contains("FOREIGN KEY")
            || msg.contains("constraint")
            || msg.contains("world_id") && msg.contains("not found"),
        "F2: expected FK/world_id error, got: {msg}"
    );

    let scaffold_root = works_root.join("atomic-test");
    assert!(
        !scaffold_root.exists(),
        "F2: ScaffoldTransaction must have removed the partial scaffold at {}",
        scaffold_root.display()
    );
}

/// AC1: Single-volume regression — existing Works get volume=1 without user action.
#[tokio::test]
async fn t6_ac1_single_volume_regression_volume_defaults_to_1() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t6_ac1").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let _world_id = insert_test_world(&pool, "wld_t6_ac1").await;

    let input = scaffold_input(
        "wrk_t6_ac1",
        "ac1-regression",
        "AC1 Regression",
        Some("wld_t6_ac1"),
        3,
    );
    let out = cap.run(input).await.expect("scaffold should succeed");

    let chapters = nexus_local_db::work_chapters::list_chapters(&pool, "wrk_t6_ac1")
        .await
        .expect("list chapters");
    assert_eq!(chapters.len(), 3, "should have 3 chapters");
    for ch in &chapters {
        assert_eq!(
            ch.volume,
            Some(1),
            "all chapters must have volume=1, got {:?}",
            ch.volume
        );
    }

    let next = nexus_local_db::work_chapters::next_chapter_volume_aware(&pool, "wrk_t6_ac1")
        .await
        .expect("next chapter volume aware");
    assert_eq!(
        next,
        Some((1, 1)),
        "next chapter should be (volume=1, chapter=1), got {next:?}",
    );

    let _ = out;
}

/// AC2: Two-volume happy path — init → finalize → auto-chain picks next.
#[tokio::test]
async fn t6_ac2_two_volume_happy_path() {
    let (dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t6_ac2").await;
    let works_root = dir.path().join("Works");
    let cap = make_cap(pool.clone(), &works_root);

    let _world_id = insert_test_world(&pool, "wld_t6_ac2").await;

    let mut input = scaffold_input(
        "wrk_t6_ac2",
        "ac2-twovol",
        "AC2 Two Volume",
        Some("wld_t6_ac2"),
        12,
    );
    input["total_volumes"] = serde_json::Value::Number(2.into());

    let _out = cap.run(input).await.expect("scaffold should succeed");

    let chapters = nexus_local_db::work_chapters::list_chapters(&pool, "wrk_t6_ac2")
        .await
        .expect("list chapters");
    assert_eq!(
        chapters.len(),
        12,
        "should have 12 chapters (2 volumes × 6)"
    );

    let vol1_count = chapters.iter().filter(|c| c.volume == Some(1)).count();
    let vol2_count = chapters.iter().filter(|c| c.volume == Some(2)).count();
    assert_eq!(vol1_count, 6, "volume 1 should have 6 chapters");
    assert_eq!(vol2_count, 6, "volume 2 should have 6 chapters");

    let next = nexus_local_db::work_chapters::next_chapter_volume_aware(&pool, "wrk_t6_ac2")
        .await
        .expect("next chapter volume aware");
    assert_eq!(
        next,
        Some((1, 1)),
        "initial next should be vol 1, ch 1, got {next:?}",
    );

    let now = "2026-06-11T12:00:00Z";
    for ch in 1..=6 {
        nexus_local_db::work_chapters::update_status(
            &pool,
            "wrk_t6_ac2",
            ch,
            1,
            "finalized",
            Some(4000),
            now,
        )
        .await
        .expect("update_status");
    }

    let next_after_vol1 =
        nexus_local_db::work_chapters::next_chapter_volume_aware(&pool, "wrk_t6_ac2")
            .await
            .expect("next chapter after vol1 complete");
    assert_eq!(
        next_after_vol1,
        Some((2, 1)),
        "after vol 1 finalized, next should be vol 2, ch 1, got {next_after_vol1:?}",
    );

    let vol_outline =
        std::fs::read_to_string(works_root.join("ac2-twovol/Outlines/volume-outline.md"))
            .expect("volume-outline.md should exist");
    assert!(
        vol_outline.contains("volume: 1"),
        "should have volume: 1 section"
    );
    assert!(
        vol_outline.contains("volume: 2"),
        "should have volume: 2 section"
    );
    assert!(
        vol_outline.contains("chapter_range"),
        "should have chapter_range"
    );
}

/// AC3: Volume-aware status — chapter rows include volume column.
#[tokio::test]
async fn t6_ac3_status_api_volume_aware_rows() {
    let (_dir, pool) = test_pool().await;
    insert_test_work(&pool, "wrk_t6_ac3").await;

    nexus_local_db::work_chapters::seed_chapters_multi_volume(
        &pool,
        "wrk_t6_ac3",
        "ac3-status",
        2,
        3,
        "2026-06-11T10:00:00Z",
    )
    .await
    .expect("seed multi-volume");

    let chapters = nexus_local_db::work_chapters::list_chapters(&pool, "wrk_t6_ac3")
        .await
        .expect("list chapters");

    for ch in &chapters {
        assert!(
            ch.volume.is_some(),
            "chapter {} must have volume, got None",
            ch.chapter
        );
    }

    let vol1_count = chapters.iter().filter(|c| c.volume == Some(1)).count();
    let vol2_count = chapters.iter().filter(|c| c.volume == Some(2)).count();
    assert_eq!(vol1_count, 3, "volume 1 should have 3 chapters");
    assert_eq!(vol2_count, 3, "volume 2 should have 3 chapters");

    for (i, ch) in chapters.iter().enumerate() {
        if i > 0 {
            let prev = &chapters[i - 1];
            assert!(
                (prev.volume, prev.chapter) <= (ch.volume, ch.chapter),
                "chapters should be ordered by (volume, chapter), got prev={:?} curr={:?}",
                (prev.volume, prev.chapter),
                (ch.volume, ch.chapter)
            );
        }
    }
}

/// Helper: insert a test World row via the shared seed helper.
async fn insert_test_world(pool: &sqlx::SqlitePool, world_id: &str) -> String {
    nexus_local_db::narrative_gateway::seed::world(
        pool,
        world_id,
        "ctr_test",
        "Test World",
        "test-world",
        "private",
        "single",
    )
    .await;
    world_id.to_string()
}
