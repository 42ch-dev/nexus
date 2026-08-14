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
//! # Async surface (V1.153 P0 T2)
//!
//! spoke-operations 0.9.1 converted the adapter port traits to
//! `#[async_trait] async fn` (and `orchestrate_*` to native `async fn`), so
//! the port impls are now natively async: each method awaits `SQLite` I/O
//! directly on the caller's runtime. The former sync bridge
//! (`Handle::block_on` + `tokio::task::block_in_place`) is gone; the adapter
//! no longer captures a runtime handle and can be constructed anywhere.

pub mod activation;
pub mod computable_port;
pub mod finding_port;
pub mod fork_port;
pub mod host_manifest_port;
pub mod knowledge_entry_port;
pub mod mca_read;
pub mod mind_state;
pub mod narrative_read;
pub mod relation_port;
pub mod rule_query_port;
pub mod scope_query_port;

use nexus_wasm_host::ModuleCache;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Production `BaselinePorts` impl backing spoke orchestrators against nexus
/// `SQLite` storage.
///
/// See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the family
/// matrix (which families are production vs stub). Construct per-request from
/// a [`SqlitePool`] (cheap handle clone); the port methods are natively
/// `async fn` (spoke-operations 0.9.1 surface) and await `SQLite` I/O on the
/// caller's runtime — no runtime handle is captured.
///
/// When `with_tx_cell` is used, the lifetime parameter ties the adapter to the
/// handler-owned `sqlx::Transaction` for the duration of one orchestrate call.
pub struct NexusAdapter<'a> {
    pool: SqlitePool,
    /// Injected installation identity (`~/.nexus42/device-id` UUID) for the
    /// `HostCapabilityManifest`. `None` → `HostManifestPort` resolves the
    /// device id from the standard nexus home on demand. V1.148 P3 N-C0:
    /// replaces the former static `"nexus-local"` host id (honesty lock —
    /// installation-scoped stable id, not a `PeerId` / world id).
    host_id: Option<String>,
    /// Host-local compute module store (`~/.nexus42/modules/`) when
    /// configured (V1.154 P2 — the Connect host). `None` ⇒ `ComputablePort`
    /// serves only the embedded ship set (baseline consumers, V1.146
    /// behavior unchanged). When set, compute modules MUST be installed
    /// under `<dir>/<id>/<id>.wasm` + `<dir>/<id>/manifest.json` — bytes
    /// are never peer-supplied (spec §2.1).
    user_modules_dir: Option<PathBuf>,
    /// Per-adapter compiled-module cache (P2 QC fix wave FW-2): the
    /// `ComputablePort` module load reuses `nexus_wasm_host::ModuleCache`,
    /// keyed by (module id, bytes hash), so the expensive wasmtime compile
    /// runs once per distinct module content instead of once per compute
    /// invocation. The Connect host constructs ONE adapter per process
    /// (per-process singleton), so the cache is process-wide there; a
    /// changed module file (new bytes hash) recompiles and overwrites the
    /// entry (the cache's only eviction).
    module_cache: ModuleCache,
    /// When set (via [`Self::with_tx_cell`]), `put_knowledge_entry` joins this
    /// transaction instead of opening its own. The handler moves the
    /// `sqlx::Transaction` into the shared cell before `orchestrate_promote`
    /// and takes it back out for sibling writes + `commit()`.
    bound_tx_cell: Option<Arc<Mutex<Option<sqlx::Transaction<'a, sqlx::Sqlite>>>>>,
}

impl NexusAdapter<'static> {
    /// Construct from a [`SqlitePool`] (cheap handle clone).
    ///
    /// V1.153 P0 T2: no tokio runtime is captured anymore — the port methods
    /// are natively `async fn` (spoke-operations 0.9.1 surface) and await
    /// `SQLite` I/O on the caller's runtime, so the former multi-threaded
    /// runtime requirement (`block_in_place` bridge) is gone.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        NexusAdapter {
            pool,
            host_id: None,
            user_modules_dir: None,
            module_cache: ModuleCache::new(),
            bound_tx_cell: None,
        }
    }
}

#[allow(clippy::elidable_lifetime_names)]
impl<'a> NexusAdapter<'a> {
    /// Inject the installation identity used by [`crate::HostManifestPort`]
    /// (`HostCapabilityManifest.host_id`).
    ///
    /// V1.148 P3 N-C0 honesty lock: the manifest `host_id` is the installation
    /// device-id UUID (`~/.nexus42/device-id`), not a static `"nexus-local"`.
    /// When no id is injected, the port resolves the device id from the
    /// standard nexus home on demand (see `adapter/host_manifest_port.rs`).
    /// Tests inject an id to stay hermetic.
    #[must_use]
    pub fn with_host_id(mut self, host_id: impl Into<String>) -> Self {
        self.host_id = Some(host_id.into());
        self
    }

    /// Configure the host-local compute module store (V1.154 P2 — the
    /// Connect host's `~/.nexus42/modules/`). When set, `ComputablePort`
    /// resolves compute modules from `<dir>/<id>/<id>.wasm` +
    /// `<dir>/<id>/manifest.json` ONLY (spec §2.1 — module bytes are never
    /// peer-supplied; an absent/incomplete pair is a client-input reject).
    /// Without a store the embedded ship set is used (baseline consumers,
    /// V1.146 behavior unchanged).
    #[must_use]
    pub fn with_user_modules_dir(mut self, dir: PathBuf) -> Self {
        self.user_modules_dir = Some(dir);
        self
    }

    /// The configured host-local module store, if any — the Connect compute
    /// gate requires one (a host without a store serves no compute module).
    #[must_use]
    pub fn user_modules_dir(&self) -> Option<&Path> {
        self.user_modules_dir.as_deref()
    }

    /// The per-adapter compiled-module cache (P2 QC fix wave FW-2). Test
    /// seam: in-crate tests observe cache hits / recompiles through it.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn module_cache(&self) -> &ModuleCache {
        &self.module_cache
    }

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
    /// `orchestrate_promote` and the other `orchestrate_*` entrypoints (now
    /// native `async fn`) are awaited from async handlers while the bound
    /// transaction cell is active. The handler must keep the [`Arc`] alive and
    /// must not commit/rollback until after the awaited orchestrator returns.
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
