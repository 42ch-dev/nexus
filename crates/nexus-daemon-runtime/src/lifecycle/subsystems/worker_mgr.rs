//! Mutex lock patterns have scoped drops.
#![allow(clippy::significant_drop_tightening)]
//! Worker Manager subsystem — real implementation (WS-E T4).
//!
//! Manages the lifecycle of worker child processes for the daemon.
//! Uses `WorkerRegistry` from `nexus-orchestration` to track workers per creator.
//!
//! # Architecture
//!
//! - `WorkerMgrSubsystem` wraps a `WorkerRegistry<WorkerManagerSpawner>`.
//! - On `start()`: initializes an empty registry with configured capacity.
//! - On `shutdown()`: calls `registry.shutdown_all()` for graceful termination.
//! - Exposes `registry()` for engine/HTTP to spawn and manage workers.
//!
//! Design: `.mstar/plans/2026-04-21-v1.7-ws-e-multi-agent-worker.md` §8 T4.

use std::sync::Arc;
use tokio::sync::Mutex;

use nexus_orchestration::worker::{WorkerManager, WorkerManagerSpawner, WorkerRegistry};

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// Maximum concurrent workers per daemon (configurable in future).
pub const DEFAULT_MAX_WORKERS: usize = 16;

/// Real implementation of Worker Manager subsystem.
///
/// Replaces the mock stub from WS2. Holds a `WorkerRegistry` that manages
/// one worker process per active creator, using `WorkerManager` for spawning.
pub struct WorkerMgrSubsystem {
    /// Shared reference to the `WorkerRegistry`.
    ///
    /// Wrapped in `Arc<Mutex>` for async access from multiple callers
    /// (engine, HTTP endpoints, health checks).
    registry: Arc<Mutex<WorkerRegistry<WorkerManagerSpawner>>>,
    /// Whether the subsystem has been started.
    started: Arc<Mutex<bool>>,
}

impl WorkerMgrSubsystem {
    /// Create a new Worker Manager subsystem with default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_WORKERS)
    }

    /// Create a new Worker Manager subsystem with custom capacity.
    ///
    /// # Arguments
    ///
    /// * `max_workers` — maximum concurrent worker processes.
    #[must_use]
    pub fn with_capacity(max_workers: usize) -> Self {
        let manager = Arc::new(Mutex::new(WorkerManager::new()));
        let spawner = WorkerManagerSpawner::new(manager);
        let registry = WorkerRegistry::new(max_workers, spawner);

        Self {
            registry: Arc::new(Mutex::new(registry)),
            started: Arc::new(Mutex::new(false)),
        }
    }

    /// Create a Worker Manager subsystem wrapping an externally-constructed
    /// shared registry (V1.51 T-A P0 / QC3 F-001).
    ///
    /// Use this when the daemon boot needs to share the registry between the
    /// worker subsystem (for lifecycle management) and the capability layer
    /// (for `nexus.llm.extract` IPC dispatch via `ProductionWorkerProvider`).
    /// Both sides hold clones of the same `Arc<Mutex<...>>`, so a worker
    /// spawned by either is visible to the other.
    #[must_use]
    pub fn with_registry(registry: Arc<Mutex<WorkerRegistry<WorkerManagerSpawner>>>) -> Self {
        Self {
            registry,
            started: Arc::new(Mutex::new(false)),
        }
    }

    /// Return a shared reference to the `WorkerRegistry`.
    ///
    /// Other components (engine, HTTP endpoints) use this to:
    /// - Spawn workers for new creators.
    /// - Look up existing workers by creator ID.
    /// - Send IPC commands to workers.
    #[must_use]
    pub fn registry(&self) -> Arc<Mutex<WorkerRegistry<WorkerManagerSpawner>>> {
        self.registry.clone()
    }

    /// Check whether the subsystem has been started.
    async fn is_started(&self) -> bool {
        *self.started.lock().await
    }
}

impl Default for WorkerMgrSubsystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for WorkerMgrSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        let mut started = self.started.lock().await;
        if *started {
            tracing::warn!("WorkerMgrSubsystem already started, skipping");
            return Ok(());
        }

        // Registry is already initialized in constructor.
        // No additional setup needed at start time.
        let worker_count = self.registry.lock().await.len();
        tracing::info!(max_workers = worker_count, "WorkerMgr subsystem started");

        *started = true;
        Ok(())
    }

    async fn shutdown(&self, grace_ms: u64) -> anyhow::Result<()> {
        let mut started = self.started.lock().await;
        if !*started {
            tracing::warn!("WorkerMgrSubsystem not started, shutdown skipped");
            return Ok(());
        }

        let worker_count = self.registry.lock().await.len();
        tracing::info!(
            grace_ms,
            workers = worker_count,
            "WorkerMgr subsystem initiating shutdown"
        );

        // Gracefully shut down all registered workers.
        let mut registry = self.registry.lock().await;
        registry.shutdown_all().await?;

        tracing::info!("WorkerMgr subsystem shutdown complete");
        *started = false;
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let started = self.is_started().await;
        if !started {
            return SubsystemHealth::Down;
        }

        // If the registry is empty, it's still healthy (no workers needed yet).
        // Future health checks could inspect worker crash events.
        SubsystemHealth::Up
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::WorkerMgr
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subsystem_starts_and_shuts_down() {
        let subsystem = WorkerMgrSubsystem::new();

        // Initially not started.
        assert_eq!(subsystem.health().await, SubsystemHealth::Down);

        // Start should succeed.
        subsystem.start().await.expect("start ok");
        assert_eq!(subsystem.health().await, SubsystemHealth::Up);

        // Shutdown should succeed.
        subsystem.shutdown(5000).await.expect("shutdown ok");
        assert_eq!(subsystem.health().await, SubsystemHealth::Down);
    }

    #[tokio::test]
    async fn subsystem_with_custom_capacity() {
        let subsystem = WorkerMgrSubsystem::with_capacity(4);
        subsystem.start().await.expect("start ok");

        // Registry should be accessible.
        let registry = subsystem.registry();
        let r = registry.lock().await;
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn idempotent_start() {
        let subsystem = WorkerMgrSubsystem::new();
        subsystem.start().await.expect("start ok");
        subsystem.start().await.expect("second start ok");
        assert_eq!(subsystem.health().await, SubsystemHealth::Up);
    }

    #[tokio::test]
    async fn idempotent_shutdown() {
        let subsystem = WorkerMgrSubsystem::new();
        subsystem
            .shutdown(5000)
            .await
            .expect("shutdown not started ok");
        subsystem.start().await.expect("start ok");
        subsystem.shutdown(5000).await.expect("shutdown ok");
        subsystem.shutdown(5000).await.expect("second shutdown ok");
        assert_eq!(subsystem.health().await, SubsystemHealth::Down);
    }

    #[test]
    fn default_capacity_is_16() {
        let _subsystem = WorkerMgrSubsystem::new();
        // We can't directly check capacity, but we know it's 16 from the constant.
        assert_eq!(DEFAULT_MAX_WORKERS, 16);
    }
}
