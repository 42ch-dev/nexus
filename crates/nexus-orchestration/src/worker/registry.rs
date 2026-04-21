//! WorkerRegistry — multi-creator worker index.
//!
//! Maps `CreatorId` → `WorkerHandle` with configurable capacity.
//! Workers are created via a [`WorkerSpawner`] trait implementation, allowing
//! tests to inject mock handles without spawning real child processes.
//!
//! Production usage: `WorkerManagerSpawner` wraps `WorkerManager::spawn()`
//! (T4 adds daemon integration).
//! Test usage: `MockSpawner` returns handles backed by `DuplexTransport`.

use crate::worker::manager::{WorkerError, WorkerEvent, WorkerHandle, WorkerSpec};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// WorkerSpawner trait
// ---------------------------------------------------------------------------

/// Trait for spawning worker processes.
///
/// Implemented by `WorkerManager` for production use and by mock structs
/// in tests.
#[allow(async_fn_in_trait)]
pub trait WorkerSpawner: Send + Sync {
    /// Spawn a new worker for the given creator using the provided spec.
    async fn spawn(
        &self,
        creator_id: &str,
        spec: &WorkerSpec,
    ) -> Result<WorkerHandle, WorkerError>;
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
        info!(
            creator_id,
            pid = handle.pid(),
            "registered new worker"
        );

        self.workers.insert(creator_id.to_string(), handle);
        // Safe: we just inserted.
        self.workers.get(creator_id).ok_or_else(|| {
            WorkerError::Internal("handle disappeared after insert".to_string())
        })
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
