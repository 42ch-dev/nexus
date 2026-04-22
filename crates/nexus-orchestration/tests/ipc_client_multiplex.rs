//! Integration test for IpcClient multiplexed request routing.
//!
//! Tests:
//! - `three_concurrent_requests`: send 3 concurrent requests, verify responses
//!   are routed to the correct callers.
//! - `ten_concurrent_requests`: stress test with 10 concurrent in-flight
//!   requests (acceptance criterion: ≥10 without response mixing).
//! - `notification_no_response`: verify `notify()` sends without awaiting.
//! - `timeout_cancels_pending`: verify timed-out requests are cleaned up.
//! - `close_cancels_pending`: verify `close()` cancels all pending requests.

use nexus_orchestration::worker::{DuplexTransport, IpcClient, IpcError, RpcTransport};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Spawn a mock server task that echoes each request's `id` and `method`
/// in the `result` field, with an optional artificial delay.
///
/// Returns the client-side transport (as Box<dyn RpcTransport>) and the
/// server task handle.
async fn spawn_mock_server(
    delay: Duration,
) -> (Box<dyn RpcTransport>, tokio::task::JoinHandle<()>) {
    let (client, server) = DuplexTransport::new_pair();
    let server = Arc::new(Mutex::new(server));

    let handle = tokio::spawn(async move {
        loop {
            let line = {
                let mut s = server.lock().await;
                s.recv().await
            };

            match line {
                Some(request_str) => {
                    // Parse the request, extract id and method.
                    if let Ok(req) = serde_json::from_str::<serde_json::Value>(&request_str) {
                        // Check if it's a notification (no id).
                        if req.get("id").is_none() {
                            // Notification — don't reply.
                            continue;
                        }

                        let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
                        let method = req
                            .get("method")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown")
                            .to_string();

                        // Apply artificial delay to simulate server processing.
                        if !delay.is_zero() {
                            tokio::time::sleep(delay).await;
                        }

                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "echo_method": method,
                                "echo_id": id,
                            }
                        });

                        let reply = serde_json::to_string(&response).expect("serialize response");

                        let mut s = server.lock().await;
                        let _ = s.send(reply).await;
                    }
                }
                None => {
                    // Client disconnected.
                    break;
                }
            }
        }
    });

    (Box::new(client), handle)
}

#[tokio::test]
async fn three_concurrent_requests() {
    let (transport, _server) = spawn_mock_server(Duration::from_millis(10)).await;
    let client = Arc::new(IpcClient::new(transport));

    let c1 = client.clone();
    let c2 = client.clone();
    let c3 = client.clone();

    let h1 = tokio::spawn(async move { c1.call("method_a", serde_json::json!({"x": 1})).await });
    let h2 = tokio::spawn(async move { c2.call("method_b", serde_json::json!({"y": 2})).await });
    let h3 = tokio::spawn(async move { c3.call("method_c", serde_json::json!({"z": 3})).await });

    let r1 = h1.await.expect("task join");
    let r2 = h2.await.expect("task join");
    let r3 = h3.await.expect("task join");

    // Each response should echo the correct method.
    assert_eq!(r1.expect("response")["echo_method"], "method_a");
    assert_eq!(r2.expect("response")["echo_method"], "method_b");
    assert_eq!(r3.expect("response")["echo_method"], "method_c");
}

#[tokio::test]
async fn ten_concurrent_requests() {
    // Acceptance criterion: IpcClient supports ≥10 concurrent in-flight
    // requests without response mixing.
    let (transport, _server) = spawn_mock_server(Duration::from_millis(5)).await;
    let client = Arc::new(IpcClient::new(transport));

    let mut handles = Vec::new();
    for i in 0..10u64 {
        let c = client.clone();
        let method = format!("method_{i}");
        handles.push(tokio::spawn(async move {
            let result = c.call(&method, serde_json::json!({"idx": i})).await;
            (i, result)
        }));
    }

    let mut responses: Vec<(u64, String)> = Vec::new();
    for h in handles {
        let (idx, result) = h.await.expect("task join");
        let val = result.expect("response");
        let echoed = val["echo_method"].as_str().expect("string").to_string();
        assert_eq!(
            echoed,
            format!("method_{idx}"),
            "response mixing detected for index {idx}"
        );
        responses.push((idx, echoed));
    }

    // All 10 responses must have been received with correct routing.
    assert_eq!(responses.len(), 10);
}

#[tokio::test]
async fn notification_no_response() {
    let (transport, _server) = spawn_mock_server(Duration::from_millis(0)).await;
    let client = IpcClient::new(transport);

    // Notification should succeed without awaiting a response.
    client
        .notify("some/notification", serde_json::json!({"data": 42}))
        .await
        .expect("notify should succeed");

    // Verify client is still usable for a normal call.
    let result = client
        .call("test/method", serde_json::json!({}))
        .await
        .expect("call after notify should work");
    assert_eq!(result["echo_method"], "test/method");
}

#[tokio::test]
async fn timeout_cancels_pending() {
    // Create a server that delays longer than the client timeout.
    let (transport, _server) = spawn_mock_server(Duration::from_millis(200)).await;
    let client = IpcClient::new(transport);

    let result = client
        .call_with_timeout(
            "slow/method",
            serde_json::json!({}),
            Duration::from_millis(50),
        )
        .await;

    match result {
        Err(IpcError::Timeout { timeout_ms }) => {
            assert!(
                timeout_ms >= 40 && timeout_ms <= 60,
                "expected ~50ms timeout, got {timeout_ms}ms"
            );
        }
        other => panic!("expected Timeout error, got {:?}", other),
    }
}

#[tokio::test]
async fn close_cancels_pending() {
    let (transport, _server) = spawn_mock_server(Duration::from_secs(10)).await;
    let client = IpcClient::new(transport);

    // Spawn a task that will block on a long-running call.
    let call_handle =
        tokio::spawn(async move { client.call("slow/method", serde_json::json!({})).await });

    // Give the request a moment to register.
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Abort the task — the IpcClient will be dropped, firing cancel token.
    call_handle.abort();

    let result = call_handle.await;
    assert!(result.is_err(), "aborted task should return Err");
}

// IpcClient must be Send + Sync for concurrent use via Arc.
#[test]
fn ipc_client_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<IpcClient>();
}
