//! Hermetic CLI integration tests — `creator works findings list|set-status`
//! and `creator world findings list` (V1.175 P1 Task 4, group 8):
//! work-findings PATCH triage over the existing V1.39 route, end-to-end
//! against a live daemon fixture with hermetic `HOME` (AR-83 #6 / AR-87).
//!
//! Each test seeds one owned Work + a finding row, then drives the REAL
//! `nexus42` binary. Failure paths: the invalid-transition path (illegal
//! status move → 422 `invalid_transition` naming `from → to`) and the
//! terminal-state-rejection path (resolved → anything → 422).
//!
mod common;

use axum::extract::State;
use axum::Json;
use common::LiveDaemon;
use nexus_daemon_runtime::api::handlers::works::{create_work, CreateWorkRequest};
use nexus_local_db::findings::{self, Finding};
use std::process::Output;

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Seed one owned Work (bound to the fixture's `wld_test_world`) and return
/// the `work_id`.
async fn seed_work(d: &LiveDaemon) -> String {
    let (_, Json(resp)) = create_work(
        State(d.state.clone()),
        Json(CreateWorkRequest {
            title: "Findings Test Novel".to_string(),
            long_term_goal: "Test findings triage".to_string(),
            initial_idea: "A test story".to_string(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: Some("findings-test-novel".to_string()),
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
            work_profile: Some("novel".to_string()),
        }),
    )
    .await
    .expect("seed work via daemon handler");
    resp.work_id
}

/// Seed one `open` finding for the work and return the `finding_id`.
async fn seed_finding(d: &LiveDaemon, work_id: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    let finding_id = findings::mint_finding_id();
    let f = Finding {
        finding_id: finding_id.clone(),
        work_id: work_id.to_string(),
        chapter: None,
        severity: "major".to_string(),
        status: "open".to_string(),
        title: "Test finding".to_string(),
        description: "A test finding".to_string(),
        target_executor: "write".to_string(),
        creator_id: "test_creator".to_string(),
        kind: "craft".to_string(),
        rule_suggestion: None,
        created_at: now,
        updated_at: now,
    };
    findings::create_finding(&d.pool, &f)
        .await
        .expect("seed finding");
    finding_id
}

// ── works findings list ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn findings_list_shows_seeded_finding() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;
    let finding_id = seed_finding(&d, &work_id).await;

    let out = d
        .cli(&["creator", "works", "findings", "list", &work_id])
        .await;
    assert!(out.status.success(), "list failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(&finding_id), "{text}");
    assert!(text.contains("open"), "{text}");
    assert!(text.contains("major"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn findings_list_json_emits_dto_verbatim() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;
    let finding_id = seed_finding(&d, &work_id).await;

    let out = d
        .cli(&["creator", "works", "findings", "list", &work_id, "--json"])
        .await;
    assert!(out.status.success(), "list failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let items = parsed["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["finding_id"], finding_id);
    assert_eq!(items[0]["status"], "open");
}

// ── works findings set-status ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_status_open_to_triaged_succeeds() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;
    let finding_id = seed_finding(&d, &work_id).await;

    let out = d
        .cli(&[
            "creator",
            "works",
            "findings",
            "set-status",
            &finding_id,
            "--work",
            &work_id,
            "--status",
            "triaged",
        ])
        .await;
    assert!(out.status.success(), "set-status failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("triaged"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_status_invalid_transition_surfaces_422() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;
    let finding_id = seed_finding(&d, &work_id).await;

    // open → resolved is legal; resolved → open is NOT (terminal state).
    let first = d
        .cli(&[
            "creator",
            "works",
            "findings",
            "set-status",
            &finding_id,
            "--work",
            &work_id,
            "--status",
            "resolved",
        ])
        .await;
    assert!(
        first.status.success(),
        "first move failed: {}",
        stderr(&first)
    );

    let out = d
        .cli(&[
            "creator",
            "works",
            "findings",
            "set-status",
            &finding_id,
            "--work",
            &work_id,
            "--status",
            "open",
        ])
        .await;
    assert!(!out.status.success(), "terminal-state move should fail");
    let err = stderr(&out);
    assert!(err.contains("invalid_transition"), "code missing: {err}");
    assert!(err.contains("resolved"), "from missing: {err}");
    assert!(err.contains("open"), "to missing: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_status_self_loop_rejected() {
    let d = LiveDaemon::start().await;
    let work_id = seed_work(&d).await;
    let finding_id = seed_finding(&d, &work_id).await;

    // open → open is `from == to` — rejected as invalid_transition.
    let out = d
        .cli(&[
            "creator",
            "works",
            "findings",
            "set-status",
            &finding_id,
            "--work",
            &work_id,
            "--status",
            "open",
        ])
        .await;
    assert!(!out.status.success(), "self-loop should fail");
    let err = stderr(&out);
    assert!(err.contains("invalid_transition"), "code missing: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_status_help_documents_transition_table() {
    let d = LiveDaemon::start().await;
    let out = d
        .cli(&["creator", "works", "findings", "set-status", "--help"])
        .await;
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
    let d = LiveDaemon::start().await;
    let out = d
        .cli(&[
            "creator",
            "world",
            "findings",
            "--world-id",
            "wld_test_world",
        ])
        .await;
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
    let d = LiveDaemon::start().await;
    let out = d
        .cli(&[
            "creator",
            "world",
            "findings",
            "--world-id",
            "wld_test_world",
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "world findings failed: {}",
        stderr(&out)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert!(parsed["findings"].is_array());
    assert_eq!(parsed["truncated"], false);
}
