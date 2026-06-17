//! Hermetic CLI integration tests for `creator works chronology` (V1.50 T-A P3 T7).
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-auto-chronology.md`
//! Spec: `.mstar/knowledge/specs/novel-writing/auto-chronology.md` §2.2.
//!
//! Covers set/show/advance round-trip (AC §8). Two test layers, all hermetic
//! (no daemon), mirroring `cron_cli.rs`:
//!
//! - **DAO round-trip layer**: opens a fresh temp DB, seeds a Work, and
//!   exercises the set/show/advance path by driving the DAO +
//!   `advance_manual` directly (the handlers are private; the binary surface
//!   tests below cover end-to-end dispatch).
//! - **CLI binary surface layer**: `assert_cmd` drives the compiled `nexus42`
//!   binary to verify the subcommand surface and help text.

use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::auto_chronology::{advance_manual, outline_path, AdvanceOutcome};

// =============================================================================
// DAO round-trip layer (AC §8: set/show/advance)
// =============================================================================

/// AC §8: `chronology set my-work --auto true` round-trips through the DAO.
#[tokio::test]
async fn chronology_set_true_persists() {
    let pool = fresh_seeded_pool().await;
    works::set_auto_chronology(&pool, "wrk_seed", true, "2026-06-18T00:00:00Z")
        .await
        .unwrap();
    assert!(
        works::get_auto_chronology(&pool, "wrk_seed").await.unwrap(),
        "set --auto true must persist the flag"
    );
}

/// AC §8: `chronology set my-work --auto false` disables the flag (default).
#[tokio::test]
async fn chronology_set_false_disables() {
    let pool = fresh_seeded_pool().await;
    works::set_auto_chronology(&pool, "wrk_seed", true, "2026-06-18T00:00:00Z")
        .await
        .unwrap();
    works::set_auto_chronology(&pool, "wrk_seed", false, "2026-06-18T00:00:00Z")
        .await
        .unwrap();
    assert!(
        !works::get_auto_chronology(&pool, "wrk_seed").await.unwrap(),
        "set --auto false must disable the flag"
    );
}

/// AC §8: `show` reports the persisted flag (defaults to false on a fresh Work).
#[tokio::test]
async fn chronology_show_defaults_false() {
    let pool = fresh_seeded_pool().await;
    assert!(
        !works::get_auto_chronology(&pool, "wrk_seed").await.unwrap(),
        "fresh Work must default to auto_chronology=false"
    );
}

/// AC §8 / §4.4: `advance my-work --volume 2 --chapters 3` creates the outline
/// + seeds 3 chapters, regardless of the `auto_chronology` flag.
#[tokio::test]
async fn chronology_advance_round_trip() {
    let pool = fresh_seeded_pool().await;
    let ws = tempfile::tempdir().unwrap();
    // The Work is NOT opted in; manual advance must still work (spec §2.2).
    assert!(
        !works::get_auto_chronology(&pool, "wrk_seed").await.unwrap(),
        "advance target is not opted in"
    );

    let outcome = advance_manual(&pool, ws.path(), "wrk_seed", 2, Some(3))
        .await
        .unwrap();
    match outcome {
        AdvanceOutcome::Advanced {
            next_volume,
            chapters_seeded,
            ..
        } => {
            assert_eq!(next_volume, 2);
            assert_eq!(chapters_seeded, 3);
        }
        other => panic!("advance should succeed, got {other:?}"),
    }

    // Outline created at the spec path layout.
    let outline = outline_path(ws.path(), "seed-ref", 2);
    assert!(outline.exists(), "advance must create the outline");
    assert!(std::fs::read_to_string(&outline)
        .unwrap()
        .contains("Volume 2 Outline"));

    // Chapters seeded in the DB.
    let vol2_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_chapters WHERE work_id = 'wrk_seed' AND volume = 2",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vol2_count, 3, "advance must seed the requested chapters");
}

/// AC §8 (negative): a second `advance` to the same volume is idempotent
/// (returns AlreadyAdvanced, does not clobber).
#[tokio::test]
async fn chronology_advance_idempotent_on_repeat() {
    let pool = fresh_seeded_pool().await;
    let ws = tempfile::tempdir().unwrap();
    advance_manual(&pool, ws.path(), "wrk_seed", 2, None)
        .await
        .unwrap();
    let outcome = advance_manual(&pool, ws.path(), "wrk_seed", 2, None)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        AdvanceOutcome::Skipped {
            reason: nexus_orchestration::auto_chronology::SkipReason::AlreadyAdvanced,
            ..
        }
    ));
}

// =============================================================================
// CLI binary surface (help text — AC §8: subcommand registered)
// =============================================================================

/// `creator works chronology` is registered as a subcommand of `creator works`.
#[test]
fn works_help_lists_chronology_subcommand() {
    use assert_cmd::Command;
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    assert!(
        help.contains("chronology"),
        "creator works --help must list 'chronology' subcommand: {help}"
    );
}

/// `creator works chronology --help` lists set/show/advance.
#[test]
fn chronology_help_lists_set_show_advance() {
    use assert_cmd::Command;
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "chronology", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    for sub in &["set", "show", "advance"] {
        assert!(
            help.contains(sub),
            "creator works chronology --help must list '{sub}': {help}"
        );
    }
}

/// `creator works chronology set --help` documents the `--auto` flag.
#[test]
fn chronology_set_help_documents_auto_flag() {
    use assert_cmd::Command;
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "chronology", "set", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    assert!(
        help.contains("--auto"),
        "chronology set --help must document --auto: {help}"
    );
}

/// `creator works chronology advance --help` documents `--volume` + `--chapters`.
#[test]
fn chronology_advance_help_documents_flags() {
    use assert_cmd::Command;
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "chronology", "advance", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    assert!(
        help.contains("--volume"),
        "chronology advance --help must document --volume: {help}"
    );
    assert!(
        help.contains("--chapters"),
        "chronology advance --help must document --chapters: {help}"
    );
}

// =============================================================================
// Test helpers
// =============================================================================

/// Open a fresh temp DB, run migrations, and seed one Work (`wrk_seed` /
/// `seed-ref`) under `ctr_test` / `default`.
async fn fresh_seeded_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    // Keep the tempdir alive for the test by leaking it (test process is short-lived).
    std::mem::forget(dir);
    let record = sample_work_record("wrk_seed", "seed-ref");
    nexus_local_db::works::create_work(&pool, &record)
        .await
        .unwrap();
    pool
}

/// Build a minimal WorkRecord mirroring `works.rs::sample_work_for_test`.
fn sample_work_record(work_id: &str, work_ref: &str) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "default".to_string(),
        status: "draft".to_string(),
        title: "Test Novel".to_string(),
        long_term_goal: "Test goal".to_string(),
        initial_idea: "An idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-18T00:00:00Z".to_string(),
        updated_at: "2026-06-18T00:00:00Z".to_string(),
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(work_ref.to_string()),
        total_planned_chapters: Some(3),
        current_chapter: 0,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}
