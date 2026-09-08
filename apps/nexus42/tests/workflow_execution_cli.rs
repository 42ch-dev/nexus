//! v1.186 P2 Task 1 — public admission through the production daemon.
//!
//! Fixture-only: hermetic LiveDaemon + real `nexus42` binary. No live omp.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::LiveDaemon;
use serde_json::Value;

fn stdout_json(output: &std::process::Output) -> Value {
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(text.trim()).unwrap_or_else(|_| {
        panic!(
            "expected JSON stdout, got status={} stderr={} stdout={}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            text
        )
    })
}

fn stderr_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[tokio::test]
async fn admission_new_public_schedule_is_driven() {
    let daemon = LiveDaemon::start().await;
    let add = daemon
        .cli(&[
            "daemon",
            "schedule",
            "add",
            "--preset",
            "essay.scaffold",
            "--creator",
            "test_creator",
            "--label",
            "p2-t1-admission",
        ])
        .await;
    assert!(
        add.status.success(),
        "schedule add failed: {}",
        stderr_text(&add)
    );
    let body = stdout_json(&add);
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    let status = body["status"].as_str().expect("status");
    assert_ne!(
        status, "pending",
        "new public add must not remain a dead pending row: {body}"
    );

    let inspect = daemon
        .cli(&["daemon", "schedule", "inspect", schedule_id])
        .await;
    assert!(
        inspect.status.success(),
        "inspect failed: {}",
        stderr_text(&inspect)
    );
    let inspected = stdout_json(&inspect);
    assert!(
        inspected
            .get("current_session_id")
            .and_then(Value::as_str)
            .is_some()
            || inspected["status"] != "pending",
        "driven schedule must own a session or leave pending: {inspected}"
    );
}

#[tokio::test]
async fn admission_system_maintenance_stays_inert() {
    let daemon = LiveDaemon::start().await;
    let add = daemon
        .cli(&[
            "daemon",
            "schedule",
            "add",
            "--preset",
            "_system.maintenance",
            "--creator",
            "test_creator",
            "--label",
            "p2-t1-system",
        ])
        .await;
    if add.status.success() {
        let body = stdout_json(&add);
        assert_ne!(
            body["status"], "running",
            "_system.maintenance must not start a driven run: {body}"
        );
        if let Some(schedule_id) = body["schedule_id"].as_str() {
            let inspect = daemon
                .cli(&["daemon", "schedule", "inspect", schedule_id])
                .await;
            assert!(
                inspect.status.success(),
                "inspect failed: {}",
                stderr_text(&inspect)
            );
            let inspected = stdout_json(&inspect);
            let policy = inspected
                .get("execution_policy")
                .and_then(Value::as_str)
                .unwrap_or("");
            assert!(
                policy == "system_inert" || inspected["status"] != "running",
                "system preset must stay inert: {inspected}"
            );
        }
        return;
    }
    let err = format!("{}{}", stderr_text(&add), String::from_utf8_lossy(&add.stdout));
    let lower = err.to_lowercase();
    assert!(
        lower.contains("system")
            || lower.contains("not eligible")
            || lower.contains("inert")
            || lower.contains("cannot"),
        "system-preset refusal must be explicit, got: {err}"
    );
}

#[tokio::test]
async fn concurrent_start_yields_one_owned_session() {
    let daemon = LiveDaemon::start().await;
    let add = daemon
        .cli(&[
            "daemon",
            "schedule",
            "add",
            "--preset",
            "essay.scaffold",
            "--creator",
            "test_creator",
            "--label",
            "p2-t1-concurrent",
        ])
        .await;
    assert!(add.status.success(), "add failed: {}", stderr_text(&add));
    let schedule_id = stdout_json(&add)["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    let start_args_a = [
        "daemon",
        "schedule",
        "start",
        schedule_id.as_str(),
    ];
    let start_args_b = start_args_a;
    let (a, b) = tokio::join!(daemon.cli(&start_args_a), daemon.cli(&start_args_b));
    assert!(
        a.status.success() || b.status.success(),
        "at least one start must succeed: {} / {}",
        stderr_text(&a),
        stderr_text(&b)
    );

    let inspect = daemon
        .cli(&["daemon", "schedule", "inspect", &schedule_id])
        .await;
    assert!(inspect.status.success(), "{}", stderr_text(&inspect));
    let inspected = stdout_json(&inspect);
    let sid = inspected
        .get("current_session_id")
        .and_then(Value::as_str)
        .expect("exactly one owned session");
    assert!(!sid.is_empty(), "owned session must be non-empty: {inspected}");
}

#[tokio::test]
async fn explicit_legacy_running_without_session_starts() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy)
           VALUES ('SCHLEGACYRUN', 'test_creator', 'essay.scaffold', 1, 'running',
                   'serial', 0, 'p2-t1-legacy-running', ?, ?, NULL, 'legacy_inert')",
    )
    .bind(now)
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed legacy running-without-session row");

    let start = daemon
        .cli(&["daemon", "schedule", "start", "SCHLEGACYRUN"])
        .await;
    assert!(
        start.status.success(),
        "legacy start failed: {}",
        stderr_text(&start)
    );
    let inspect = daemon
        .cli(&["daemon", "schedule", "inspect", "SCHLEGACYRUN"])
        .await;
    assert!(inspect.status.success(), "{}", stderr_text(&inspect));
    let inspected = stdout_json(&inspect);
    assert!(
        inspected
            .get("current_session_id")
            .and_then(Value::as_str)
            .is_some(),
        "legacy start must mint an owned session: {inspected}"
    );
}

#[tokio::test]
async fn future_scheduled_add_stays_pending() {
    let daemon = LiveDaemon::start().await;
    let future = chrono::Utc::now() + chrono::Duration::hours(2);
    let add = daemon
        .cli(&[
            "daemon",
            "schedule",
            "add",
            "--preset",
            "essay.scaffold",
            "--creator",
            "test_creator",
            "--label",
            "p2-t1-gated-clock",
            "--scheduled-at",
            &future.to_rfc3339(),
        ])
        .await;
    assert!(add.status.success(), "add failed: {}", stderr_text(&add));
    let body = stdout_json(&add);
    assert_eq!(
        body["status"].as_str(),
        Some("pending"),
        "future scheduled_at must defer admission: {body}"
    );
}
