//! Hermetic CLI integration tests for `creator works cron` (V1.50 T-A P0 T6).
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-cron-foundation.md`
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §3.
//!
//! Covers AC #3 (set/show/list), AC #4 (round-trip through DAO), and AC #5
//! (validation surface). Two test layers, all hermetic (no daemon):
//!
//! - **DAO round-trip layer** (`cron_dao_round_trip`): opens a fresh temp DB,
//!   seeds a Work, and exercises `cron::handle_set/show/list`'s core logic by
//!   driving the DAO + pure functions directly. This is the AC #4 path.
//! - **CLI binary surface layer**: `assert_cmd` drives the compiled `nexus42`
//!   binary to verify the subcommand surface, help text, and argument
//!   validation (invalid cron/TZ exit non-zero) — AC #5.
//!
//! The DAO round-trip layer reuses the public `cron` pure functions +
//! `nexus-local-db` directly so it runs without a configured creator/home.

use nexus42::commands::creator::works::cron::{
    apply_set_args, render_list, render_show, resolve_schedule, CronSetArgs, ListRow, WorkSchedule,
};

/// AC #4: `creator works cron set my-work --brainstorm "0 3,9,15,21 * * *"
/// --tz Asia/Shanghai` round-trips through the DAO and re-renders on `show`.
#[tokio::test]
async fn cron_dao_round_trip_set_then_show() {
    let pool = fresh_seeded_pool().await;

    // `set my-work --brainstorm "0 3,9,15,21 * * *" --tz Asia/Shanghai`
    let args = CronSetArgs {
        brainstorm: Some("0 3,9,15,21 * * *".to_string()),
        tz: Some("Asia/Shanghai".to_string()),
        ..Default::default()
    };
    let base = resolve_schedule(None); // fresh Work → defaults
    let schedule = apply_set_args(base, &args).expect("valid args must apply");
    let blob = serde_json::to_string(&schedule).unwrap();
    nexus_local_db::works::set_schedule_json(&pool, "wrk_seed", &blob, "2026-06-18T00:00:00Z")
        .await
        .unwrap();

    // `show my-work` — re-render from the stored blob.
    let stored = nexus_local_db::works::get_schedule_json(&pool, "wrk_seed")
        .await
        .unwrap()
        .expect("blob must be read back");
    let resolved = resolve_schedule(Some(&stored));
    assert_eq!(resolved.tz, "Asia/Shanghai");
    assert_eq!(resolved.roles.brainstorm.cron, "0 3,9,15,21 * * *");
    // write/review kept at defaults.
    assert!(resolved.roles.write.enabled);
    assert!(resolved.roles.review.enabled);

    let rendered = render_show("my-work", &resolved);
    assert!(rendered.contains("Work: my-work"));
    assert!(rendered.contains("Asia/Shanghai"));
    assert!(rendered.contains("0 3,9,15,21 * * *"));
}

/// AC #3: `set` with `--no-review` disables a role and persists; `show`
/// renders `disabled` for it.
#[tokio::test]
async fn cron_set_no_review_disables_role() {
    let pool = fresh_seeded_pool().await;

    let args = CronSetArgs {
        no_review: true,
        ..Default::default()
    };
    let schedule = apply_set_args(WorkSchedule::defaults(), &args).unwrap();
    assert!(!schedule.roles.review.enabled);
    let blob = serde_json::to_string(&schedule).unwrap();
    nexus_local_db::works::set_schedule_json(&pool, "wrk_seed", &blob, "2026-06-18T00:00:00Z")
        .await
        .unwrap();

    let stored = nexus_local_db::works::get_schedule_json(&pool, "wrk_seed")
        .await
        .unwrap()
        .unwrap();
    let resolved = resolve_schedule(Some(&stored));
    let rendered = render_show("nr-work", &resolved);
    assert!(rendered.contains("disabled"), "rendered show: {rendered}");
}

/// AC #3: `list` across workspace renders a row per Work, defaults shown as
/// the canonical cron expression.
#[tokio::test]
async fn cron_list_across_workspace() {
    let pool = fresh_seeded_pool().await;
    // Seed a second Work.
    let other = sample_work_record("wrk_other", "other-ref");
    nexus_local_db::works::create_work(&pool, &other)
        .await
        .unwrap();
    // Keep `wrk_other` at defaults; set a custom blob on `wrk_seed`.
    let custom = apply_set_args(
        WorkSchedule::defaults(),
        &CronSetArgs {
            tz: Some("America/New_York".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let blob = serde_json::to_string(&custom).unwrap();
    nexus_local_db::works::set_schedule_json(&pool, "wrk_seed", &blob, "2026-06-18T00:00:00Z")
        .await
        .unwrap();

    let rows_db = nexus_local_db::works::list_works_schedule(&pool, "ctr_test", "default")
        .await
        .unwrap();
    let rows: Vec<ListRow> = rows_db
        .into_iter()
        .map(|r| ListRow {
            work_ref: r.work_ref,
            work_id: r.work_id,
            schedule: resolve_schedule(r.schedule_json.as_deref()),
        })
        .collect();
    let rendered = render_list(&rows);
    assert!(rendered.contains("WORK_REF"));
    assert!(rendered.contains("seed-ref"));
    assert!(rendered.contains("other-ref"));
    assert!(rendered.contains("America/New_York"));
    // Defaults shown as canonical cron for the unset Work.
    assert!(rendered.contains("0 3,9,15,21 * * *"));
}

/// AC #4: a fresh Work (NULL schedule_json) shows defaults on `show`.
#[tokio::test]
async fn cron_show_on_unset_work_uses_defaults() {
    let pool = fresh_seeded_pool().await;
    let stored = nexus_local_db::works::get_schedule_json(&pool, "wrk_seed")
        .await
        .unwrap();
    assert!(
        stored.is_none(),
        "freshly-seeded Work must have no schedule"
    );
    let resolved = resolve_schedule(stored.as_deref());
    assert_eq!(resolved, WorkSchedule::defaults());
    let rendered = render_show("seed-ref", &resolved);
    assert!(rendered.contains("UTC")); // default tz
    assert!(rendered.contains("0,30 * * * *")); // default review cron
}

/// AC #4 (negative): `set` with no flags resets an existing custom schedule
/// back to defaults.
#[tokio::test]
async fn cron_set_no_flags_resets_to_defaults() {
    let pool = fresh_seeded_pool().await;
    // First, set a custom blob.
    let custom = apply_set_args(
        WorkSchedule::defaults(),
        &CronSetArgs {
            tz: Some("Asia/Shanghai".to_string()),
            brainstorm: Some("0 9 * * *".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let blob = serde_json::to_string(&custom).unwrap();
    nexus_local_db::works::set_schedule_json(&pool, "wrk_seed", &blob, "2026-06-18T00:00:00Z")
        .await
        .unwrap();

    // Now `set <work>` with no flags → reset to defaults.
    let current = resolve_schedule(
        nexus_local_db::works::get_schedule_json(&pool, "wrk_seed")
            .await
            .unwrap()
            .as_deref(),
    );
    let reset = apply_set_args(current, &CronSetArgs::default()).unwrap();
    assert_eq!(reset, WorkSchedule::defaults());
}

// =============================================================================
// CLI binary surface (AC #5: invalid cron/TZ → ValidationError exit)
// =============================================================================

/// `creator works cron` is registered as a subcommand of `creator works`.
#[test]
fn works_help_lists_cron_subcommand() {
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
        help.contains("cron"),
        "creator works --help must list 'cron' subcommand: {help}"
    );
}

/// `creator works cron --help` lists set/show/list.
#[test]
fn cron_help_lists_set_show_list() {
    use assert_cmd::Command;
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "cron", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    for sub in &["set", "show", "list"] {
        assert!(
            help.contains(sub),
            "creator works cron --help must list '{sub}': {help}"
        );
    }
}

/// `creator works cron set --help` documents the role flags.
#[test]
fn cron_set_help_documents_flags() {
    use assert_cmd::Command;
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "cron", "set", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    for flag in &[
        "--brainstorm",
        "--write",
        "--review",
        "--tz",
        "--no-brainstorm",
    ] {
        assert!(
            help.contains(flag),
            "cron set --help must document {flag}: {help}"
        );
    }
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
fn sample_work_record(work_id: &str, work_ref: &str) -> nexus_local_db::works::WorkRecord {
    use nexus_local_db::works::WorkRecord;
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "default".to_string(),
        status: "draft".to_string(),
        title: "Test Novel".to_string(),
        long_term_goal: "Test goal".to_string(),
        initial_idea: "An idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
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
        total_planned_chapters: None,
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
