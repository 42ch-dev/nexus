//! `WorkerRegistry` — multi-creator worker index.
//!
//! Maps `CreatorId` → `WorkerHandle` with configurable capacity.
//! Workers are created via a [`WorkerSpawner`] trait implementation, allowing
//! tests to inject mock handles without spawning real child processes.
//!
//! Production usage: [`WorkerManagerSpawner`] wraps [`WorkerManager::spawn()`]
//! (WS-E T4 adds daemon integration).
//! Test usage: [`MockSpawner`] returns handles backed by [`DuplexTransport`].

use crate::worker::manager::{WorkerError, WorkerEvent, WorkerHandle, WorkerManager, WorkerSpec};
use crate::worker::{DuplexTransport, IpcClient};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// WorkerSpawner trait
// ---------------------------------------------------------------------------

/// Trait for spawning worker processes.
///
/// Implemented by [`WorkerManagerSpawner`] for production use and by mock structs
/// in tests.
#[allow(async_fn_in_trait)]
pub trait WorkerSpawner: Send + Sync {
    /// Spawn a new worker for the given creator using the provided spec.
    async fn spawn(&self, creator_id: &str, spec: &WorkerSpec)
        -> Result<WorkerHandle, WorkerError>;
}

// ---------------------------------------------------------------------------
// WorkerManagerSpawner — production implementation
// ---------------------------------------------------------------------------

/// Production implementation of [`WorkerSpawner`] that wraps [`WorkerManager`].
///
/// Used by the daemon's `WorkerMgrSubsystem` to spawn real worker processes.
///
/// # Example
///
/// ```ignore
/// let manager = WorkerManager::new();
/// let spawner = WorkerManagerSpawner::new(Arc::new(Mutex::new(manager)));
/// let registry = WorkerRegistry::new(16, spawner);
/// ```
pub struct WorkerManagerSpawner {
    /// Shared reference to the `WorkerManager`.
    manager: Arc<Mutex<WorkerManager>>,
}

impl WorkerManagerSpawner {
    /// Create a new spawner wrapping the given manager.
    pub const fn new(manager: Arc<Mutex<WorkerManager>>) -> Self {
        Self { manager }
    }

    /// Create a spawner with a fresh `WorkerManager`.
    #[must_use] 
    pub fn fresh() -> Self {
        Self::new(Arc::new(Mutex::new(WorkerManager::new())))
    }
}

impl WorkerSpawner for WorkerManagerSpawner {
    async fn spawn(
        &self,
        _creator_id: &str,
        spec: &WorkerSpec,
    ) -> Result<WorkerHandle, WorkerError> {
        let manager = self.manager.lock().await;
        manager.spawn(spec).await
    }
}

// ---------------------------------------------------------------------------
// WorkerRegistry
// ---------------------------------------------------------------------------

/// Index of active workers keyed by creator ID.
///
/// Generic over the spawner implementation so tests can inject mocks.
///
/// Supports:
/// - `get_or_spawn`: look up or create a worker for a creator.
/// - `get` / `remove`: direct map operations.
/// - `shutdown_all`: graceful shutdown of all tracked workers.
/// - `len` / `is_empty`: capacity queries.
pub struct WorkerRegistry<S: WorkerSpawner> {
    /// Broadcast sender for worker lifecycle events.
    #[allow(dead_code)]
    event_tx: broadcast::Sender<WorkerEvent>,
    /// Map from creator ID to worker handle.
    workers: HashMap<String, WorkerHandle>,
    /// Maximum number of concurrent workers.
    max_workers: usize,
    /// Worker spawner (production or mock).
    spawner: S,
}

impl<S: WorkerSpawner> WorkerRegistry<S> {
    /// Create a new registry with the given capacity and spawner.
    pub fn new(max_workers: usize, spawner: S) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            event_tx,
            workers: HashMap::new(),
            max_workers,
            spawner,
        }
    }

    /// Look up an existing worker for the given creator, or spawn a new one.
    ///
    /// If the creator already has a worker, returns the existing handle.
    /// If the registry is at capacity, returns a `WorkerError::Internal`.
    /// Otherwise, calls the spawner to create a new worker and inserts it.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerError`] if spawning a new worker fails.
    pub async fn get_or_spawn(
        &mut self,
        creator_id: &str,
        spec: &WorkerSpec,
    ) -> Result<&WorkerHandle, WorkerError> {
        if self.workers.contains_key(creator_id) {
            return self.workers.get(creator_id).ok_or_else(|| {
                WorkerError::Internal("handle disappeared after contains_key".to_string())
            });
        }

        if self.workers.len() >= self.max_workers {
            return Err(WorkerError::Internal(format!(
                "worker registry at capacity ({} / {})",
                self.workers.len(),
                self.max_workers
            )));
        }

        let handle = self.spawner.spawn(creator_id, spec).await?;
        info!(creator_id, pid = handle.pid(), "registered new worker");

        self.workers.insert(creator_id.to_string(), handle);
        // Safe: we just inserted.
        self.workers
            .get(creator_id)
            .ok_or_else(|| WorkerError::Internal("handle disappeared after insert".to_string()))
    }

    /// Get a reference to the handle for the given creator, if present.
    pub fn get(&self, creator_id: &str) -> Option<&WorkerHandle> {
        self.workers.get(creator_id)
    }

    /// Remove the worker for the given creator, returning the handle.
    pub fn remove(&mut self, creator_id: &str) -> Option<WorkerHandle> {
        debug!(creator_id, "removing worker from registry");
        self.workers.remove(creator_id)
    }

    /// Shut down all registered workers and clear the registry.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerError`] if any worker shutdown fails.
    pub async fn shutdown_all(&mut self) -> Result<(), WorkerError> {
        let count = self.workers.len();
        if count == 0 {
            return Ok(());
        }

        info!(count, "shutting down all workers");

        // Drain the map and shut down each worker.
        let handles: Vec<(String, WorkerHandle)> = self.workers.drain().collect();

        for (creator_id, mut handle) in handles {
            debug!(creator_id, pid = handle.pid(), "shutting down worker");
            if let Err(e) = handle.shutdown().await {
                tracing::warn!(
                    creator_id,
                    pid = handle.pid(),
                    error = %e,
                    "worker shutdown failed"
                );
            }
        }

        Ok(())
    }

    /// Number of registered workers.
    pub fn len(&self) -> usize {
        self.workers.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }

    /// Subscribe to worker lifecycle events.
    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<WorkerEvent> {
        self.event_tx.subscribe()
    }
}

impl<S: WorkerSpawner + Default> Default for WorkerRegistry<S> {
    fn default() -> Self {
        Self::new(16, S::default())
    }
}

// ---------------------------------------------------------------------------
// MockSpawner — test implementation
// ---------------------------------------------------------------------------

/// Mock implementation of [`WorkerSpawner`] for tests.
///
/// Creates [`WorkerHandle`] instances backed by [`DuplexTransport`] without
/// spawning real child processes. Useful for integration tests that need
/// to exercise the registry without the overhead of process spawning.
///
/// # Example
///
/// ```ignore
/// let spawner = MockSpawner::new();
/// let registry = WorkerRegistry::new(4, spawner);
///
/// // get_or_spawn will create a mock handle with IpcClient over DuplexTransport
/// let handle = registry.get_or_spawn("creator-1", &spec).await?;
/// ```
pub struct MockSpawner {
    /// Whether to simulate spawn failures.
    fail: bool,
}

impl MockSpawner {
    /// Create a mock spawner that successfully creates handles.
    #[must_use] 
    pub const fn new() -> Self {
        Self { fail: false }
    }

    /// Create a mock spawner that fails all spawn requests.
    #[must_use] 
    pub const fn failing() -> Self {
        Self { fail: true }
    }
}

impl Default for MockSpawner {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerSpawner for MockSpawner {
    async fn spawn(
        &self,
        creator_id: &str,
        _spec: &WorkerSpec,
    ) -> Result<WorkerHandle, WorkerError> {
        if self.fail {
            return Err(WorkerError::SpawnFailed(std::io::Error::other(
                "mock spawn failure",
            )));
        }

        // Create a duplex transport pair: one end for the daemon, one for the "worker".
        // For the mock, we only need the daemon end since we're not simulating a real worker.
        // The IpcClient will be able to send requests, but responses need to be provided
        // by the test (via the other end of the duplex).
        let (daemon_transport, _worker_transport) = DuplexTransport::new_pair();

        let ipc = IpcClient::new(Box::new(daemon_transport));
        let handle = WorkerHandle::from_ipc_for_test(ipc);

        info!(
            creator_id,
            pid = handle.pid(),
            "mock spawner created handle"
        );
        Ok(handle)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn worker_manager_spawner_creates_fresh() {
        let spawner = WorkerManagerSpawner::fresh();
        assert!(Arc::strong_count(&spawner.manager) == 1);
    }

    #[tokio::test]
    async fn mock_spawner_creates_handle() {
        let spawner = MockSpawner::new();
        let spec = WorkerSpec::from_program("test-program");
        let handle = spawner.spawn("test-creator", &spec).await;
        assert!(handle.is_ok());
        let h = handle.expect("handle");
        assert_eq!(h.pid(), 0); // Mock PID
    }

    #[tokio::test]
    async fn mock_spawner_failing() {
        let spawner = MockSpawner::failing();
        let spec = WorkerSpec::from_program("test-program");
        let result = spawner.spawn("test-creator", &spec).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn registry_get_or_spawn_with_mock() {
        let spawner = MockSpawner::new();
        let mut registry = WorkerRegistry::new(4, spawner);
        let spec = WorkerSpec::from_program("test");

        // First call should spawn.
        let handle_ref = registry.get_or_spawn("creator-1", &spec).await;
        assert!(handle_ref.is_ok());
        assert_eq!(registry.len(), 1);

        // Second call for same creator should return existing.
        let handle_ref2 = registry.get_or_spawn("creator-1", &spec).await;
        assert!(handle_ref2.is_ok());
        assert_eq!(registry.len(), 1); // Still 1, not 2.
    }

    #[tokio::test]
    async fn registry_capacity_limit() {
        let spawner = MockSpawner::new();
        let mut registry = WorkerRegistry::new(2, spawner);
        let spec = WorkerSpec::from_program("test");

        // Spawn 2 workers (capacity).
        registry.get_or_spawn("c1", &spec).await.expect("ok");
        registry.get_or_spawn("c2", &spec).await.expect("ok");
        assert_eq!(registry.len(), 2);

        // Third should fail due to capacity.
        let result = registry.get_or_spawn("c3", &spec).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn registry_shutdown_all() {
        let spawner = MockSpawner::new();
        let mut registry = WorkerRegistry::new(4, spawner);
        let spec = WorkerSpec::from_program("test");

        registry.get_or_spawn("c1", &spec).await.expect("ok");
        registry.get_or_spawn("c2", &spec).await.expect("ok");
        assert_eq!(registry.len(), 2);

        // Shutdown should clear the registry.
        registry.shutdown_all().await.expect("shutdown ok");
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn registry_remove() {
        let spawner = MockSpawner::new();
        let mut registry = WorkerRegistry::new(4, spawner);
        let spec = WorkerSpec::from_program("test");

        registry.get_or_spawn("c1", &spec).await.expect("ok");
        assert_eq!(registry.len(), 1);

        let removed = registry.remove("c1");
        assert!(removed.is_some());
        assert!(registry.is_empty());

        // Remove non-existent should return None.
        let removed2 = registry.remove("nonexistent");
        assert!(removed2.is_none());
    }
}
