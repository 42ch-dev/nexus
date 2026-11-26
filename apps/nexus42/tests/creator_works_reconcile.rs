//! Direct-core `creator works reconcile-chapters --json` wire test (I-000625).
//!
//! A chapter whose frontmatter status write-back cannot be rendered used to be
//! reported only on stderr; the reconcile report now carries the detail as
//! `skipped: [{path, reason}]`, and omits the key entirely when nothing was
//! skipped so a clean run's JSON stays byte-compatible with the pre-I-000625
//! payload.

#[path = "common/direct.rs"]
mod direct;
#[path = "common/direct_actor.rs"]
mod direct_actor;

use direct::DirectFixture;
use direct_actor::fixture_creator_id;
use nexus_contracts::{CreateWorkRequest, CreateWorldRequest};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_home_layout::{operational_workspace_dir, workspace_state_db_path};
use nexus_local_db::work_chapters;
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use serde_json::json;
use std::path::Path;
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";
/// Work whose chapter frontmatter cannot be rendered.
const BROKEN_REF: &str = "reconcile-skip-novel";
/// Work whose chapter file already agrees with the DB row.
const CLEAN_REF: &str = "reconcile-clean-novel";
/// The unrenderable chapter: `status` differs from the DB row, and the
/// frontmatter block never closes, so no bytes can be rendered to resync it.
const BROKEN_CHAPTER: &str = "---\ntitle: Broken\nchapter: 1\nstatus: draft\nUnclosed body";
/// The chapter that matches its DB row (`not_started`).
const CLEAN_CHAPTER: &str = "---\ntitle: Ok\nchapter: 1\nstatus: not_started\n---\nBody";
const NOW: &str = "2026-01-01T00:00:00Z";

fn describe(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Register `creative_root` as the workspace's `local_root`, the root every
/// Work path resolves against.
fn register_creative_root(op_dir: &Path, creator_id: &str, creative_root: &Path) {
    std::fs::create_dir_all(op_dir).expect("materialize operational workspace dir");
    std::fs::write(
        op_dir.join("meta.json"),
        serde_json::to_string(&json!({
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

/// Seed one novel Work carrying `work_ref`, then one `work_chapters` row and
/// the matching `Stories/ch01-*.md` file.
async fn seed_novel_work(
    fixture: &DirectFixture,
    creator_id: &str,
    creative_root: &Path,
    work_ref: &str,
    chapter: &str,
    file_name: &str,
) -> String {
    let core = CoreService::open(CoreOpenOptions {
        user_home: fixture.home.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
        memory: None,
    })
    .await
    .expect("seed core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    let world_id = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(json!({
                "title": format!("{work_ref} world")
            }))
            .expect("world request shape"),
        )
        .await
        .expect("create world")
        .world_id;
    let work_id = core
        .create_work(
            &principal,
            CreateWorkRequest {
                client_request_id: None,
                initial_idea: "A test story".to_string(),
                lineage_from_work_id: None,
                long_term_goal: "Prove the reconcile wire shape".to_string(),
                primary_preset_id: None,
                set_pool_active: None,
                story_ref: Some(work_ref.to_string()),
                title: format!("{work_ref} title"),
                work_profile: Some("novel".to_string()),
                world_id,
            },
        )
        .await
        .expect("seed work")
        .work_id;
    core.close().await.expect("seed core closes");

    let db_path = workspace_state_db_path(fixture.home.path(), creator_id, WORKSPACE_SLUG);
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("workspace pool")
        .clone_pool();
    work_chapters::seed_chapters(&pool, &work_id, work_ref, 1, NOW)
        .await
        .expect("seed chapter row");
    pool.close().await;
    release_retained_writer_guards(&db_path);

    let stories_dir = creative_root.join("Works").join(work_ref).join("Stories");
    std::fs::create_dir_all(&stories_dir).expect("materialize Stories dir");
    std::fs::write(stories_dir.join(file_name), chapter).expect("write chapter file");

    work_id
}

fn reconcile_json(fixture: &DirectFixture, work_id: &str) -> serde_json::Value {
    let out = fixture
        .command()
        .args([
            "creator",
            "works",
            "reconcile-chapters",
            work_id,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("spawn nexus42");
    assert!(
        out.status.success(),
        "reconcile must succeed: {}",
        describe(&out)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|err| {
        panic!(
            "expected a JSON report on stdout ({err}): {}",
            describe(&out)
        )
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_json_carries_skipped_and_omits_it_when_empty() {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(fixture.home.path());
    let creative_root = fixture.home.path().join("creative");
    register_creative_root(
        &operational_workspace_dir(fixture.home.path(), &creator_id, WORKSPACE_SLUG),
        &creator_id,
        &creative_root,
    );

    let broken_id = seed_novel_work(
        &fixture,
        &creator_id,
        &creative_root,
        BROKEN_REF,
        BROKEN_CHAPTER,
        "ch01-broken.md",
    )
    .await;
    let clean_id = seed_novel_work(
        &fixture,
        &creator_id,
        &creative_root,
        CLEAN_REF,
        CLEAN_CHAPTER,
        "ch01-clean.md",
    )
    .await;

    let broken = reconcile_json(&fixture, &broken_id);
    let skipped = broken
        .get("skipped")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("the unrenderable chapter must reach the wire: {broken}"));
    assert_eq!(skipped.len(), 1, "one chapter cannot be rendered: {broken}");
    let expected_path = format!("Works/{BROKEN_REF}/Stories/ch01-broken.md");
    assert_eq!(
        skipped[0].get("path").and_then(serde_json::Value::as_str),
        Some(expected_path.as_str()),
        "the skip must name the chapter path: {broken}"
    );
    assert!(
        skipped[0]
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|reason| reason.contains("cannot be rendered")),
        "the skip must carry the reason: {broken}"
    );

    let clean = reconcile_json(&fixture, &clean_id);
    assert!(
        clean.get("skipped").is_none(),
        "a run with nothing skipped must omit the key: {clean}"
    );
    assert_eq!(
        clean,
        json!({"created": 0, "updated": 0, "resynced": 0, "preserved": 1}),
        "the four counters and the key set must stay byte-compatible: {clean}"
    );
}
