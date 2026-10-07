//! Direct-core CLI tests — `creator kb queue-extract|extract-status`
//! (v1.207 P1 Task 4, R-V1190-KB-EXTRACT-QUEUE-GAP).
//!
//! The leaves run on the typed core seam (`CoreService::queue_kb_extract` /
//! `list_kb_extract_jobs` / `get_kb_extract_job`) against a hermetic
//! direct-core home — no daemon, no Node child (`common/direct.rs` precedent).
//! The rendered text is pinned line-for-line so the migration onto the core
//! producer cannot drop or reorder CLI output.
//!
//! `kb_extract_jobs` is guarded to an engine writer at the schema level, so the
//! enqueue leaf takes the workspace's engine-owned admission; the status leaf
//! stays on the direct-writer read path.

#![allow(clippy::too_many_lines)] // one end-to-end CLI scenario per test

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use std::process::Output;

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Run `creator kb queue-extract` with `args`.
fn queue_extract(fixture: &DirectFixture, args: &[&str]) -> Output {
    let mut command_args = vec!["creator", "kb", "queue-extract"];
    command_args.extend_from_slice(args);
    fixture
        .command()
        .args(&command_args)
        .output()
        .expect("run kb queue-extract")
}

/// Run `creator kb extract-status` with `args`.
fn extract_status(fixture: &DirectFixture, args: &[&str]) -> Output {
    let mut command_args = vec!["creator", "kb", "extract-status"];
    command_args.extend_from_slice(args);
    fixture
        .command()
        .args(&command_args)
        .output()
        .expect("run kb extract-status")
}

/// The job id carried by a `queue-extract` render.
fn queued_job_id(rendered: &str) -> String {
    rendered
        .lines()
        .find_map(|line| {
            line.strip_prefix("✓ Extract job queued: ")
                .or_else(|| line.strip_prefix("ℹ Extract job already exists: "))
        })
        .expect("queue render line")
        .to_string()
}

/// The one creator the fixture registered, read from the hermetic home.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    std::fs::read_dir(fixture.home.path().join(".nexus42/creators"))
        .expect("creators dir")
        .next()
        .expect("one creator")
        .expect("creator entry")
        .file_name()
        .into_string()
        .expect("utf8 creator id")
}

/// Queue → status round trip keeps the full human render, and the enqueue is
/// idempotent on its `(creator, work entry, world)` key.
#[tokio::test]
async fn queue_extract_and_status_round_trip() {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(&fixture);

    let queued = queue_extract(&fixture, &["kb_smoke", "--world-id", "wld_smoke"]);
    assert!(
        queued.status.success(),
        "queue-extract failed: {}",
        stderr(&queued)
    );
    let rendered = stdout(&queued);
    let job_id = queued_job_id(&rendered);
    assert!(job_id.starts_with("xj_"), "{rendered}");

    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "queue-extract line count changed: {rendered}"
    );
    assert_eq!(lines[0], format!("✓ Extract job queued: {job_id}"));
    assert_eq!(lines[1], "  Work entry:  kb_smoke");
    assert_eq!(lines[2], "  Target world: wld_smoke");
    assert_eq!(lines[3], "  Status:       queued");
    assert!(
        lines[4].starts_with("  Created:      ") && lines[4].len() > "  Created:      ".len(),
        "created line: {}",
        lines[4]
    );

    // Re-queue while the job is still `queued`: the store returns the same job
    // (no duplicate row), so the leaf renders the same "queued" line.
    let again = queue_extract(&fixture, &["kb_smoke", "--world-id", "wld_smoke"]);
    assert!(
        again.status.success(),
        "re-queue failed: {}",
        stderr(&again)
    );
    let re_rendered = stdout(&again);
    let re_lines: Vec<&str> = re_rendered.lines().collect();
    assert_eq!(
        re_lines[0],
        format!("✓ Extract job queued: {job_id}"),
        "{re_rendered}"
    );

    // The list arm renders the header (twice, unchanged) plus exactly one job
    // row — the idempotency key never duplicates a row.
    let listed = extract_status(&fixture, &[]);
    assert!(
        listed.status.success(),
        "extract-status failed: {}",
        stderr(&listed)
    );
    let listed_text = stdout(&listed);
    let listed_lines: Vec<&str> = listed_text.lines().collect();
    assert_eq!(listed_lines.len(), 3, "status list: {listed_text}");
    assert_eq!(
        listed_lines[0],
        format!("Extract jobs for creator {creator_id} (showing up to 100):")
    );
    assert_eq!(
        listed_lines[1],
        format!(
            "{:<20} {:<15} {:<20} {:<20} STATUS",
            "JOB_ID", "WORK_ENTRY", "WORLD", "CREATED"
        ),
        "row header"
    );
    let created_at = lines[4].trim_start_matches("  Created:      ");
    assert_eq!(
        listed_lines[2],
        format!(
            "{job_id:<20} {:<15} {:<20} {created_at:<20} queued",
            "kb_smoke", "wld_smoke"
        ),
        "job row"
    );

    // Detail arm: the same seven-line render the local implementation printed.
    let detail = extract_status(&fixture, &["--job-id", &job_id]);
    assert!(
        detail.status.success(),
        "extract-status --job-id failed: {}",
        stderr(&detail)
    );
    let detail_text = stdout(&detail);
    let detail_lines: Vec<&str> = detail_text.lines().collect();
    assert_eq!(detail_lines.len(), 7, "detail: {detail_text}");
    assert_eq!(detail_lines[0], format!("Job:           {job_id}"));
    assert_eq!(detail_lines[1], format!("  Creator:     {creator_id}"));
    assert_eq!(detail_lines[2], "  Workspace:   default");
    assert_eq!(detail_lines[3], "  Work entry:  kb_smoke");
    assert_eq!(detail_lines[4], "  World:       wld_smoke");
    assert_eq!(detail_lines[5], "  Status:      queued");
    assert_eq!(detail_lines[6], format!("  Created:     {created_at}"));

    // A non-`queued` existing job is what the "already exists" branch renders.
    // Move the row between runs on the fixture's own workspace DB (the CLI
    // child releases its admission when it exits).
    let db_path =
        nexus_home_layout::workspace_state_db_path(fixture.home.path(), &creator_id, "default");
    {
        let pool = nexus_local_db::init_engine_pool(&db_path)
            .await
            .unwrap()
            .clone_pool();
        nexus_local_db::mark_extract_job_running(&pool, &job_id)
            .await
            .unwrap();
        pool.close().await;
    }
    nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);

    let running = queue_extract(&fixture, &["kb_smoke", "--world-id", "wld_smoke"]);
    assert!(
        running.status.success(),
        "re-queue failed: {}",
        stderr(&running)
    );
    let running_text = stdout(&running);
    let running_lines: Vec<&str> = running_text.lines().collect();
    assert_eq!(
        running_lines[0],
        format!("ℹ Extract job already exists: {job_id}"),
        "{running_text}"
    );
    assert_eq!(running_lines[3], "  Status:       running");
}

/// `--chapter N` fills the artifact-locator lines through the core, and the
/// list arm shows the same job with its truncated columns.
#[tokio::test]
async fn chapter_sugar_renders_the_locator_lines() {
    let fixture = DirectFixture::new().await;

    let queued = queue_extract(
        &fixture,
        &[
            "kb_chapter",
            "--world-id",
            "wld_chapter",
            "--work-id",
            "wrk_novel",
            "--chapter",
            "5",
        ],
    );
    assert!(
        queued.status.success(),
        "queue-extract --chapter failed: {}",
        stderr(&queued)
    );
    let rendered = stdout(&queued);
    let job_id = queued_job_id(&rendered);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines.len(), 9, "chapter render: {rendered}");
    assert_eq!(lines[0], format!("✓ Extract job queued: {job_id}"));
    assert_eq!(lines[1], "  Work entry:  kb_chapter");
    assert_eq!(lines[2], "  Target world: wld_chapter");
    assert_eq!(lines[3], "  Source kind:  work_chapter");
    assert_eq!(lines[4], "  Source loc:   chapter:05");
    assert_eq!(lines[5], "  Profile:      novel");
    assert_eq!(lines[6], "  Work ID:      wrk_novel");
    assert_eq!(lines[7], "  Status:       queued");
    assert!(lines[8].starts_with("  Created:      "));
}

/// A chapter below 1 keeps its legacy refusal wording, and an unknown job id
/// keeps the legacy not-found wording — both without writing a job.
#[tokio::test]
async fn refused_inputs_keep_their_legacy_wording() {
    let fixture = DirectFixture::new().await;

    let zero = queue_extract(
        &fixture,
        &["kb_zero", "--world-id", "wld_zero", "--chapter", "0"],
    );
    assert!(!zero.status.success(), "chapter 0 must be refused");
    assert!(
        stderr(&zero).contains("Chapter number must be >= 1"),
        "chapter refusal wording: {}",
        stderr(&zero)
    );

    let missing = extract_status(&fixture, &["--job-id", "xj_missing"]);
    assert!(!missing.status.success(), "unknown job must be refused");
    assert!(
        stderr(&missing).contains("Extract job 'xj_missing' not found."),
        "not-found wording: {}",
        stderr(&missing)
    );

    // Neither refusal enqueued anything.
    let listed = extract_status(&fixture, &[]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    assert_eq!(
        stdout(&listed),
        format!(
            "No extract jobs for creator {}.\n",
            fixture_creator_id(&fixture)
        )
    );
}
