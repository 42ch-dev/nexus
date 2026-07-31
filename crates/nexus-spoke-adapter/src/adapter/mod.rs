//! Production `BaselinePorts` implementation home (spec §7.4).
//!
//! `NexusAdapter` is the production spoke port impl backing spoke
//! orchestrators against `nexus-local-db`'s `SQLite` storage. The port-family
//! matrix (which families are production vs stub) lives in
//! `.mstar/specs/spoke-adapter-architecture.md` §7.4.
//!
//! V1.145 P1b rehome: this module moved from
//! `nexus-local-db/src/spoke_adapter/` so that `nexus-local-db` is pure
//! storage (no spoke-adapter dep) and `nexus-spoke-adapter` is the capability
//! aggregation layer (spec §8 dep-graph reversal).
//!
//! # Async ↔ sync bridge
//!
//! Spoke's port traits are **synchronous** (`fn ... -> SpokeResult<T>`) while
//! `SQLite` I/O is async. The adapter captures the current tokio runtime
//! [`Handle`] at construction and bridges each sync port method to async I/O
//! via `tokio::task::block_in_place` + `Handle::block_on`. This requires the
//! calling thread to be inside a tokio **multi-threaded** runtime — which the
//! production daemon uses (`tokio::runtime::Builder::new_multi_thread` in
//! `apps/nexus42/src/main.rs`). Construct the adapter from inside an async
//! context (e.g. an HTTP handler or a `#[tokio::test(flavor = "multi_thread")]`
//! test) so a runtime handle is available.

pub mod activation;
pub mod computable_port;
pub mod finding_port;
pub mod fork_port;
pub mod host_manifest_port;
pub mod knowledge_entry_port;
pub mod mca_read;
pub mod narrative_read;
pub mod relation_port;
pub mod rule_query_port;
pub mod scope_query_port;

use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;

/// Production `BaselinePorts` impl backing spoke orchestrators against nexus
/// `SQLite` storage.
///
/// See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the family
/// matrix (which families are production vs stub). Construct per-request from
/// a [`SqlitePool`] (cheap handle clone) **while inside a tokio multi-threaded
/// runtime**: the adapter captures the current runtime [`Handle`] and bridges
/// the sync spoke port trait to async `SQLite` I/O via
/// `tokio::task::block_in_place`.
///
/// When `with_tx_cell` is used, the lifetime parameter ties the adapter to the
/// handler-owned `sqlx::Transaction` for the duration of one orchestrate call.
///
/// # Panics
///
/// [`Self::new`] panics if no tokio runtime is running on the current thread.
/// In debug builds it additionally panics if that runtime is **not**
/// multi-threaded — the `block_in_place` bridge used by every sync port
/// method requires a multi-threaded runtime (see [`Self::block_on`]).
pub struct NexusAdapter<'a> {
    pool: SqlitePool,
    handle: Handle,
    /// When set (via [`Self::with_tx_cell`]), `put_knowledge_entry` joins this
    /// transaction instead of opening its own. The handler moves the
    /// `sqlx::Transaction` into the shared cell before `orchestrate_promote`
    /// and takes it back out for sibling writes + `commit()`.
    bound_tx_cell: Option<Arc<Mutex<Option<sqlx::Transaction<'a, sqlx::Sqlite>>>>>,
}

impl NexusAdapter<'static> {
    /// Construct from the current tokio runtime.
    ///
    /// # Panics
    ///
    /// Panics if no tokio runtime is running on the current thread
    /// ([`Handle::current`]). In debug builds, additionally panics if the
    /// current runtime is **not** multi-threaded: `block_in_place` (used by
    /// [`Self::block_on`]) panics under a `current_thread` runtime, so this
    /// early check surfaces the misuse at construction rather than at the
    /// first port method call. Construct this from inside a multi-threaded
    /// tokio context (e.g. an async daemon handler or a
    /// `#[tokio::test(flavor = "multi_thread")]` test).
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        let handle = Handle::current();
        // W-2 (qc3): `Handle::current()` succeeds even for a `current_thread`
        // runtime, so the real guard is the flavor check. `block_in_place`
        // panics under a current-thread runtime; fail fast at construction in
        // debug builds so the panic points here, not at the first port call.
        // No-op in release builds.
        debug_assert!(
            handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread,
            "NexusAdapter requires a multi-threaded tokio runtime \
             (block_in_place panics under a current_thread runtime)"
        );
        NexusAdapter {
            pool,
            handle,
            bound_tx_cell: None,
        }
    }
}

#[allow(clippy::elidable_lifetime_names)]
impl<'a> NexusAdapter<'a> {
    /// Attach a shared transaction cell for the duration of one adopt/orchestrate
    /// call. The handler installs the open `sqlx::Transaction` in the cell before
    /// calling [`Self::with_bound_tx`], then removes it afterward for job flip +
    /// `commit()`.
    #[must_use]
    pub fn with_tx_cell(
        self,
        cell: Arc<Mutex<Option<sqlx::Transaction<'a, sqlx::Sqlite>>>>,
    ) -> Self {
        Self {
            bound_tx_cell: Some(cell),
            ..self
        }
    }

    /// Run `f` while the adapter's bound transaction cell (if any) is active.
    ///
    /// `orchestrate_promote` and other sync spoke orchestrators call this from
    /// async handlers via a short synchronous bridge (`block_in_place` inside
    /// `put_knowledge_entry`). The handler must keep the [`Arc`] alive and must
    /// not commit/rollback until after `f` returns.
    pub fn with_bound_tx<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        f()
    }

    pub(crate) fn take_bound_tx(&self) -> Option<sqlx::Transaction<'a, sqlx::Sqlite>> {
        let cell = self.bound_tx_cell.as_ref()?;
        cell.lock().ok()?.take()
    }

    pub(crate) fn restore_bound_tx(&self, tx: sqlx::Transaction<'a, sqlx::Sqlite>) {
        if let Some(cell) = &self.bound_tx_cell {
            if let Ok(mut guard) = cell.lock() {
                *guard = Some(tx);
            }
        }
    }

    pub(crate) fn is_bound(&self) -> bool {
        self.bound_tx_cell
            .as_ref()
            .is_some_and(|cell| cell.lock().ok().is_some_and(|guard| guard.is_some()))
    }

    /// Bridge a sync trait method → async `SQLite` I/O.
    ///
    /// Requires the calling thread to be inside a tokio multi-threaded runtime
    /// (the production daemon uses `tokio::runtime::Builder::new_multi_thread`;
    /// see `apps/nexus42/src/main.rs`). `block_in_place` moves the current
    /// worker out of the scheduler while the `SQLite` future resolves elsewhere
    /// on the runtime.
    fn block_on<F, R>(&self, future: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        tokio::task::block_in_place(|| self.handle.block_on(future))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time proof that `NexusAdapter` satisfies the
    /// `BaselinePorts` blanket impl once all 6 port families are in scope
    /// (spec §7.4 — production-vs-stub matrix is complete).
    ///
    /// Each helper accepts `&dyn <PortFamily>`; passing a
    /// `&NexusAdapter` performs the implicit trait-upcast that
    /// only compiles when the appropriate `impl <PortFamily> for
    /// NexusAdapter` block exists. The function body is empty
    /// — runtime behavior is exercised in the per-port `tests` modules.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn nexus_adapter_satisfies_baseline_ports_blanket_impl() {
        fn accepts_baseline_ports(_: &dyn crate::BaselinePorts) {}
        fn accepts_computable_ports(_: &dyn crate::ComputablePorts) {}
        fn accepts_fork_ports(_: &dyn crate::ForkPorts) {}
        fn accepts_computable_port(_: &dyn crate::ComputablePort) {}
        fn accepts_knowledge_entry_port(_: &dyn crate::KnowledgeEntryPort) {}
        fn accepts_relation_port(_: &dyn crate::RelationPort) {}
        fn accepts_scope_query_port(_: &dyn crate::ScopeQueryPort) {}
        fn accepts_finding_port(_: &dyn crate::FindingPort) {}
        fn accepts_rule_query_port(_: &dyn crate::RuleQueryPort) {}
        fn accepts_host_manifest_port(_: &dyn crate::HostManifestPort) {}
        fn accepts_fork_timeline_port(_: &dyn crate::ForkTimelineQueryPort) {}

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        let adapter = NexusAdapter::new(pool);

        accepts_baseline_ports(&adapter);
        accepts_computable_port(&adapter);
        accepts_computable_ports(&adapter);
        accepts_fork_ports(&adapter);
        accepts_fork_timeline_port(&adapter);
        accepts_knowledge_entry_port(&adapter);
        accepts_relation_port(&adapter);
        accepts_scope_query_port(&adapter);
        accepts_finding_port(&adapter);
        accepts_rule_query_port(&adapter);
        accepts_host_manifest_port(&adapter);
    }
}
