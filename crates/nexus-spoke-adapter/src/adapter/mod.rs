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
#[cfg(feature = "compute")]
pub mod computable_port;
#[cfg(not(feature = "compute"))]
pub mod computable_port_stub;
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

use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeOwnerRef, DISCLOSURE_OWNER_PRIVATE};
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_knowledge::world_kb::{KnowledgeEntryRecord, KnowledgeReadPolicy, KnowledgeReadScope};
use nexus_local_db::kb_store::SqliteKbStore;
#[cfg(feature = "compute")]
use nexus_wasm_host::ModuleCache;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::{Scope, SpokeReject, SpokeRejectCode};

/// Production `BaselinePorts` impl backing spoke orchestrators against nexus
/// `SQLite` storage.
///
/// See `.mstar/specs/spoke-adapter-architecture.md` §7.4 for the family
/// matrix (which families are production vs stub). Construct per-request from
/// a [`SqlitePool`] (cheap handle clone); the port methods are natively
/// `async fn` (spoke-operations 0.9.1 surface) and await `SQLite` I/O on the
/// caller's runtime — no runtime handle is captured.
///
/// # Request-bound knowledge scope (HARD, v1.191 P1 T8 / durable §4.1)
///
/// An adapter is constructed **either** with a validated request-bound
/// [`KnowledgeReadScope`] ([`NexusAdapter::new`]) **or** without one
/// ([`NexusAdapter::new_host`]). The two are not interchangeable:
///
/// | Port family | [`NexusAdapter::new`] | [`NexusAdapter::new_host`] |
/// |---|---|---|
/// | `KnowledgeEntryPort`, `ScopeQueryPort`, `RelationPort`, `FindingPort`, `ComputablePort` | served inside the bound selection | fail closed (`read_scope_missing`) |
/// | `HostManifestPort` (host metadata), `RuleQueryPort`, fork/timeline reads, mind-state carriers | served | served |
///
/// The host surface (host metadata and the connect tool reads) therefore needs
/// no knowledge scope, and it can never widen into a KE read: every
/// KE-capable method first requires the bound scope, and the selection is the
/// same one the local-db read primitives already enforce in SQL. A
/// client-supplied `Scope` can only *narrow* against it — a wire `viewpoint`
/// that is not the bound resolved holder is refused rather than trusted.
///
/// When `with_tx_cell` is used, the lifetime parameter ties the adapter to the
/// handler-owned `sqlx::Transaction` for the duration of one orchestrate call.
pub struct NexusAdapter<'a> {
    pool: SqlitePool,
    /// The admitted read authority for this adapter (durable §4.1).
    /// `None` = host metadata/tools construction: every KE-bearing port fails
    /// closed instead of reading outside an admitted selection.
    read_scope: Option<KnowledgeReadScope>,
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
    /// Compiled-module cache (P2 QC fix wave FW-2), keyed by
    /// `(module id, bytes hash, manifest hash)` so the expensive wasmtime
    /// compile runs once per distinct module content instead of once per
    /// compute invocation.
    ///
    /// V1.191 P1 T8 (HARD): the cache is **host-owned and injected**, not
    /// rebuilt per adapter. A per-request adapter (now the norm, because the
    /// read scope is request-bound) would otherwise carry an empty cache every
    /// time and recompile every module per request. The Connect host keeps one
    /// [`ModuleCache`] for the process and hands out `Arc` clones through
    /// [`NexusAdapter::with_module_cache`]; an adapter built without one owns a
    /// private cache (tests, one-shot CLI paths).
    #[cfg(feature = "compute")]
    module_cache: Arc<ModuleCache>,
    /// When set (via [`Self::with_tx_cell`]), `put_knowledge_entry` joins this
    /// transaction instead of opening its own. The handler moves the
    /// `sqlx::Transaction` into the shared cell before `orchestrate_promote`
    /// and takes it back out for sibling writes + `commit()`.
    bound_tx_cell: Option<Arc<Mutex<Option<sqlx::Transaction<'a, sqlx::Sqlite>>>>>,
}

impl NexusAdapter<'static> {
    /// Construct a KE-capable adapter bound to one validated request scope.
    ///
    /// V1.153 P0 T2: no tokio runtime is captured anymore — the port methods
    /// are natively `async fn` (spoke-operations 0.9.1 surface) and await
    /// `SQLite` I/O on the caller's runtime, so the former multi-threaded
    /// runtime requirement (`block_in_place` bridge) is gone.
    ///
    /// `read_scope` is the admitted selection the caller resolved from its
    /// principal (`nexus-core` admission or an equivalent validated request
    /// scope); the adapter never derives one from a client payload.
    #[must_use]
    pub fn new(pool: SqlitePool, read_scope: KnowledgeReadScope) -> Self {
        Self::with_read_scope(pool, Some(read_scope))
    }

    /// Construct the **host metadata/tools** adapter: no knowledge read scope.
    ///
    /// Used where only host metadata and the connect tool surface are served.
    /// Every KE-bearing port (`KnowledgeEntryPort`, `ScopeQueryPort`,
    /// `RelationPort`, `FindingPort`, `ComputablePort`) rejects with the
    /// `read_scope_missing` marker instead of reading knowledge — the host
    /// surface cannot bypass the admission boundary (durable §4.1).
    #[must_use]
    pub fn new_host(pool: SqlitePool) -> Self {
        Self::with_read_scope(pool, None)
    }

    fn with_read_scope(pool: SqlitePool, read_scope: Option<KnowledgeReadScope>) -> Self {
        NexusAdapter {
            pool,
            read_scope,
            host_id: None,
            user_modules_dir: None,
            #[cfg(feature = "compute")]
            module_cache: Arc::new(ModuleCache::new()),
            bound_tx_cell: None,
        }
    }
}

#[allow(clippy::elidable_lifetime_names)]
impl<'a> NexusAdapter<'a> {
    /// The bound admitted read selection, or `None` for a host-only adapter.
    #[must_use]
    pub const fn read_scope(&self) -> Option<&KnowledgeReadScope> {
        self.read_scope.as_ref()
    }

    /// Require the bound read selection of a KE-bearing port (durable §4.1).
    ///
    /// A host-only adapter ([`NexusAdapter::new_host`]) has none, so every KE
    /// read/mutate entry point fails closed rather than widening to an
    /// unscoped read.
    ///
    /// # Errors
    ///
    /// Returns the `read_scope_missing` reject when no scope is bound.
    pub(crate) fn require_read_scope(
        &self,
        context: &str,
    ) -> Result<&KnowledgeReadScope, SpokeReject> {
        self.read_scope
            .as_ref()
            .ok_or_else(|| unbound_scope_reject(context))
    }

    /// Whether the bound selection authorizes this container.
    #[must_use]
    pub(crate) fn admits_container(&self, owner: &KnowledgeOwnerRef) -> bool {
        self.read_scope
            .as_ref()
            .is_some_and(|selection| selection.containers().contains(owner))
    }

    /// The durable §4.2 visibility rule for one stored row, applied against
    /// the bound selection: the row must sit in an authorized container, and
    /// `owner-private` must resolve to a holder the policy admits (the exact
    /// resolved holder for `ActorView`, the known-governance holder set for
    /// `CreatorManagement`). Unknown disclosure vocabulary is never admitted.
    ///
    /// A host-only adapter admits nothing.
    #[must_use]
    pub(crate) fn admits_record(&self, record: &KnowledgeEntryRecord) -> bool {
        let Some(selection) = self.read_scope.as_ref() else {
            return false;
        };
        if !selection.containers().contains(&record.owner) {
            return false;
        }
        match record.disclosure.as_deref() {
            None => true,
            Some(DISCLOSURE_OWNER_PRIVATE) => {
                record
                    .holder_entry_id
                    .as_deref()
                    .is_some_and(|holder| match selection.policy() {
                        KnowledgeReadPolicy::ActorView => {
                            selection.holder_entry_id() == Some(holder)
                        }
                        KnowledgeReadPolicy::CreatorManagement => {
                            selection.authorized_holders().iter().any(|h| h == holder)
                        }
                    })
            }
            Some(_) => false,
        }
    }

    /// Load one knowledge entry by id **inside the bound selection**.
    ///
    /// The returned value is `None` both for an absent id and for a row the
    /// selection does not admit, so a hidden read-by-id is indistinguishable
    /// from a missing one at this boundary (durable §4.2). The row is fetched
    /// by primary key (one row), and the §4.2 rule above decides admission
    /// before any payload or id leaves this call — there is no
    /// cap/limit/ranking interaction to filter after.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError`] on storage failure (an absent row is `Ok(None)`).
    pub(crate) async fn load_admitted_entry(
        &self,
        entry_id: &str,
    ) -> Result<Option<KnowledgeEntryRecord>, KbStoreError> {
        let store = SqliteKbStore::new(self.pool.clone());
        let record = match store.get_knowledge_entry(entry_id).await {
            Ok(record) => record,
            Err(KbStoreError::NotFound(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        Ok(self.admits_record(&record).then_some(record))
    }

    /// Refuse a wire-supplied `Scope.viewpoint` that is not the bound resolved
    /// holder (durable §4.1/§9).
    ///
    /// `Scope.viewpoint` is the separately resolved holder `entry_id` of an
    /// ownership-aware operation; a client may only *narrow* with it. When the
    /// adapter's selection carries a resolved holder (`ActorView`), the wire
    /// value must equal it exactly; a management selection carries no resolved
    /// holder, so any supplied viewpoint is refused rather than interpreted as
    /// authority. The supplied id is never echoed back in the reject details.
    ///
    /// # Errors
    ///
    /// Returns an `InvalidInput` reject when the wire viewpoint does not match.
    pub(crate) fn refuse_foreign_scope_viewpoint(&self, scope: &Scope) -> Result<(), SpokeReject> {
        let Some(viewpoint) = scope.viewpoint.as_ref() else {
            return Ok(());
        };
        let bound_holder = self
            .read_scope
            .as_ref()
            .and_then(KnowledgeReadScope::holder_entry_id);
        if bound_holder == Some(viewpoint.as_str()) {
            return Ok(());
        }
        Err(SpokeReject {
            code: SpokeRejectCode::InvalidInput,
            message: "scope viewpoint does not match the bound read selection's resolved holder"
                .to_string(),
            details: Some(scope_reject_details(&scope.scope_id)),
        })
    }

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

    /// Share one host-owned compiled-module cache across adapters (v1.191 P1
    /// T8).
    ///
    /// The Connect host builds one [`ModuleCache`] per process and clones this
    /// `Arc` into every per-request adapter, so binding a request scope never
    /// recompiles a module the host already compiled. See the `module_cache`
    /// field docs for why the cache cannot stay per-adapter once the scope is
    /// request-bound.
    #[cfg(feature = "compute")]
    #[must_use]
    pub fn with_module_cache(mut self, cache: Arc<ModuleCache>) -> Self {
        self.module_cache = cache;
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

    /// The compiled-module cache backing this adapter. Test seam: in-crate
    /// tests observe cache hits / recompiles through it, and the shared-cache
    /// proof compares two scoped adapters over one cache.
    #[cfg(feature = "compute")]
    #[must_use]
    pub const fn module_cache(&self) -> &Arc<ModuleCache> {
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
            .is_some_and(|cell| cell.lock().is_ok_and(|guard| guard.is_some()))
    }
}

// ── Request-bound scope reject helpers (v1.191 P1 T8) ────────────────────

/// Contents marker for the missing-scope reject, so hosts classify it by
/// marker instead of sniffing the message (same pattern as
/// `module_identity_missing` / `world_conflict`).
const READ_SCOPE_MISSING_MARKER: &str = "read_scope_missing";

/// The fail-closed reject a KE-bearing port produces on a host-only adapter
/// ([`NexusAdapter::new_host`]).
///
/// It is an `InternalError` — a host wiring fault, not client input — and it
/// carries the [`READ_SCOPE_MISSING_MARKER`] details marker.
pub(crate) fn unbound_scope_reject(context: &str) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InternalError,
        message: format!(
            "{context} requires a request-bound knowledge read scope; this adapter was \
             constructed without one (NexusAdapter::new_host serves host metadata/tools only)"
        ),
        details: Some({
            let mut details = Map::new();
            details.insert(READ_SCOPE_MISSING_MARKER.to_string(), Value::Bool(true));
            details
        }),
    }
}

/// Details payload for a wire-scope refusal: the requested scope id only —
/// never the client-supplied viewpoint (durable §4.1 — a client value is not
/// authority and is not echoed as if it named one).
fn scope_reject_details(scope_id: &str) -> Map<String, Value> {
    let mut details = Map::new();
    details.insert("scope_id".to_string(), json!(scope_id));
    details
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
    ///
    /// v1.191 P1 T8: the KE-bearing families are served by a *scoped*
    /// adapter; the metadata/tools families are served by both constructions.
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
        let adapter = NexusAdapter::new(pool.clone(), test_read_scope());
        let host = NexusAdapter::new_host(pool);

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

        // The metadata/tool families stay reachable without any knowledge
        // scope; the KE-bearing ones are compiled for the type but fail closed
        // at runtime (proved in `tests/adapter_surface.rs`).
        accepts_rule_query_port(&host);
        accepts_host_manifest_port(&host);
        accepts_fork_timeline_port(&host);
    }

    /// Any well-formed selection: the trait-upcast proof never reads through it.
    fn test_read_scope() -> KnowledgeReadScope {
        KnowledgeReadScope::creator_management(
            vec![KnowledgeOwnerRef::world("wld_upcast")],
            Vec::new(),
        )
    }
}
