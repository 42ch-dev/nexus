//! Mutex lock patterns have scoped drops.
#![allow(clippy::significant_drop_tightening)]
//! Mock subsystems for testing.
//!
//! Provides a single struct that implements all 5 subsystem mocks,
//! useful for integration tests where we want to control subsystem behavior.

use std::sync::Arc;
use tokio::sync::Mutex;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// Mock subsystem that can be configured to succeed or fail.
#[derive(Debug)]
struct MockSubsystemInner {
    should_succeed: bool,
    started: bool,
}

/// Mock subsystem for testing.
#[derive(Debug)]
pub struct MockSubsystem {
    /// Subsystem kind (immutable, no lock needed).
    kind: SubsystemKind,
    /// Mutable state (behind async Mutex).
    inner: Arc<Mutex<MockSubsystemInner>>,
}

impl MockSubsystem {
    fn new(kind: SubsystemKind, should_succeed: bool) -> Self {
        Self {
            kind,
            inner: Arc::new(Mutex::new(MockSubsystemInner {
                should_succeed,
                started: false,
            })),
        }
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for MockSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().await;
        if inner.should_succeed {
            inner.started = true;
            tracing::debug!("Mock subsystem {:?} started", self.kind);
            Ok(())
        } else {
            tracing::debug!("Mock subsystem {:?} failed to start", self.kind);
            anyhow::bail!("mock subsystem {:?} startup failure", self.kind);
        }
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().await;
        inner.started = false;
        tracing::debug!("Mock subsystem {:?} shutdown", self.kind);
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let inner = self.inner.lock().await;
        if inner.started {
            SubsystemHealth::Up
        } else {
            SubsystemHealth::Down
        }
    }

    fn kind(&self) -> SubsystemKind {
        self.kind
    }
}

/// Container for all mock subsystems.
///
/// Implements all 5 subsystem bootstraps for integration testing.
#[derive(Debug)]
pub struct MockAllSubsystems {
    http: MockSubsystem,
    db: MockSubsystem,
    sync: MockSubsystem,
    engine: MockSubsystem,
    worker_mgr: MockSubsystem,
}

impl MockAllSubsystems {
    /// Create mock subsystems where all succeed on startup.
    #[must_use] 
    pub fn all_succeed() -> Self {
        Self {
            http: MockSubsystem::new(SubsystemKind::Http, true),
            db: MockSubsystem::new(SubsystemKind::Db, true),
            sync: MockSubsystem::new(SubsystemKind::Sync, true),
            engine: MockSubsystem::new(SubsystemKind::Engine, true),
            worker_mgr: MockSubsystem::new(SubsystemKind::WorkerMgr, true),
        }
    }

    /// Create mock subsystems where one fails on startup.
    #[must_use] 
    pub fn one_fails(failing_kind: SubsystemKind) -> Self {
        Self {
            http: MockSubsystem::new(SubsystemKind::Http, failing_kind != SubsystemKind::Http),
            db: MockSubsystem::new(SubsystemKind::Db, failing_kind != SubsystemKind::Db),
            sync: MockSubsystem::new(SubsystemKind::Sync, failing_kind != SubsystemKind::Sync),
            engine: MockSubsystem::new(
                SubsystemKind::Engine,
                failing_kind != SubsystemKind::Engine,
            ),
            worker_mgr: MockSubsystem::new(
                SubsystemKind::WorkerMgr,
                failing_kind != SubsystemKind::WorkerMgr,
            ),
        }
    }

    /// Get all subsystems as a vector of trait objects.
    #[must_use] 
    pub fn as_bootstraps(&self) -> Vec<Arc<dyn SubsystemBootstrap>> {
        vec![
            Arc::new(self.http.clone()),
            Arc::new(self.db.clone()),
            Arc::new(self.sync.clone()),
            Arc::new(self.engine.clone()),
            Arc::new(self.worker_mgr.clone()),
        ]
    }

    /// Get the HTTP mock subsystem.
    #[must_use] 
    pub const fn http(&self) -> &MockSubsystem {
        &self.http
    }

    /// Get the DB mock subsystem.
    #[must_use] 
    pub const fn db(&self) -> &MockSubsystem {
        &self.db
    }

    /// Get the Sync mock subsystem.
    #[must_use] 
    pub const fn sync(&self) -> &MockSubsystem {
        &self.sync
    }

    /// Get the Engine mock subsystem.
    #[must_use] 
    pub const fn engine(&self) -> &MockSubsystem {
        &self.engine
    }

    /// Get the `WorkerMgr` mock subsystem.
    #[must_use] 
    pub const fn worker_mgr(&self) -> &MockSubsystem {
        &self.worker_mgr
    }
}

impl Clone for MockSubsystem {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            inner: Arc::clone(&self.inner),
        }
    }
}
