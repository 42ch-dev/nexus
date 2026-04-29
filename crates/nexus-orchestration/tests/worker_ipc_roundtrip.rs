//! Integration tests for Worker Manager + JSON-RPC IPC roundtrip.
//!
//! Tests:
//! - `worker_health_roundtrip`: spawn dummy worker, call health, shutdown.
//! - `worker_crash_emits_event`: spawn suicide worker, verify crash detection.

use nexus_orchestration::worker::{WorkerEvent, WorkerManager, WorkerSpec};
use std::time::Duration;

/// Spawn a dummy worker, send `worker/health`, verify response, shutdown.
#[tokio::test]
async fn worker_health_roundtrip() {
    let mgr = WorkerManager::new();
    let spec = WorkerSpec::test_stub("./tests/fixtures/dummy-worker.sh");
    let mut handle = mgr.spawn(&spec).await.expect("spawn");

    // Small delay to let the worker start and be ready to read.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let health = handle
        .call_json_rpc("worker/health", serde_json::json!({}))
        .await
        .expect("rpc call");

    assert_eq!(health["ok"], true, "health result should have ok=true");
    assert!(
        health.get("uptime_ms").is_some(),
        "health result should have uptime_ms"
    );
    assert!(
        health.get("acp_session_state").is_some(),
        "health result should have acp_session_state"
    );

    handle.shutdown().await.unwrap();
}

/// Spawn a worker that exits immediately with code 1, verify crash event.
#[tokio::test]
async fn worker_crash_emits_event() {
    let mgr = WorkerManager::new();
    let spec = WorkerSpec::test_stub("./tests/fixtures/suicide-worker.sh");
    let _handle = mgr.spawn(&spec).await.expect("spawn");

    let mut events = mgr.subscribe();

    let evt = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("timeout waiting for crash event")
        .expect("event channel not closed");

    // Skip any "Started" event — look for Crashed.
    match evt {
        WorkerEvent::Started { .. } => {
            // Wait for the next event (should be Crashed).
            let evt2 = tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("timeout waiting for crash event")
                .expect("event channel not closed");
            match evt2 {
                WorkerEvent::Crashed {
                    pid: _,
                    exit_status,
                } => {
                    assert_ne!(exit_status, Some(0));
                }
                other => panic!("expected Crashed, got {other:?}"),
            }
        }
        WorkerEvent::Crashed {
            pid: _,
            exit_status,
        } => {
            assert_ne!(exit_status, Some(0));
        }
        other => panic!("expected Crashed, got {other:?}"),
    }
}
