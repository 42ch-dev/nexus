//! Direct-core CLI tests — `creator works findings list|set-status` and
//! `creator world findings list` (V1.175 P1 Task 4, group 8; direct-core
//! retarget v1.193 P0-T7).
//!
//! The leaves run on the typed core seam (`CoreService::list_findings` /
//! `get_work` / `update_finding` and the world findings read) against a
//! hermetic direct-core home — no daemon, no Node child (`common/direct.rs`
//! precedent).
//!
//! Each test seeds one owned World + Work (plus its findings), releases every
//! seed writer, and then drives the REAL `nexus42` binary. Failure paths: the
//! invalid-transition path (an illegal status move naming `from → to`) and the
//! terminal-state-rejection path (resolved → anything).

#[path = "common/direct.rs"]
mod direct;

use assert_cmd::Command;
use direct::DirectFixture;
use nexus_contracts::{CreateWorkRequest, CreateWorldRequest};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_home_layout::{nexus_root_from_home, operational_workspace_dir, workspace_state_db_path};
use nexus_local_db::findings::{self, Finding};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";
/// World title the seeded World is created under.
const WORLD_TITLE: &str = "Findings Test World";
/// Deterministic `story_ref` for the seeded Work.
const STORY_REF: &str = "findings-test-novel";

/// A hermetic direct-core home with one owned World + Work and its findings.
struct FindingsEnv {
    fixture: DirectFixture,
    world_id: String,
    work_id: String,
    /// Seeded finding ids, in seed order.
    finding_ids: Vec<String>,
}

impl FindingsEnv {
    /// Run the real `nexus42` binary against this fixture's hermetic `HOME`.
    fn cli(&self, args: &[&str]) -> Output {
        self.fixture
            .command()
            .args(args)
            .output()
            .expect("spawn nexus42")
    }

    /// The first seeded finding id.
    fn first_finding(&self) -> &str {
        self.finding_ids
            .first()
            .expect("fixture seeds at least one finding")
    }
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under `~/.nexus42/creators/`.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
    let mut entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .pop()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}

/// Seed one owned World + novel Work and `finding_count` `open` findings on it,
/// then release every seed writer so the CLI child can admit its own.
async fn fresh_env(finding_count: usize) -> FindingsEnv {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(&fixture);
    let db_path = workspace_state_db_path(fixture.home.path(), &creator_id, WORKSPACE_SLUG);
    let creative_root = fixture.home.path().join("creative");

    // The core resolves workspace files against the operational `meta.json`
    // `local_root` — the same key the CLI's own workspace registration writes.
    std::fs::create_dir_all(&creative_root).expect("materialize creative root");
    std::fs::write(
        operational_workspace_dir(fixture.home.path(), &creator_id, WORKSPACE_SLUG)
            .join("meta.json"),
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "creator_id": creator_id,
            "workspace_slug": WORKSPACE_SLUG,
            "local_root": creative_root,
            "created_at": "2020-01-01T00:00:00Z"
        }))
        .expect("meta json"),
    )
    .expect("write meta.json");

    // A Work binds to an owned World, so the World comes first. Both go through
    // the core seam, which owns the ids, the ownership rows and the Work
    // directory.
    let core = CoreService::open(CoreOpenOptions {
        user_home: fixture.home.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("seed core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    let world_id = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({
                "title": WORLD_TITLE
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
                long_term_goal: "Test findings triage".to_string(),
                primary_preset_id: None,
                set_pool_active: None,
                story_ref: Some(STORY_REF.to_string()),
                title: "Findings Test Novel".to_string(),
                work_profile: Some("novel".to_string()),
                world_id: Some(world_id.clone()),
            },
        )
        .await
        .expect("seed work")
        .work_id;
    core.close().await.expect("seed core closes");

    // The finding rows go in after the Work row (they carry the Work key) and
    // are creator-scoped, exactly as the findings reads require.
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("workspace pool")
        .clone_pool();
    let now = chrono::Utc::now().timestamp();
    let mut finding_ids = Vec::with_capacity(finding_count);
    for index in 0..finding_count {
        let finding_id = findings::mint_finding_id();
        findings::create_finding(
            &pool,
            &Finding {
                finding_id: finding_id.clone(),
                work_id: work_id.clone(),
                chapter: None,
                severity: "major".to_string(),
                status: "open".to_string(),
                title: format!("Test finding {index}"),
                description: "A test finding".to_string(),
                target_executor: "write".to_string(),
                creator_id: creator_id.clone(),
                kind: "craft".to_string(),
                rule_suggestion: None,
                created_at: now,
                updated_at: now,
            },
        )
        .await
        .expect("seed finding");
        finding_ids.push(finding_id);
    }
    pool.close().await;
    // A CLI child must not be handed a home another writer still holds.
    release_retained_writer_guards(&db_path);

    FindingsEnv {
        fixture,
        world_id,
        work_id,
        finding_ids,
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ── works status — novel-only findings enrichment ─────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_json_enriches_novel_work_with_open_findings() {
    let env = fresh_env(1).await;

    let out = env.cli(&["creator", "works", "status", &env.work_id, "--json"]);
    assert!(out.status.success(), "status failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(parsed["work_profile"], "novel");
    let findings = parsed["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["finding_id"], env.first_finding());
    assert_eq!(findings[0]["status"], "open");
    // One open finding is far below the fetch cap, so no truncation marker.
    assert!(parsed.get("findings_truncated").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_human_renders_open_findings_summary() {
    let env = fresh_env(1).await;

    let out = env.cli(&["creator", "works", "status", &env.work_id]);
    assert!(out.status.success(), "status failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("findings: 1 open"), "{text}");
}

// ── works findings list ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn findings_list_shows_seeded_finding() {
    let env = fresh_env(1).await;

    let out = env.cli(&["creator", "works", "findings", "list", &env.work_id]);
    assert!(out.status.success(), "list failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(env.first_finding()), "{text}");
    assert!(text.contains("open"), "{text}");
    assert!(text.contains("major"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn findings_list_json_emits_dto_verbatim() {
    let env = fresh_env(1).await;

    let out = env.cli(&[
        "creator",
        "works",
        "findings",
        "list",
        &env.work_id,
        "--json",
    ]);
    assert!(out.status.success(), "list failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let items = parsed["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["finding_id"], env.first_finding());
    assert_eq!(items[0]["status"], "open");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn findings_list_notes_has_more_when_paginated() {
    // qc3 W-002: the human default must never look complete when it is not.
    // The core's default page is 100 findings; with 101 rows the DTO's
    // `pagination.has_more` is true and the human output must say so.
    let env = fresh_env(101).await;

    let out = env.cli(&["creator", "works", "findings", "list", &env.work_id]);
    assert!(out.status.success(), "list failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("truncated"), "has_more note missing: {text}");
    assert!(text.contains("use --json"), "{text}");
}

// ── works findings set-status ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_status_open_to_triaged_succeeds() {
    let env = fresh_env(1).await;

    let out = env.cli(&[
        "creator",
        "works",
        "findings",
        "set-status",
        env.first_finding(),
        "--work",
        &env.work_id,
        "--status",
        "triaged",
    ]);
    assert!(out.status.success(), "set-status failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("triaged"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_status_invalid_transition_surfaces_422() {
    let env = fresh_env(1).await;

    // open → resolved is legal; resolved → open is NOT (terminal state).
    let first = env.cli(&[
        "creator",
        "works",
        "findings",
        "set-status",
        env.first_finding(),
        "--work",
        &env.work_id,
        "--status",
        "resolved",
    ]);
    assert!(
        first.status.success(),
        "first move failed: {}",
        stderr(&first)
    );

    let out = env.cli(&[
        "creator",
        "works",
        "findings",
        "set-status",
        env.first_finding(),
        "--work",
        &env.work_id,
        "--status",
        "open",
    ]);
    assert!(!out.status.success(), "terminal-state move should fail");
    let err = stderr(&out);
    assert!(err.contains("invalid_transition"), "code missing: {err}");
    assert!(
        err.contains("'resolved' → 'open'"),
        "from → to arrow unit missing: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_status_self_loop_rejected() {
    let env = fresh_env(1).await;

    // open → open is `from == to` — rejected as invalid_transition.
    let out = env.cli(&[
        "creator",
        "works",
        "findings",
        "set-status",
        env.first_finding(),
        "--work",
        &env.work_id,
        "--status",
        "open",
    ]);
    assert!(!out.status.success(), "self-loop should fail");
    let err = stderr(&out);
    assert!(err.contains("invalid_transition"), "code missing: {err}");
    assert!(
        err.contains("'open' → 'open'"),
        "self-loop from → to unit missing: {err}"
    );
}

#[test]
fn set_status_help_documents_transition_table() {
    // The closed transition table is parser documentation; no home is needed.
    let out = Command::cargo_bin("nexus42")
        .expect("nexus42 binary")
        .args(["creator", "works", "findings", "set-status", "--help"])
        .output()
        .expect("spawn nexus42");
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("triaged"), "{text}");
    assert!(text.contains("in_review"), "{text}");
    assert!(text.contains("wont_fix"), "{text}");
    assert!(text.contains("duplicate"), "{text}");
    assert!(text.contains("terminal"), "{text}");
}

// ── world findings list (GET-only) ────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn world_findings_list_empty_world() {
    let env = fresh_env(1).await;

    let out = env.cli(&[
        "creator",
        "world",
        "findings",
        "list",
        "--world-id",
        &env.world_id,
    ]);
    assert!(
        out.status.success(),
        "world findings failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("No world findings"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn world_findings_list_json_emits_dto_verbatim() {
    let env = fresh_env(1).await;

    let out = env.cli(&[
        "creator",
        "world",
        "findings",
        "list",
        "--world-id",
        &env.world_id,
        "--json",
    ]);
    assert!(
        out.status.success(),
        "world findings failed: {}",
        stderr(&out)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert!(parsed["findings"].is_array());
    assert_eq!(parsed["truncated"], false);
}
