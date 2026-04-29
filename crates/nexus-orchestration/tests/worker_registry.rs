//! Integration tests for `WorkerRegistry` — multi-creator worker index.
//!
//! Tests:
//! - `registry_get_or_spawn_creates_worker`: spawning for a new creator inserts a handle.
//! - `registry_get_or_spawn_returns_existing`: second call for same creator returns same handle.
//! - `registry_get_returns_handle`: `get()` returns the correct handle by creator.
//! - `registry_get_returns_none_for_unknown`: `get()` returns `None` for unknown creator.
//! - `registry_remove_removes_entry`: `remove()` removes only the specified creator's entry.
//! - `registry_remove_returns_none_for_unknown`: `remove()` on unknown creator returns `None`.
//! - `registry_capacity_rejects_overflow`: `get_or_spawn` returns error when at max capacity.
//! - `registry_len_and_is_empty`: verify `len()` and `is_empty()` correctness.
//! - `registry_shutdown_all`: `shutdown_all()` sends shutdown to all workers.
//! - `registry_supports_sixteen_creators`: verifies ≥16 concurrent creator workers.

use std::sync::Arc;
use tokio::sync::Mutex;

use nexus_orchestration::worker::registry::WorkerSpawner;
use nexus_orchestration::worker::{
    DuplexTransport, IpcClient, RpcTransport, WorkerError, WorkerHandle, WorkerRegistry, WorkerSpec,
};

/// Mock spawner that creates `WorkerHandle`s backed by `DuplexTransport`.
///
/// Each spawned worker gets a mock server task that echoes JSON-RPC requests.
struct MockSpawner;

impl WorkerSpawner for MockSpawner {
    async fn spawn(
        &self,
        _creator_id: &str,
        _spec: &WorkerSpec,
    ) -> Result<WorkerHandle, WorkerError> {
        let (client_transport, server_transport) = DuplexTransport::new_pair();
        let server = Arc::new(Mutex::new(server_transport));

        // Spawn a background mock server that echoes requests.
        tokio::spawn(async move {
            loop {
                let line = {
                    let mut s = server.lock().await;
                    s.recv().await
                };
                match line {
                    Some(request_str) => {
                        if let Ok(req) = serde_json::from_str::<serde_json::Value>(&request_str) {
                            // Notifications have no id — skip reply.
                            if req.get("id").is_none() {
                                continue;
                            }
                            let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
                            let method = req
                                .get("method")
                                .and_then(|m| m.as_str())
                                .unwrap_or("unknown")
                                .to_string();

                            let response = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": { "echo_method": method }
                            });
                            let reply = serde_json::to_string(&response).expect("serialize");
                            let _ = server.lock().await.send(reply).await;
                        }
                    }
                    None => break,
                }
            }
        });

        let ipc = IpcClient::new(Box::new(client_transport));
        Ok(WorkerHandle::from_ipc_for_test(ipc))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registry_get_or_spawn_creates_worker() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    let spec = WorkerSpec::from_program("/bin/echo");

    let handle = registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("should spawn worker for new creator");

    assert_eq!(handle.pid(), 0, "mock worker should have PID 0");
    assert_eq!(registry.len(), 1);
}

#[tokio::test]
async fn registry_get_or_spawn_returns_existing() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    let spec = WorkerSpec::from_program("/bin/echo");

    let h1 = registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("first spawn");
    let pid1 = h1.pid();

    let h2 = registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("second call returns existing");

    // Both handles should point to the same worker (same PID).
    assert_eq!(pid1, h2.pid());
    assert_eq!(registry.len(), 1, "should still have exactly 1 worker");
}

#[tokio::test]
async fn registry_get_returns_handle() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    let spec = WorkerSpec::from_program("/bin/echo");

    let spawned = registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("spawn");
    let pid = spawned.pid();

    let found = registry
        .get("creator-a")
        .expect("should find existing creator");
    assert_eq!(found.pid(), pid);
}

#[tokio::test]
async fn registry_get_returns_none_for_unknown() {
    let registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    assert!(registry.get("nonexistent").is_none());
}

#[tokio::test]
async fn registry_remove_removes_entry() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    let spec = WorkerSpec::from_program("/bin/echo");

    registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("spawn");
    registry
        .get_or_spawn("creator-b", &spec)
        .await
        .expect("spawn");

    assert_eq!(registry.len(), 2);

    let removed = registry.remove("creator-a");
    assert!(removed.is_some(), "should return removed handle");
    assert_eq!(removed.unwrap().pid(), 0);

    assert_eq!(registry.len(), 1);
    assert!(
        registry.get("creator-a").is_none(),
        "removed creator should be gone"
    );
    assert!(
        registry.get("creator-b").is_some(),
        "other creator should remain"
    );
}

#[tokio::test]
async fn registry_remove_returns_none_for_unknown() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    assert!(registry.remove("nonexistent").is_none());
}

#[tokio::test]
async fn registry_capacity_rejects_overflow() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(2, MockSpawner);
    let spec = WorkerSpec::from_program("/bin/echo");

    registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("first");
    registry
        .get_or_spawn("creator-b", &spec)
        .await
        .expect("second");

    // Third creator should fail — at capacity.
    let result = registry.get_or_spawn("creator-c", &spec).await;

    match result {
        Err(WorkerError::Internal(msg)) => {
            assert!(
                msg.contains("capacity"),
                "error should mention capacity, got: {msg}"
            );
        }
        Ok(_) => panic!("expected capacity error, got Ok"),
        Err(e) => panic!("expected Internal capacity error, got: {e}"),
    }

    assert_eq!(registry.len(), 2);
}

#[tokio::test]
async fn registry_len_and_is_empty() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    let spec = WorkerSpec::from_program("/bin/echo");
    registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("spawn");

    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 1);
}

#[tokio::test]
async fn registry_shutdown_all() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(4, MockSpawner);
    let spec = WorkerSpec::from_program("/bin/echo");

    registry
        .get_or_spawn("creator-a", &spec)
        .await
        .expect("spawn");
    registry
        .get_or_spawn("creator-b", &spec)
        .await
        .expect("spawn");

    registry.shutdown_all().await.expect("shutdown_all");

    assert!(
        registry.is_empty(),
        "all workers should be removed after shutdown"
    );
}

#[tokio::test]
async fn registry_supports_sixteen_creators() {
    let mut registry: WorkerRegistry<MockSpawner> = WorkerRegistry::new(16, MockSpawner);
    let spec = WorkerSpec::from_program("/bin/echo");

    for i in 0..16u32 {
        let creator = format!("creator-{i}");
        registry
            .get_or_spawn(&creator, &spec)
            .await
            .unwrap_or_else(|e| panic!("should spawn {creator}: {e}"));
    }

    assert_eq!(registry.len(), 16);

    // Verify all creators are accessible.
    for i in 0..16u32 {
        let creator = format!("creator-{i}");
        assert!(
            registry.get(&creator).is_some(),
            "creator-{i} should be in registry"
        );
    }
}

// WorkerRegistry must be Send.
#[test]
fn worker_registry_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<WorkerRegistry<MockSpawner>>();
}
