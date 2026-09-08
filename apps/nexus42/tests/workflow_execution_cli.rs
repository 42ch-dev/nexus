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
        String::from_utf8_lossy(&add.stderr)
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
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspected = stdout_json(&inspect);
    assert!(
        inspected.get("current_session_id").and_then(Value::as_str).is_some()
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
    if !add.status.success() {
        // Fail-closed refusal is an acceptable system-inert outcome.
        return;
    }
    let body = stdout_json(&add);
    assert_ne!(
        body["status"], "running",
        "_system.maintenance must not start a driven run: {body}"
    );
}
