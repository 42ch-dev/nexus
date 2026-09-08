//! v1.186 P2 Task 1 — public admission through the production daemon.
//!
//! Fixture: hermetic `LiveDaemon` (real router) plus JSON HTTP against
//! `/v1/daemon/orchestration/schedules`. CLI stdout is human-formatted
//! and is not treated as JSON. No live omp.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::LiveDaemon;
use serde_json::{json, Value};

const PUBLIC_PRESET: &str = "memory-augmented";
const SYSTEM_PRESET: &str = "_system.maintenance";

#[derive(sqlx::FromRow)]
struct ScheduleDriveRow {
    status: String,
    execution_policy: String,
    current_session_id: Option<String>,
}

async fn post_json(daemon: &LiveDaemon, path: &str, body: Value) -> (reqwest::StatusCode, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}{path}", daemon.http_url))
        .json(&body)
        .send()
        .await
        .expect("POST schedule API");
    let status = resp.status();
    let text = resp.text().await.expect("response body");
    let json: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    (status, json)
}

async fn add_public(daemon: &LiveDaemon, label: &str) -> (reqwest::StatusCode, Value) {
    post_json(
        daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": label,
            "seed": "p2-t1-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-topic"
            }
        }),
    )
    .await
}

async fn load_drive_row(daemon: &LiveDaemon, schedule_id: &str) -> ScheduleDriveRow {
    sqlx::query_as::<_, ScheduleDriveRow>(
        "SELECT status, execution_policy, current_session_id
         FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_one(&daemon.pool)
    .await
    .expect("load schedule drive row")
}

#[tokio::test]
async fn admission_new_public_schedule_is_driven() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = add_public(&daemon, "p2-t1-admission").await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "public add must succeed: {body}"
    );
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    assert_ne!(
        body["status"].as_str(),
        Some("pending"),
        "new public add must not remain a dead pending row: {body}"
    );

    let row = load_drive_row(&daemon, schedule_id).await;
    assert_eq!(row.execution_policy, "driven_v1", "{schedule_id}");
    assert!(
        row.current_session_id.as_deref().is_some_and(|s| !s.is_empty()),
        "driven schedule must own a session: status={} session={:?}",
        row.status,
        row.current_session_id
    );
    assert_ne!(row.status, "pending");

    let cli = daemon
        .cli(&[
            "daemon",
            "schedule",
            "add",
            "--preset",
            PUBLIC_PRESET,
            "--creator",
            "test_creator",
            "--label",
            "p2-t1-admission-cli",
            "--seed",
            "p2-t1-topic",
        ])
        .await;
    assert!(
        cli.status.success(),
        "CLI add failed: {}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let stdout = String::from_utf8_lossy(&cli.stdout);
    assert!(
        stdout.contains("schedule_id:"),
        "CLI add is human-formatted, not JSON: {stdout}"
    );
    assert!(
        serde_json::from_str::<Value>(stdout.trim()).is_err(),
        "CLI add must not be mistaken for JSON stdout"
    );
}

#[tokio::test]
async fn admission_system_maintenance_stays_inert() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": SYSTEM_PRESET,
            "label": "p2-t1-system"
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "_system.maintenance must insert as an inert row, not a typed refusal or 5xx: {body}"
    );
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    assert_ne!(
        body["status"].as_str(),
        Some("running"),
        "_system.maintenance must not start a driven run: {body}"
    );

    let row = load_drive_row(&daemon, schedule_id).await;
    assert_eq!(row.execution_policy, "system_inert", "{schedule_id}");
    assert!(
        row.current_session_id.is_none(),
        "system preset must not own a session: {:?}",
        row.current_session_id
    );
    assert_ne!(row.status, "running");
}

#[tokio::test]
async fn concurrent_start_yields_one_owned_session() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = add_public(&daemon, "p2-t1-concurrent").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id").to_string();

    let path = format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal");
    let payload = json!({ "signal": "start" });
    let (a, b) = tokio::join!(
        post_json(&daemon, &path, payload.clone()),
        post_json(&daemon, &path, payload),
    );
    assert!(
        a.0.is_success() || b.0.is_success(),
        "at least one start must succeed: {} / {}",
        a.1,
        b.1
    );

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("exactly one owned session");
    assert!(!sid.is_empty(), "owned session must be non-empty");
    assert_eq!(row.execution_policy, "driven_v1");
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
           VALUES ('SCHLEGACYRUN', 'test_creator', ?, 1, 'running',
                   'serial', 0, 'p2-t1-legacy-running', ?, ?, NULL, 'legacy_inert')",
    )
    .bind(PUBLIC_PRESET)
    .bind(now)
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed legacy running-without-session row");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules/SCHLEGACYRUN/signal",
        json!({ "signal": "start" }),
    )
    .await;
    assert!(
        status.is_success(),
        "legacy start failed: status={status} body={body}"
    );

    let row = load_drive_row(&daemon, "SCHLEGACYRUN").await;
    assert_eq!(row.execution_policy, "driven_v1");
    assert!(
        row.current_session_id.as_deref().is_some_and(|s| !s.is_empty()),
        "legacy start must mint an owned session: status={} session={:?}",
        row.status,
        row.current_session_id
    );
}

#[tokio::test]
async fn future_scheduled_add_stays_pending() {
    let daemon = LiveDaemon::start().await;
    let future = chrono::Utc::now().timestamp() + 2 * 60 * 60;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-gated-clock",
            "scheduled_at": future.to_string(),
            "seed": "p2-t1-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-topic"
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    assert_eq!(
        body["status"].as_str(),
        Some("pending"),
        "future scheduled_at must defer admission: {body}"
    );
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    let row = load_drive_row(&daemon, schedule_id).await;
    assert_eq!(row.status, "pending");
    assert!(row.current_session_id.is_none());
    assert_eq!(row.execution_policy, "driven_v1");
}
