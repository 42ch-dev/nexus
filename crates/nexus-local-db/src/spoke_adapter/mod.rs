//! Production `BaselinePorts` implementation home (spec §7.4).
//!
//! `NexusBaselineAdapter` is the production spoke port impl backing spoke
//! orchestrators against this crate's SQLite storage. The port-family matrix
//! (which families are production vs stub) lives in
//! `.mstar/specs/spoke-adapter-architecture.md` §7.4.
//!
//! # Async ↔ sync bridge
//!
//! Spoke's port traits are **synchronous** (`fn ... -> SpokeResult<T>`) while
//! SQLite I/O is async. The adapter captures the current tokio runtime
//! [`Handle`] at construction and bridges each sync port method to async I/O
//! via `tokio::task::block_in_place` + `Handle::block_on`. This requires the
//! calling thread to be inside a tokio **multi-threaded** runtime — which the
//! production daemon uses (`tokio::runtime::Builder::new_multi_thread` in
//! `apps/nexus42/src/main.rs`). Construct the adapter from inside an async
//! context (e.g. an HTTP handler or a `#[tokio::test(flavor = "multi_thread")]`
//! test) so a runtime handle is available.

pub mod knowledge_entry_port;

use sqlx::SqlitePool;
use tokio::runtime::Handle;

/// Production `BaselinePorts` impl backing spoke orchestrators against nexus
/// SQLite storage.
///
/// See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the family
/// matrix (which families are production vs stub). Construct per-request from
/// a [`SqlitePool`] (cheap handle) **while inside a tokio multi-threaded
/// runtime**: the adapter captures the current runtime [`Handle`] and bridges
/// the sync spoke port trait to async SQLite I/O via
/// `tokio::task::block_in_place`.
///
/// # Panics
///
/// [`Self::new`] panics if no tokio runtime is running on the current thread.
pub struct NexusBaselineAdapter {
    pool: SqlitePool,
    handle: Handle,
}

impl NexusBaselineAdapter {
    /// Construct from the current tokio runtime.
    ///
    /// # Panics
    ///
    /// Panics if no tokio runtime is running on the current thread. Construct
    /// this from inside a tokio context (e.g. an async daemon handler or a
    /// `#[tokio::test(flavor = "multi_thread")]` test).
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            handle: Handle::current(),
        }
    }

    /// Bridge a sync trait method → async SQLite I/O.
    ///
    /// Requires the calling thread to be inside a tokio multi-threaded runtime
    /// (the production daemon uses `tokio::runtime::Builder::new_multi_thread`;
    /// see `apps/nexus42/src/main.rs`). `block_in_place` moves the current
    /// worker out of the scheduler while the SQLite future resolves elsewhere
    /// on the runtime.
    fn block_on<F, R>(&self, future: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        tokio::task::block_in_place(|| self.handle.block_on(future))
    }
}
