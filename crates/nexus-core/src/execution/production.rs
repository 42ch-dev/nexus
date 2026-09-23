//! Core-owned hosted production composition (v1.195 P0-T1/P0-T2).
//!
//! The hosted production owner is composed ONCE, from the state the core was
//! opened against — never from a caller-supplied path. This module owns both
//! halves of that composition:
//!
//! - [`CoreService::hosted_workspace_deps`] (P0-T1) takes the canonical
//!   creative workspace root the engine-owner admission PINNED at open,
//!   constructs the one durable workspace commit/recovery authority over the
//!   core's pool, settles every interrupted commit BEFORE returning, and hands
//!   back the [`RunnerDeps`] workspace ports;
//! - [`CoreService::start_hosted_execution`] (P0-T2), the public factory of
//!   current-host contracts §3.1, adds the Host-plane half — prompt executor,
//!   provider-catalog port, run-event port and the shared maps — plus the
//!   hosted scheduler, and calls [`CoreService::start_execution`] once.
//!
//! Why the root is the OPEN-TIME PIN rather than a value read here: the native
//! boot binds its Host probe boundary through
//! [`CoreService::admission_creative_root`], so a selected-metadata write
//! landing between that probe and this factory used to hand the same owner a
//! Host bound to one root and execution/commit authority bound to another. A
//! caller-supplied root would be a second, unvalidated workspace truth beside
//! the one the core was admitted for, and a fresh read here is the same race
//! one step later. [`RunnerDeps::workspace_root`] is the root the engine's
//! `Filesystem` gates resolve against, so it MUST be the same root the commit
//! authority commits through; taking both from the one pinned value is what
//! makes them identical.
//!
//! The pin is also where a root this build cannot OWN is refused. Every port
//! built below is `String`-typed while the Host binds raw path bytes, so a
//! canonical root with no lossless UTF-8 form could only be carried by
//! substituting U+FFFD — pointing the ports at a different, possibly existing,
//! directory than the Host probes. Such a root never becomes a pin
//! ([`crate::works::canonical_selected_workspace_root`]), and this factory
//! re-checks the invariant with [`crate::works::lossless_root_str`] before it
//! constructs a single port, so a substituted root can never be published. The
//! drift check that compares the CURRENT selection with that pin
//! ([`crate::works::selection_matches_pinned_root`]) classifies by the raw
//! canonical path alone, so a selection that moved to an unrepresentable root
//! is refused as stale rather than as an environment fault.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use nexus_agent_host::config::TimeoutConfig;
use nexus_agent_host::{HostFacade, ProviderId};
use nexus_orchestration::capability::{PromptExecutor, WorkspaceExecutor, WorkspaceStateProvider};
use nexus_orchestration::run_state::{RunRecord, WorkflowStateStore};
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_provider_ports::ProviderPort;

use crate::error::{CoreError, CoreResult};
use crate::execution::executor::WorkspaceCommitExecutor;
use crate::execution::lifecycle::{ExecutionHandle, ExecutionOpenError, RunnerDeps};
use crate::execution::prompt_executor::HostPromptExecutor;
use crate::execution::run_events::{PageError, RunEventRegistry, RunEventSinkMap, RunPage};
use crate::execution::schedules::HostedSchedulerConfig;
use crate::execution::session::{SessionError, WorkspaceSessionManager};
use crate::execution::state_provider::CoreWorkspaceStateProvider;
use crate::execution::workflow::{ProviderCatalogPort, RunEventPort};
use crate::execution::workspace::WorkspaceCommitAuthority;
use crate::service::{CoreAccess, CoreService};

impl CoreService {
    /// Start the ONE hosted execution owner for this core (v1.195 P0-T2).
    ///
    /// The public factory of current-host contracts §3.1. It composes the
    /// complete hosted owner from state the core ALREADY owns and calls
    /// [`CoreService::start_execution`] exactly once:
    ///
    /// 1. the selected-root workspace bundle (P0-T1) — canonical creative root
    ///    resolved from this core's own creator/workspace metadata, startup
    ///    intent recovery settled BEFORE any port is published;
    /// 2. the ONE Host prompt executor
    ///    ([`HostPromptExecutor::new_with_run_event_sinks`]) over that durable
    ///    store, publishing host events into the shared sink map;
    /// 3. the shared per-run cancellation map and the bounded per-run event
    ///    registry/sink map (ring reservation, run-state publication, terminal
    ///    closing) the coordinator reads through [`RunEventPort`];
    /// 4. the provider-catalog port over the SAME Host, so admission validates
    ///    frozen role bindings against live native+ACP catalog truth;
    /// 5. the hosted scheduler (through
    ///    [`RunnerDeps::hosted_scheduler`](crate::execution::lifecycle::RunnerDeps)):
    ///    the production supervisor with the coordinator-backed
    ///    [`crate::execution::workflow::CoordinatorScheduleRunStarter`], plus
    ///    the ONE clock task — installed before recovery, started after it.
    ///
    /// Callers cannot override the creator, the canonical workspace root or the
    /// DB: every one of those comes from the opened core. No second Host is
    /// created and no SQL is owned here — the caller passes the Host it already
    /// owns.
    ///
    /// What this factory deliberately does NOT do:
    ///
    /// - It does not guess a default binding provider. Explicit frozen
    ///   `agent_bindings` (what the public add path freezes) win; a row with
    ///   prompt roles and no explicit binding refuses at admission with a typed
    ///   error instead of being bound to an arbitrary catalog row.
    /// - It does not start unrelated historical jobs (cron staggering,
    ///   refresh, SOUL narrative). Only schedule admission is clocked here.
    ///
    /// # Errors
    /// - [`ExecutionOpenError::Workspace`] when the selected workspace cannot
    ///   be composed (uninitialized root, a rival workspace authority, or an
    ///   environment/storage fault);
    /// - the [`CoreService::start_execution`] refusals (not engine owner,
    ///   already owned, closing).
    pub async fn start_hosted_execution(
        &self,
        host: Arc<dyn HostFacade>,
        providers: Arc<dyn ProviderPort>,
        timeouts: TimeoutConfig,
    ) -> Result<Arc<ExecutionHandle>, ExecutionOpenError> {
        // Refuse a non-owner core BEFORE composing anything: the admission root
        // is pinned for engine-owner opens only, so a service-only profile must
        // keep the documented `NotEngineOwner` refusal rather than be answered
        // with a workspace refusal it would have reached later.
        if self.inner.access != CoreAccess::EngineOwner {
            return Err(ExecutionOpenError::NotEngineOwner(self.inner.access));
        }
        let mut deps = self
            .hosted_workspace_deps()
            .await
            .map_err(ExecutionOpenError::Workspace)?;

        // ONE bounded ring registry plus the ONE sink map: the coordinator
        // reserves/publishes/closes a run's ring through the port while the
        // prompt executor publishes that run's host events into this map.
        let registry = Arc::new(RunEventRegistry::new());
        let sinks: RunEventSinkMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let workflow_store: Arc<dyn WorkflowStateStore> =
            Arc::new(SqliteSessionStorage::new(Arc::new(self.inner.pool.clone())));
        deps.prompt_executor = Some(Arc::new(HostPromptExecutor::new_with_run_event_sinks(
            Arc::clone(&host),
            workflow_store,
            timeouts,
            Some(Arc::clone(&sinks)),
        )) as Arc<dyn PromptExecutor>);
        // Catalog presence is a candidate, never readiness: this port answers
        // "known" only, and the caller owns the readiness probe.
        deps.provider_catalog = Some(Arc::new(HostProviderCatalogPort::new(Arc::clone(&host))));
        deps.run_events = Some(Arc::new(CoreRunEventPort::new(registry, sinks)));
        // ONE shared cancellation map: the engine, the coordinator and every
        // cancel path that fires a run's token must resolve the same token.
        deps.session_cancels = Some(Arc::new(std::sync::RwLock::new(HashMap::new())));
        // The owner's own supervisor, coordinator-backed starter and clock task.
        deps.hosted_scheduler = Some(HostedSchedulerConfig::from_env());

        self.start_execution(providers, deps).await
    }

    /// Assemble the workspace port bundle for hosted production.
    ///
    /// The bundle carries the complete workspace half of the hosted owner: a
    /// real `workspace.open`/`workspace.commit` executor, the
    /// `_context.workspace.*` state provider, the durable commit authority and
    /// the recovered manager all three share, plus the frozen root and nexus
    /// home the engine needs. Every port is bound to ONE canonical root — the
    /// engine-owner admission's OPEN-TIME PIN, the same value the native boot
    /// bound its Host probe to.
    ///
    /// Startup intent recovery runs HERE, before any port is returned: a commit
    /// interrupted by a crash must reach its durable conclusion before the
    /// hosted owner can admit work. Recovery is scoped to the PINNED root —
    /// another root's unsettled intents are left untouched, never settled,
    /// rolled back, failed on or deleted, because this authority was not
    /// admitted for them.
    ///
    /// # Errors
    /// - [`CoreError::Closing`] when the service has begun closing, and
    ///   [`CoreError::AuthRequired`] when the on-disk creator/workspace
    ///   selection — including the selected `local_root` — no longer matches
    ///   the pinned admission. The comparison is over the raw canonical path, so
    ///   a moved selection stays this class even when its replacement root has
    ///   no lossless UTF-8 form: it MOVED, and the next open admits it as a new
    ///   epoch;
    /// - [`CoreError::Uninitialized`] when the pinned admission registered no
    ///   creative root (absent, blank, or a root that no longer existed at
    ///   open);
    /// - [`CoreError::Busy`] when this DB already has a workspace
    ///   commit/recovery authority (a second bundle must never become a
    ///   duplicate writer), or when the selected root's own recovery state is
    ///   an unresolvable conflict;
    /// - [`CoreError::Internal`] when the environment prevents establishing the
    ///   authority (the lease file cannot be created or opened), when a
    ///   metadata/IO fault occurs, or when the pinned canonical root has no
    ///   lossless UTF-8 form — the `String`-typed workspace ports could only
    ///   carry a U+FFFD-substituted root, which is a different directory, so the
    ///   bundle is refused instead of published over a substituted path. (The
    ///   pin already refuses such a root at open, so in practice this factory
    ///   re-check never refuses on its own.)
    pub(crate) async fn hosted_workspace_deps(&self) -> CoreResult<RunnerDeps> {
        // The selection comes from the core's own config snapshot, so a caller
        // cannot inject a different creator/workspace into the bundle.
        let principal = self.active_principal().await?;
        // ONE root for this whole owner: the canonical creative root the
        // engine-owner admission was PINNED to at open. Resolving the selected
        // metadata again here is what let a write landing between the Host
        // probe and this factory publish a Host bound to one root beside
        // execution/commit authority bound to another, so the pin — not a fresh
        // read — is the authority every port below is built from.
        let canonical_root = match &self.inner.admission_root {
            Ok(Some(root)) => root.clone(),
            Ok(None) => return Err(CoreError::Uninitialized),
            Err(err) => return Err(err.clone()),
        };
        // The pin stays the authority, but a selection that MOVED away from it
        // makes this admission stale: refuse BEFORE any authority exists, so
        // neither the probed root's lane nor a stale root's ports are published
        // as one owner. The next open (a new epoch) admits the moved root.
        //
        // The comparison is over the raw canonical path, BEFORE the current
        // selection's representability is required: a replacement root the
        // `String`-typed ports could not carry is still a MOVED selection, so it
        // keeps this stale-admission class here. Only the PIN-TIME selection can
        // become the Host/probe owner, and that one passed (or was refused at)
        // the pin's own lossless gate.
        if !crate::works::selection_matches_pinned_root(
            &self.inner.nexus_home,
            principal.creator_id(),
            principal.workspace_slug(),
            &canonical_root,
        )? {
            tracing::warn!(
                pinned = %canonical_root.display(),
                "the selected creative root moved after this engine-owner admission was pinned; \
                 refusing to compose a workspace bundle for a stale admission"
            );
            return Err(CoreError::AuthRequired);
        }
        // The ports below are `String`-typed, so the pinned root is carried
        // through the SAME lossless check the pin itself passed. A lossy
        // conversion here is what let one owner assemble a Host bound to the
        // real root while its executor, state provider, commit authority and
        // recovery filter addressed a U+FFFD-substituted path — a different,
        // possibly existing, directory. Re-checking the invariant at the very
        // seam the ports are built from is what makes it unbypassable: no port
        // is ever constructed from a substituted root.
        let canonical_root_str = crate::works::lossless_root_str(&canonical_root)?.to_owned();

        // ONE manager per DB: `new_recoverable` takes the exclusive
        // workspace-authority lease, so a second authority over the same DB
        // refuses instead of becoming a rival writer.
        let manager = Arc::new(
            WorkspaceSessionManager::new_recoverable(
                Arc::new(self.inner.pool.clone()),
                self.inner.db_path.clone(),
            )
            .map_err(map_authority_error)?,
        );
        manager
            .startup_recovery_for_root(&canonical_root_str)
            .await
            .map_err(map_recovery_error)?;

        let executor: Arc<dyn WorkspaceExecutor> = Arc::new(WorkspaceCommitExecutor::new(
            Arc::clone(&manager),
            canonical_root_str.clone(),
        ));
        let state_provider: Arc<dyn WorkspaceStateProvider> = Arc::new(
            CoreWorkspaceStateProvider::new(Arc::clone(&manager), canonical_root_str.clone()),
        );
        let commit_authority =
            WorkspaceCommitAuthority::new(Arc::clone(&manager), canonical_root_str);

        Ok(RunnerDeps {
            workspace_executor: Some(executor),
            workspace_state_provider: Some(state_provider),
            workspace_commit: Some(commit_authority),
            // The pin itself, not a re-parsed string: the engine's `Filesystem`
            // gates then resolve against the exact bytes the native Host probes.
            workspace_root: Some(canonical_root),
            nexus_home: Some(self.inner.nexus_home.clone()),
            ..RunnerDeps::default()
        })
    }
}

/// Adapts the execution layer's provider-catalog port onto the ONE Host.
///
/// The catalog read is the same one admission always performed (explicit
/// config → PATH scan → ACP registry); the port only stops the execution layer
/// from naming `HostFacade`/`ProviderId`. It answers WHETHER a provider is
/// known — never whether it is ready, which stays the boot caller's probe.
struct HostProviderCatalogPort {
    host: Arc<dyn HostFacade>,
}

impl HostProviderCatalogPort {
    fn new(host: Arc<dyn HostFacade>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl ProviderCatalogPort for HostProviderCatalogPort {
    async fn provider_available(&self, provider_id: &str) -> Result<bool, String> {
        let catalog = self
            .host
            .provider_catalog()
            .await
            .map_err(|e| e.to_string())?;
        Ok(catalog
            .find(&ProviderId::new(provider_id.to_string()))
            .is_some())
    }
}

/// Adapts the bounded per-run registry to the coordinator's [`RunEventPort`].
///
/// Every operation forwards to the existing registry, so item/byte/subscriber
/// accounting, ring reuse, the shared sink map and terminal-closing semantics
/// stay owned by [`RunEventRegistry`] — the execution layer only reserves,
/// publishes and releases.
struct CoreRunEventPort {
    registry: Arc<RunEventRegistry>,
    /// The SAME sink map the prompt executor was constructed with, so a
    /// reserved ring is visible to that executor's host events.
    sinks: RunEventSinkMap,
}

impl CoreRunEventPort {
    fn new(registry: Arc<RunEventRegistry>, sinks: RunEventSinkMap) -> Self {
        Self { registry, sinks }
    }
}

#[async_trait]
impl RunEventPort for CoreRunEventPort {
    async fn try_register_live(&self, run_id: &str) -> bool {
        let Some(sink) = self.registry.try_register_live(run_id) else {
            return false;
        };
        self.sinks.lock().await.insert(run_id.to_string(), sink);
        true
    }

    async fn remove_live(&self, run_id: &str) {
        self.sinks.lock().await.remove(run_id);
    }

    fn publish_run_state(&self, run_id: &str, record: &RunRecord) {
        self.registry.publish_run_state(run_id, record);
    }

    fn mark_terminal(&self, run_id: &str) {
        self.registry.mark_terminal(run_id);
    }

    fn read_page(
        &self,
        run_id: &str,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> Result<RunPage, PageError> {
        // Forwards to the same bounded ring the SSE surface reads, so caps and
        // explicit-gap semantics cannot drift between the two readers.
        self.registry.read_page(run_id, after_sequence, limit)
    }
}

/// Map a refused workspace-authority construction onto the neutral taxonomy.
///
/// [`SessionError::AuthorityBusy`] is a WRITER conflict: this DB already has a
/// workspace commit/recovery authority, so the refusal is retryable. Every
/// other failure came from the environment — the lease file cannot be created
/// or opened (read-only mount, permissions, unusable path) — and is reported as
/// [`CoreError::Internal`], never as "another writer holds this DB".
fn map_authority_error(err: SessionError) -> CoreError {
    match err {
        SessionError::AuthorityBusy => {
            tracing::error!("workspace commit/recovery authority is held by another writer");
            CoreError::Busy
        }
        other => {
            tracing::error!(error = %other, "workspace authority could not be established");
            CoreError::Internal {
                category: format!("workspace authority: {other}"),
            }
        }
    }
}

/// Map a refused startup intent recovery onto the neutral taxonomy.
///
/// A recovery conflict means an unsettled intent for some root could not be
/// resolved (another writer owns it, or its stored metadata is corrupt), so the
/// bundle must not publish ports over that state: it is a writer conflict. Any
/// other failure is a storage fault.
fn map_recovery_error(err: SessionError) -> CoreError {
    match err {
        SessionError::RecoveryConflict(workspace_root) => {
            tracing::error!(
                workspace_root = %workspace_root,
                "workspace startup recovery conflict; refusing to publish workspace ports"
            );
            CoreError::Busy
        }
        other => CoreError::Internal {
            category: format!("workspace startup recovery: {other}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::Engine as _;
    use nexus_contracts::local::orchestration::{WorkspaceCommitInput, WorkspaceOpenInput};
    use nexus_contracts::{
        CoreWorkspaceCommitRequest, CoreWorkspaceCommitRequestChangesItem,
        CoreWorkspaceCommitRequestChangesItemOp,
    };
    use serial_test::serial;

    use crate::execution::session::{ChangeEntry, ChangeOp, SessionId};
    use crate::execution::test_hooks;
    use crate::service::{CoreAccess, CoreOpenOptions};

    const CREATOR: &str = "test_creator";
    const SLUG: &str = "default";
    const PAYLOAD: &[u8] = b"hosted workspace payload\n";

    /// A user home whose selected workspace registers a real creative root.
    struct Fixture {
        tmp: tempfile::TempDir,
        creative_root: std::path::PathBuf,
    }

    async fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().expect("tempdir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        std::fs::create_dir_all(&nexus_home).expect("nexus home");
        let creative_root = user_home.join("creative");
        // The opened scope the bundle commits through.
        std::fs::create_dir_all(creative_root.join("notes")).expect("creative root");
        std::fs::write(
            nexus_home.join("config.toml"),
            format!(
                "active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\"\n"
            ),
        )
        .expect("config");
        // The core resolves the creative root from the operational meta.json —
        // the same document the daemon/CLI workspace registration writes.
        std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            user_home, CREATOR, SLUG,
        ))
        .expect("operational dir");
        register_selected_root(user_home, serde_json::json!(creative_root));

        let db_path = nexus_home_layout::workspace_state_db_path(user_home, CREATOR, SLUG);
        // Seed through a temporary admitted pool and RELEASE the writer guard
        // before the engine owner opens: the owner takes an OS lock, not a
        // stealable lease, so the seeder must be gone first.
        {
            let guarded = nexus_local_db::init_engine_pool(&db_path)
                .await
                .expect("engine pool init");
            sqlx::query(
                "INSERT OR IGNORE INTO creators (creator_id, display_name, status, \
                 cached_at, data) VALUES (?, 'Hosted', 'active', datetime('now'), '{}')",
            )
            .bind(CREATOR)
            .execute(guarded.pool())
            .await
            .expect("seed the admitted creator row");
            guarded.pool().close().await;
            nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);
        }

        Fixture { tmp, creative_root }
    }

    async fn open_core(fixture: &Fixture) -> CoreService {
        CoreService::open(CoreOpenOptions {
            user_home: fixture.tmp.path().to_path_buf(),
            access: CoreAccess::EngineOwner,
        })
        .await
        .expect("engine-owner core open")
    }

    /// Rewrite the selected workspace's registered `local_root`.
    ///
    /// This is the supported operational write the CLI's `creator workspace
    /// create --creative-root` performs, not a private seed: the same
    /// document the admission pin and the drift check read.
    fn register_selected_root(user_home: &std::path::Path, local_root: serde_json::Value) {
        std::fs::write(
            nexus_home_layout::operational_workspace_dir(user_home, CREATOR, SLUG)
                .join("meta.json"),
            serde_json::to_vec(&serde_json::json!({ "local_root": local_root }))
                .expect("meta json"),
        )
        .expect("write meta");
    }

    /// The fixture with its selected `local_root` set BEFORE any core opens.
    async fn fixture_registering(local_root: serde_json::Value) -> Fixture {
        let fx = fixture().await;
        register_selected_root(fx.tmp.path(), local_root);
        fx
    }

    /// The lease path the workspace commit/recovery authority takes.
    fn authority_lease_path(core: &CoreService) -> std::path::PathBuf {
        core.inner
            .db_path
            .with_extension("workspace_authority.lock")
    }

    /// Is this error the FILESYSTEM's own refusal of a name whose bytes are not
    /// a valid sequence for it — the ONE condition that makes a non-UTF-8
    /// fixture name impossible to create?
    ///
    /// The condition is `EILSEQ` (`Errno::EILSEQ`; APFS answers it for a name
    /// that is not valid UTF-8, observed as raw OS error 92 on this host, and
    /// Linux filesystems answer the same errno). std maps it to
    /// `ErrorKind::Uncategorized` here, so no `ErrorKind` can name it. Every
    /// OTHER failure — a permission, resource or I/O fault — is a fixture-setup
    /// failure, and a fixture that never ran must not be reported as a pass.
    #[cfg(unix)]
    fn filesystem_rejects_non_utf8_names(error: &std::io::Error) -> bool {
        error.raw_os_error() == Some(nix::errno::Errno::EILSEQ as i32)
    }

    /// Only the filesystem's OWN refusal of the name bytes may be reported as a
    /// skip: every unrelated setup fault must fail the fixture instead of
    /// passing a case that never ran.
    #[cfg(unix)]
    #[test]
    fn only_the_filesystems_own_refusal_skips_the_non_utf8_fixture() {
        use std::io::Error;

        for errno in [
            nix::errno::Errno::EACCES,
            nix::errno::Errno::ENOSPC,
            nix::errno::Errno::EIO,
            nix::errno::Errno::EEXIST,
            nix::errno::Errno::ENOENT,
        ] {
            assert!(
                !filesystem_rejects_non_utf8_names(&Error::from_raw_os_error(errno as i32)),
                "{errno:?} is an unrelated setup fault, not an unsupported filesystem"
            );
        }
        assert!(
            !filesystem_rejects_non_utf8_names(&Error::from(std::io::ErrorKind::PermissionDenied)),
            "an error with no OS errno is not evidence of an unsupported filesystem"
        );
        assert!(
            filesystem_rejects_non_utf8_names(&Error::from_raw_os_error(
                nix::errno::Errno::EILSEQ as i32
            )),
            "the filesystem's own refusal of the name bytes is the one condition that may skip"
        );
    }

    fn create_entry(path: &str, content: &[u8]) -> ChangeEntry {
        ChangeEntry {
            path: path.to_string(),
            op: ChangeOp::Create,
            expected_hash: None,
            content_base64: Some(base64::engine::general_purpose::STANDARD.encode(content)),
        }
    }

    fn b64(content: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(content)
    }

    /// A one-change commit request in the generated wire shape.
    fn commit_request(session_id: &str, path: &str) -> CoreWorkspaceCommitRequest {
        CoreWorkspaceCommitRequest {
            changes: vec![CoreWorkspaceCommitRequestChangesItem {
                content_base64: Some(b64(PAYLOAD)),
                expected_hash: None,
                op: CoreWorkspaceCommitRequestChangesItemOp::Create,
                path: path.parse().expect("non-empty path"),
            }],
            session_id: session_id.parse().expect("non-empty session id"),
        }
    }

    /// Interrupt a commit that runs through the PRODUCTION authority port.
    ///
    /// `WorkspaceCommitAuthority::commit` runs on the RETAINED owner, which
    /// clears inherited crash points at spawn — so arming the seam before the
    /// call would prove nothing. The owner gate parks the owner at its
    /// admission boundary (claim held, nothing applied yet), the test arms the
    /// seam AFTER that clear, then releases it: no sleeps, no races. The
    /// interrupted commit is exactly what a process death at the apply leaves.
    async fn interrupted_authority_commit(
        authority: &WorkspaceCommitAuthority,
        session_id: &str,
        path: &str,
    ) -> CoreError {
        let gate = Arc::new(test_hooks::OwnerGate::for_session(session_id.to_string()));
        test_hooks::set_owner_gate(Some(Arc::clone(&gate)));
        let authority = authority.clone();
        let request = commit_request(session_id, path);
        let caller = tokio::spawn(async move { authority.commit(request).await });
        gate.admitted.notified().await;
        test_hooks::set_crash_point(Some("after_file_apply"));
        gate.proceed.notify_one();
        gate.settled.notified().await;
        test_hooks::set_owner_gate(None);
        test_hooks::set_crash_point(None);
        caller
            .await
            .expect("the retained commit owner joins")
            .expect_err("the armed crash point must interrupt the apply")
    }

    /// The workspace bundle must bind every port to the SELECTED root, refuse a
    /// foreign root and a second authority, and recover an interrupted commit
    /// before it publishes those ports.
    ///
    /// `#[serial]`: the crash seam is process-global, and the committed content
    /// must survive the recovery pass the reopened bundle runs.
    #[tokio::test]
    #[serial]
    async fn hosted_workspace_recovery_uses_selected_root() {
        let fx = fixture().await;
        let core = open_core(&fx).await;
        let canonical_root = std::fs::canonicalize(&fx.creative_root)
            .expect("canonical selected root")
            .to_string_lossy()
            .into_owned();
        // The interrupted session, observed after the reopen: it is what proves
        // the reopened bundle settled THAT intent rather than merely re-reading
        // the earlier one.
        let recovered_session: SessionId;

        {
            let deps = core.hosted_workspace_deps().await.expect("workspace bundle");

            // ── The bundle is bound to the selected root, not a caller path. ──
            assert_eq!(
                deps.workspace_root.as_deref(),
                Some(std::path::Path::new(canonical_root.as_str())),
                "the frozen run root must be the canonical selected root"
            );
            assert_eq!(deps.nexus_home.as_deref(), Some(core.nexus_home()));
            assert!(
                deps.workspace_state_provider.is_some(),
                "the workspace state provider is part of the bundle"
            );
            let executor = deps.workspace_executor.clone().expect("workspace executor");
            let authority = deps.workspace_commit.clone().expect("commit authority");
            assert_eq!(
                authority.active_root(),
                canonical_root,
                "the commit authority must commit through the selected root"
            );

            // ── No duplicate writer: a second bundle over the same DB must
            // refuse rather than open a rival commit/recovery authority. ──
            assert!(
                matches!(core.hosted_workspace_deps().await, Err(CoreError::Busy)),
                "a second workspace bundle must not become a duplicate writer"
            );

            // ── One authorized write through the real executor port. ──
            let opened = executor
                .open(WorkspaceOpenInput {
                    path: "notes".to_string(),
                })
                .await
                .expect("open session");
            let committed = executor
                .commit(WorkspaceCommitInput {
                    session_id: opened.session_id,
                    changes: vec![create_entry("first.txt", PAYLOAD)],
                })
                .await
                .expect("commit");
            assert!(committed.committed, "the commit must be durable");
            assert_eq!(
                std::fs::read(fx.creative_root.join("notes/first.txt")).expect("first bytes"),
                PAYLOAD
            );

            // ── A session persisted against a DIFFERENT root is refused, and
            // nothing is written under either root. ──
            let other_root = fx.tmp.path().join("other-root");
            std::fs::create_dir_all(&other_root).expect("other root");
            let other_root_string = other_root.to_string_lossy().into_owned();
            let manager = Arc::clone(authority.manager());
            let foreign_session = manager
                .open_session(&other_root_string, "", true)
                .await
                .expect("foreign session");
            assert!(
                matches!(
                    authority
                        .commit(commit_request(&foreign_session.to_string(), "escape.txt"))
                        .await,
                    Err(CoreError::Busy)
                ),
                "a foreign workspace root must be refused"
            );
            assert!(!fx.creative_root.join("escape.txt").exists());
            assert!(!other_root.join("escape.txt").exists());

            // ── An interrupted commit, produced through the PRODUCTION ports:
            // the session is opened by the bundle's executor and the commit is
            // interrupted inside the authority's retained owner. ──
            let opened = executor
                .open(WorkspaceOpenInput {
                    path: "notes".to_string(),
                })
                .await
                .expect("open recovery session");
            recovered_session = SessionId(opened.session_id.clone());
            let crashed =
                interrupted_authority_commit(&authority, &opened.session_id, "recovered.txt").await;
            match crashed {
                CoreError::Internal { ref category } => assert!(
                    category.contains("test_crash"),
                    "the armed crash point must be what interrupted the apply, got: {category}"
                ),
                other => panic!("the crash must surface as a storage fault, got: {other:?}"),
            }
            assert_eq!(
                std::fs::read(fx.creative_root.join("notes/recovered.txt")).expect("applied bytes"),
                PAYLOAD
            );
        }
        // The block scope is what releases the bundle: every clone of the
        // authority's manager (executor, authority) must be gone before the
        // reopen can take the workspace-authority lease again.

        // ── Reopen over the same DB: the next bundle settles the interrupted
        // intent before it publishes a port, and no committed content is lost. ──
        let reopened = core
            .hosted_workspace_deps()
            .await
            .expect("reopened bundle settles the interrupted commit");
        let state = reopened
            .workspace_state_provider
            .clone()
            .expect("state provider")
            .workspace_state()
            .await
            .expect("a committed workspace state");
        assert_eq!(state["committed"], serde_json::json!(true));
        assert_eq!(
            state["session_id"],
            serde_json::json!(recovered_session.to_string()),
            "recovery must settle the interrupted commit, not merely re-read the earlier one"
        );
        assert_eq!(state["change_count"], serde_json::json!(1));
        assert_eq!(state["workspace_root"], serde_json::json!(canonical_root));
        assert!(
            state["revision"]
                .as_str()
                .is_some_and(|revision| revision.starts_with("rev_")),
            "a durable revision must be reported, got: {state:?}"
        );
        for path in ["first.txt", "recovered.txt"] {
            assert_eq!(
                std::fs::read(fx.creative_root.join("notes").join(path)).expect("committed bytes"),
                PAYLOAD,
                "{path} must survive recovery"
            );
        }
    }

    /// The admission PIN — not a metadata re-read — is what a Host probe and
    /// every workspace port of the owner bind.
    ///
    /// A supported `local_root` write landing after open moves the SELECTION,
    /// but it can never move the root this admission was pinned to, so the
    /// owner refuses the stale admission (typed, before any authority exists)
    /// instead of publishing a Host bound to one root beside execution/commit
    /// authority bound to another. Restoring the pinned selection proves the
    /// refusal retained nothing: the same core composes again.
    #[tokio::test]
    async fn admission_root_is_pinned_across_a_moved_selection() {
        let fx = fixture().await;
        let core = open_core(&fx).await;
        let pinned = std::fs::canonicalize(&fx.creative_root).expect("canonical selected root");
        assert_eq!(
            core.admission_creative_root(),
            Some(pinned.as_path()),
            "the admission must pin the canonical registered creative root"
        );
        let lease_path = authority_lease_path(&core);

        // The moved root: a real directory the metadata now selects instead.
        let moved = fx.tmp.path().join("moved-creative-root");
        std::fs::create_dir_all(&moved).expect("moved root");
        register_selected_root(fx.tmp.path(), serde_json::json!(moved));

        assert_eq!(
            core.admission_creative_root(),
            Some(pinned.as_path()),
            "a metadata write after open must not move the pinned admission root"
        );
        assert!(
            matches!(
                core.hosted_workspace_deps().await.map(|_| ()),
                Err(CoreError::AuthRequired)
            ),
            "a selection that moved away from the pinned root must be refused"
        );
        assert!(
            !lease_path.exists(),
            "the refusal must land before any workspace authority is composed"
        );

        // Nothing was retained by the refusal: with the selection back on the
        // pinned root, the same admission composes the bundle it always would.
        register_selected_root(fx.tmp.path(), serde_json::json!(fx.creative_root));
        assert!(
            core.hosted_workspace_deps().await.is_ok(),
            "the refusal must not retain the workspace authority"
        );
    }

    /// A selected workspace that registers no USABLE canonical root — no
    /// registration, a blank path, or a root that no longer exists — pins no
    /// root, so no owner is admissible: the factory answers `Uninitialized`
    /// rather than composing ports over a fabricated boundary.
    #[tokio::test]
    async fn admission_without_a_usable_root_pins_no_owner() {
        for registered in [
            serde_json::Value::Null,
            serde_json::json!("   "),
            serde_json::json!("/nonexistent/nexus-t5-root"),
        ] {
            let fx = fixture_registering(registered.clone()).await;
            let core = open_core(&fx).await;
            assert!(
                core.admission_creative_root().is_none(),
                "a root registering {registered} must pin nothing"
            );
            assert!(
                matches!(
                    core.hosted_workspace_deps().await.map(|_| ()),
                    Err(CoreError::Uninitialized)
                ),
                "a pinned-empty admission must refuse with uninitialized"
            );
        }
    }

    /// The port seam refuses a canonical root with no lossless UTF-8 form, and
    /// the substitution it prevents would address a real, DIFFERENT directory.
    ///
    /// Runs wherever the harness runs: the seam is driven with an in-memory
    /// canonical root carrying non-UTF-8 path bytes — the value `canonicalize`
    /// yields for the symlink fixture below on a filesystem that can host it.
    #[cfg(unix)]
    #[test]
    fn a_root_without_a_lossless_utf8_form_is_refused_by_the_port_seam() {
        use std::os::unix::ffi::OsStrExt as _;

        let home = tempfile::tempdir().expect("sentinel home");
        let raw = home.path().join(std::ffi::OsStr::from_bytes(b"creative-\xff"));
        assert!(raw.to_str().is_none(), "the fixture must not be UTF-8");
        // The name a lossy conversion would substitute exists as a real
        // directory — the wrong root those ports must never commit through.
        let substituted = raw.to_string_lossy().into_owned();
        std::fs::create_dir(&substituted).expect("replacement-character directory");
        assert!(
            std::path::Path::new(&substituted).is_dir(),
            "the substituted path must be addressable"
        );
        assert!(
            matches!(
                crate::works::lossless_root_str(&raw),
                Err(CoreError::Internal { .. })
            ),
            "a root with no lossless UTF-8 form must be refused, not substituted"
        );
        let representable = home.path().join("creative-plain");
        assert_eq!(
            crate::works::lossless_root_str(&representable).ok(),
            representable.to_str(),
            "a representable root must pass through byte-identically"
        );
    }

    /// A selected root with no lossless UTF-8 form is refused AT THE PIN: no
    /// probe boundary, no owner and no workspace authority — instead of one
    /// owner whose Host probes the real bytes while its executor, state
    /// provider, commit authority and recovery filter write through the
    /// U+FFFD-substituted path.
    ///
    /// The fixture is the shape the defect needs: `meta.json.local_root` is an
    /// ordinary UTF-8 path to a SYMLINK whose target directory carries
    /// non-UTF-8 bytes, and the replacement-character path a lossy conversion
    /// would name exists as a DIFFERENT directory (the discriminating
    /// sentinel). A filesystem that cannot host non-UTF-8 names — macOS APFS
    /// answers `EILSEQ` for the create — cannot express the state this case is
    /// about, so it reports the skip instead of a hollow pass.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_canonical_root_without_a_lossless_utf8_form_is_refused_at_the_pin() {
        use std::os::unix::ffi::OsStringExt as _;

        let fx = fixture().await;
        let raw_root = fx.tmp.path().join("raw-root");
        std::fs::create_dir_all(&raw_root).expect("raw root parent");
        // One replacement character per invalid byte: `creative-\xff\xfe`
        // renders lossily as `creative-\u{FFFD}\u{FFFD}`.
        let target = raw_root.join(std::ffi::OsString::from_vec(b"creative-\xff\xfe".to_vec()));
        if let Err(error) = std::fs::create_dir(&target) {
            // ONLY a filesystem that refuses the fixture's name BYTES may skip:
            // any other failure here means the fixture never ran, which is a
            // failure of this test, not an unsupported-filesystem pass.
            assert!(
                filesystem_rejects_non_utf8_names(&error),
                "the non-UTF-8 root fixture could not be created for a reason other than the \
                 filesystem refusing unsupported name bytes, so this case did not run: {error}"
            );
            eprintln!(
                "skipping: this filesystem cannot host a non-UTF-8 directory name ({error}), \
                 so the lossy-canonical-root fixture cannot exist"
            );
            return;
        }
        let sentinel = raw_root.join("creative-\u{FFFD}\u{FFFD}");
        std::fs::create_dir(&sentinel).expect("the replacement-character directory");
        // The registration itself is representable: the symlink's NAME is
        // UTF-8, only its resolved target is not.
        let link = raw_root.join("creative-link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink to the non-UTF-8 root");
        let pinned = std::fs::canonicalize(&link).expect("the symlink resolves");
        assert!(
            pinned.to_str().is_none(),
            "the fixture must resolve to a non-UTF-8 canonical root, got {pinned:?}"
        );
        register_selected_root(fx.tmp.path(), serde_json::json!(link));
        let core = open_core(&fx).await;

        match core.hosted_workspace_deps().await {
            Err(CoreError::Internal { category }) => assert!(
                category.contains("not valid UTF-8"),
                "the refusal must name the representation fault, got {category}"
            ),
            Err(other) => panic!("the refusal must be the typed environment class, got {other:?}"),
            Ok(deps) => panic!(
                "a canonical root with no lossless UTF-8 form must not compose a workspace \
                 bundle: the ports were pointed at {:?} while the Host probes {pinned:?}",
                deps.workspace_root
            ),
        }
        // No pin means no probe boundary: the native boot binds its Host to the
        // PINNED root only, so a root the execution side cannot own is never
        // probed and no owner is ever admitted over it.
        assert!(
            core.admission_creative_root().is_none(),
            "a root with no lossless UTF-8 form must pin nothing"
        );
        assert!(
            !authority_lease_path(&core).exists(),
            "the refusal must land before any workspace authority lease exists"
        );
        for dir in [&sentinel, &target] {
            assert!(
                std::fs::read_dir(dir).expect("fixture dir").next().is_none(),
                "the refusal must not touch {}",
                dir.display()
            );
        }

        // Control over the same helper flow: a representable root still pins
        // and composes, so the refusal above is about the representation and
        // not about the fixture shape.
        let control = fixture().await;
        let core = open_core(&control).await;
        let expected =
            std::fs::canonicalize(&control.creative_root).expect("canonical control root");
        assert_eq!(core.admission_creative_root(), Some(expected.as_path()));
        let deps = core
            .hosted_workspace_deps()
            .await
            .expect("a representable root composes");
        assert_eq!(deps.workspace_root.as_deref(), Some(expected.as_path()));
    }

    /// A selection that MOVED to a root the `String`-typed ports could never
    /// carry is still a moved selection: the drift check classifies by the raw
    /// canonical path, so the factory keeps the stale-admission refusal class
    /// instead of reporting the environment fault the representability gate
    /// answers with.
    ///
    /// The fixture is the shape the defect needs: root A is pinned as an
    /// ordinary representable directory, then the registration selects a UTF-8
    /// symlink whose canonical TARGET is a non-UTF-8 directory B. Refusing B for
    /// its representation first — while the pin is valid and the ports were
    /// never built — would relabel a moved selection as an environment fault and
    /// hide that a later open (a new epoch, with its own pin) simply admits B;
    /// the same admission must instead refuse as stale, exactly as it does for a
    /// representable B, with no authority lease, no port and no touch of the
    /// directory a lossy conversion would have substituted.
    ///
    /// A filesystem that cannot host non-UTF-8 names cannot express B at all, so
    /// it reports the skip; any OTHER setup fault fails the test.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_moved_selection_to_a_non_utf8_root_is_refused_as_stale() {
        use std::os::unix::ffi::OsStringExt as _;

        let fx = fixture().await;
        let core = open_core(&fx).await;
        let pinned = std::fs::canonicalize(&fx.creative_root).expect("canonical selected root");
        assert_eq!(
            core.admission_creative_root(),
            Some(pinned.as_path()),
            "the admission must pin the canonical registered creative root"
        );
        let lease_path = authority_lease_path(&core);

        let raw_root = fx.tmp.path().join("raw-root");
        std::fs::create_dir_all(&raw_root).expect("raw root parent");
        let target = raw_root.join(std::ffi::OsString::from_vec(b"creative-\xff\xfe".to_vec()));
        if let Err(error) = std::fs::create_dir(&target) {
            assert!(
                filesystem_rejects_non_utf8_names(&error),
                "the moved non-UTF-8 root could not be created for a reason other than the \
                 filesystem refusing unsupported name bytes, so this case did not run: {error}"
            );
            eprintln!(
                "skipping: this filesystem cannot host a non-UTF-8 directory name ({error}), \
                 so the moved non-UTF-8 root cannot exist"
            );
            return;
        }
        // The directory a lossy conversion would have substituted instead.
        let sentinel = raw_root.join("creative-\u{FFFD}\u{FFFD}");
        std::fs::create_dir(&sentinel).expect("the replacement-character directory");
        // The registration itself is representable: the symlink's NAME is UTF-8.
        let link = raw_root.join("creative-link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink to the non-UTF-8 root");
        let moved = std::fs::canonicalize(&link).expect("the symlink resolves");
        assert!(
            moved.to_str().is_none(),
            "the moved root must resolve to non-UTF-8 bytes, got {moved:?}"
        );
        assert_ne!(moved, pinned, "the moved root must differ from the pin");
        register_selected_root(fx.tmp.path(), serde_json::json!(link));

        assert_eq!(
            core.admission_creative_root(),
            Some(pinned.as_path()),
            "a metadata write after open must not move the pinned admission root"
        );
        match core.hosted_workspace_deps().await.map(|_| ()) {
            Err(CoreError::AuthRequired) => {}
            Err(other) => panic!(
                "a selection that moved to a non-UTF-8 root is a STALE admission, not an \
                 environment fault, got {other:?}"
            ),
            Ok(()) => panic!(
                "a moved selection must not compose a workspace bundle (the Host probes \
                 {pinned:?})"
            ),
        }
        assert!(
            !lease_path.exists(),
            "the refusal must land before any workspace authority lease exists"
        );
        for dir in [&sentinel, &target] {
            assert!(
                std::fs::read_dir(dir).expect("fixture dir").next().is_none(),
                "the refusal must not touch {}",
                dir.display()
            );
        }

        // Nothing was retained by the refusal: the pinned selection composes the
        // bundle it always would, so no manager or port survived it.
        register_selected_root(fx.tmp.path(), serde_json::json!(fx.creative_root));
        assert!(
            core.hosted_workspace_deps().await.is_ok(),
            "the refusal must retain no workspace authority"
        );
    }

    /// A registration that cannot be read is NOT a moved selection: the drift
    /// check reports the fault verbatim instead of answering `AuthRequired`,
    /// which would tell a caller to re-open (a new epoch) for a root the
    /// document never named.
    ///
    /// Two faults the classification must keep distinct from drift: a document
    /// that does not parse, and a root that exists but cannot be canonicalized
    /// (`notes/plain.txt` is a regular file, so anything below it answers
    /// ENOTDIR rather than "no longer exists"). Neither may reach the workspace
    /// authority, and the pin itself is unaffected by both.
    #[tokio::test]
    async fn an_unresolvable_registration_is_not_reported_as_a_moved_selection() {
        let fx = fixture().await;
        let core = open_core(&fx).await;
        let pinned = std::fs::canonicalize(&fx.creative_root).expect("canonical selected root");
        let meta = nexus_home_layout::operational_workspace_dir(fx.tmp.path(), CREATOR, SLUG)
            .join("meta.json");

        std::fs::write(&meta, b"{ not json").expect("malformed registration");
        match core.hosted_workspace_deps().await.map(|_| ()) {
            Err(CoreError::Internal { .. }) => {}
            other => panic!("a malformed registration is an environment fault, got {other:?}"),
        }

        std::fs::write(fx.creative_root.join("notes/plain.txt"), b"not a directory")
            .expect("fixture file");
        register_selected_root(
            fx.tmp.path(),
            serde_json::json!(fx.creative_root.join("notes/plain.txt/inner")),
        );
        match core.hosted_workspace_deps().await.map(|_| ()) {
            Err(CoreError::Internal { .. }) => {}
            other => panic!("a canonicalization fault is an environment fault, got {other:?}"),
        }

        assert_eq!(
            core.admission_creative_root(),
            Some(pinned.as_path()),
            "the pin must survive both faults unchanged"
        );
        assert!(
            !authority_lease_path(&core).exists(),
            "neither refusal may reach the workspace authority"
        );
        register_selected_root(fx.tmp.path(), serde_json::json!(fx.creative_root));
        assert!(
            core.hosted_workspace_deps().await.is_ok(),
            "a valid registration must still compose the pinned bundle"
        );
    }

    /// Deterministic no-model provider port: the owner/close contract is what
    /// these cases exercise, never a provider effect.
    struct RejectingProviderPort;

    #[async_trait::async_trait]
    impl ProviderPort for RejectingProviderPort {
        async fn call(
            &self,
            _request: nexus_contracts::ProviderCall,
        ) -> nexus_provider_ports::ProviderResult<nexus_contracts::ProviderReply> {
            Err(nexus_contracts::CoreError {
                code: nexus_contracts::CoreErrorCode::Internal,
                message: "no live model in this test".to_string(),
                details: serde_json::Map::default(),
                http_status: Some(500),
            })
        }

        async fn next(
            &self,
            operation_id: String,
            _max_events: u32,
            _max_bytes: u32,
        ) -> nexus_provider_ports::ProviderResult<nexus_contracts::ProviderEventBatch> {
            Ok(nexus_contracts::ProviderEventBatch {
                operation_id,
                events: vec![],
                has_more: false,
                gap: None,
            })
        }
    }

    /// A CONFIRMED close releases the workspace commit/recovery authority the
    /// owner composed, so the next owner over the same home can be established
    /// in the SAME process — with a real (advanced) engine epoch, i.e. as a new
    /// admission rather than the settled generation's — even while the closed
    /// handle (and the manager it still references) is retained.
    ///
    /// Before the fix this is `AuthorityBusy`: the settled owner's engine,
    /// capability registry and commit authority keep the
    /// `state.workspace_authority.lock` lease open, so the home stays fenced
    /// after a close that reported `cleanup_confirmed`.
    ///
    /// Ordering matters and is asserted: the release happens only AFTER the
    /// owned scheduler has been joined and every drive has drained, and a LIVE
    /// owner still fences the home.
    #[tokio::test]
    async fn confirmed_close_releases_workspace_authority_to_the_next_same_process_owner() {
        let fx = fixture().await;

        // ── Owner A, in the production shape: the hosted scheduler clock is
        // installed, so the retained supervisor/starter/registry chain exists. ──
        let core_a = open_core(&fx).await;
        let mut deps = core_a.hosted_workspace_deps().await.expect("bundle a");
        let manager = Arc::clone(
            deps.workspace_commit
                .as_ref()
                .expect("commit authority")
                .manager(),
        );
        deps.hosted_scheduler = Some(HostedSchedulerConfig::from_env());
        let owner_a = core_a
            .start_execution(Arc::new(RejectingProviderPort), deps)
            .await
            .expect("owner a starts");
        let epoch_a = owner_a.engine_epoch();
        assert!(
            !owner_a.owned_tasks_finished(),
            "a live hosted owner still owns its scheduler clock task"
        );

        // ── A live duplicate is refused: the composed authority is held. ──
        assert!(
            matches!(
                core_a.hosted_workspace_deps().await.map(|_| ()),
                Err(CoreError::Busy)
            ),
            "a live owner still fences the home"
        );

        let report = core_a.close().await.expect("confirmed close");
        assert_eq!(
            report.state,
            nexus_contracts::CoreCloseReportState::Closed
        );
        assert!(report.cleanup_confirmed);
        assert!(
            owner_a.is_settled(),
            "the confirmed close joins the owned scheduler task and drains the drives"
        );
        assert!(
            owner_a.owned_tasks_finished(),
            "the confirmed close joins the owned scheduler clock task"
        );

        // `core_a`, `owner_a` and the manager clone are all still referenced
        // here on purpose: releasing the authority must not depend on any of
        // them being dropped.
        assert!(Arc::strong_count(&manager) > 1);

        // ── Owner B over the SAME home, in the SAME process. ──
        let core_b = open_core(&fx).await;
        let deps_b = core_b
            .hosted_workspace_deps()
            .await
            .expect("the confirmed close released the home");
        let owner_b = core_b
            .start_execution(Arc::new(RejectingProviderPort), deps_b)
            .await
            .expect("owner b starts");
        assert!(
            owner_b.engine_epoch() > epoch_a,
            "the next owner is a NEW admission over the home: {} -> {}",
            epoch_a,
            owner_b.engine_epoch()
        );

        // ── And the new owner fences the home again. ──
        assert!(
            matches!(
                core_b.hosted_workspace_deps().await.map(|_| ()),
                Err(CoreError::Busy)
            ),
            "the next owner holds the authority it composed"
        );

        owner_b.close().await.expect("close b");
        core_b.close().await.expect("close core b");
    }

    /// A settled hosted owner releases its whole composed generation once the
    /// owner and its service are dropped.
    ///
    /// The hosted composition installs a scheduler whose starter admitted back
    /// into the coordinator that owns it, so a STRONG back-edge from the
    /// starter closes a `coordinator → supervisor → starter → coordinator`
    /// cycle. Every member of that cycle — the engine, the capability registry
    /// (with the workspace executor), the engine's workspace-state provider and
    /// the prompt executor — then outlives the owner forever, and with them the
    /// selected home's `WorkspaceSessionManager` and its `state.workspace_
    /// authority.lock` lease. Only the explicit close-time release made the
    /// next same-process owner possible; the settled composition itself stayed
    /// referenced for the rest of the process.
    ///
    /// Both halves are asserted, so the fix has to be a NON-OWNING edge rather
    /// than a shorter-lived owner: while the owner is live the composition is
    /// retained AND the installed scheduler's admission still reaches that
    /// same coordinator, and after `close()` + dropping the owner and its
    /// service every probe must fail to upgrade while the starter refuses
    /// instead of admitting through a coordinator nobody owns.
    #[tokio::test]
    async fn settled_hosted_composition_is_released_when_the_owner_is_dropped() {
        let fx = fixture().await;
        let coordinator_probe: std::sync::Weak<
            crate::execution::workflow::WorkflowRunCoordinator,
        >;
        let manager_probe: std::sync::Weak<WorkspaceSessionManager>;
        {
            let core = open_core(&fx).await;
            let mut deps = core.hosted_workspace_deps().await.expect("bundle");
            // The manager is what the cycle ultimately retained: the engine's
            // state provider, the registry's workspace executor and the
            // handle's commit authority each clone this one `Arc`.
            let manager = Arc::clone(
                deps.workspace_commit
                    .as_ref()
                    .expect("commit authority")
                    .manager(),
            );
            manager_probe = Arc::downgrade(&manager);
            deps.hosted_scheduler = Some(HostedSchedulerConfig::from_env());
            let owner = core
                .start_execution(Arc::new(RejectingProviderPort), deps)
                .await
                .expect("hosted owner starts");
            coordinator_probe = Arc::downgrade(&owner.coordinator());
            let supervisor = owner
                .coordinator()
                .schedule_supervisor()
                .expect("the hosted owner installs its supervisor");
            let starter = supervisor
                .schedule_starter_clone()
                .expect("the supervisor is coordinator-backed");

            // HELD LIVE: the retained composition is exactly what a live owner
            // must still hold — the scheduler's admission upgrades back to this
            // same coordinator, so an unknown row is refused by ITS admission
            // and not by the missing-owner branch.
            assert!(
                coordinator_probe.upgrade().is_some(),
                "a live hosted owner retains its coordinator"
            );
            assert!(
                manager_probe.upgrade().is_some(),
                "a live hosted owner retains the home's session manager"
            );
            let live_refusal = starter
                .start("sched_absent")
                .await
                .expect_err("an unknown row is refused");
            assert!(
                live_refusal.to_string().contains("not found"),
                "a live tick must reach the owning coordinator's admission: {live_refusal}"
            );

            let report = core.close().await.expect("confirmed close");
            assert!(report.cleanup_confirmed, "the close must be confirmed");
            assert!(
                owner.owned_tasks_finished(),
                "the confirmed close joins the owned scheduler clock task"
            );
            drop(owner);

            // A clock tick that outlives its owner (the handle was dropped)
            // must FAIL CLOSED: the row stays pending rather than being
            // admitted through a coordinator nobody owns.
            let released_refusal = starter
                .start("sched_released")
                .await
                .expect_err("a released owner must refuse admission");
            assert!(
                released_refusal
                    .to_string()
                    .contains("no live execution owner"),
                "the released owner must fail closed (refusal: {released_refusal}, \
                 coordinator retained: {})",
                coordinator_probe.upgrade().is_some()
            );
            // This test's own handles to the supervisor chain are dropped, so
            // only the owner's composition can still retain the probes.
            drop(starter);
            drop(supervisor);
        }
        // The owner, its service and this test's own manager clone are gone.
        let coordinator_retained = coordinator_probe.upgrade().is_some();
        let manager_retained = manager_probe.upgrade().is_some();
        assert!(
            !coordinator_retained && !manager_retained,
            "the settled owner's composition must be released when the dropped owner \
             reported a confirmed close (coordinator retained: {coordinator_retained}, \
             session manager retained: {manager_retained})"
        );
    }

    /// A held workspace authority and an unusable lease path must not be
    /// reported alike: the first is a retryable writer conflict, the second is
    /// an environment fault.
    #[tokio::test]
    async fn workspace_authority_failure_distinguishes_busy_from_environment() {
        let fx = fixture().await;
        let core = open_core(&fx).await;

        // ── Environment fault: the lease file cannot even be opened (a
        // directory occupies its path), so no writer holds anything and the
        // refusal must not claim one does. ──
        let lease_path = core
            .inner
            .db_path
            .with_extension("workspace_authority.lock");
        std::fs::create_dir(&lease_path).expect("obstruct the lease path");
        match core.hosted_workspace_deps().await.map(|_| ()) {
            Err(CoreError::Internal { category }) => assert!(
                category.contains("workspace authority"),
                "unexpected category: {category}"
            ),
            other => panic!("an unusable lease path must be an environment fault, got: {other:?}"),
        }

        // ── With a usable lease path the first bundle succeeds, a genuine
        // second authority is still a conflict, and the lease is released with
        // the bundle. ──
        std::fs::remove_dir(&lease_path).expect("clear the obstruction");
        let first = core.hosted_workspace_deps().await.expect("first bundle");
        assert!(
            matches!(
                core.hosted_workspace_deps().await.map(|_| ()),
                Err(CoreError::Busy)
            ),
            "a genuinely held workspace authority must be refused as a conflict"
        );
        drop(first);
        assert!(
            core.hosted_workspace_deps().await.is_ok(),
            "the workspace authority is released with the bundle"
        );
    }

    /// Recovery through the bundle is scoped to the SELECTED root: another
    /// root's unsettled intent keeps its persisted state and its files.
    #[tokio::test]
    #[serial]
    async fn hosted_workspace_recovery_ignores_foreign_root_intents() {
        let fx = fixture().await;
        let core = open_core(&fx).await;
        let canonical_root = std::fs::canonicalize(&fx.creative_root)
            .expect("canonical selected root")
            .to_string_lossy()
            .into_owned();
        let foreign_root = fx.tmp.path().join("foreign-root");
        std::fs::create_dir_all(foreign_root.join("notes")).expect("foreign root");
        let foreign_root_string = std::fs::canonicalize(&foreign_root)
            .expect("canonical foreign root")
            .to_string_lossy()
            .into_owned();
        let selected_session: SessionId;

        {
            let deps = core.hosted_workspace_deps().await.expect("workspace bundle");
            let executor = deps.workspace_executor.clone().expect("workspace executor");
            let authority = deps.workspace_commit.clone().expect("commit authority");

            // The SELECTED root's interrupted commit, through the real ports.
            let opened = executor
                .open(WorkspaceOpenInput {
                    path: "notes".to_string(),
                })
                .await
                .expect("open selected session");
            selected_session = SessionId(opened.session_id.clone());
            let crashed =
                interrupted_authority_commit(&authority, &opened.session_id, "selected.txt").await;
            assert!(
                matches!(crashed, CoreError::Internal { ref category } if category.contains("test_crash")),
                "the armed crash point must interrupt the selected root's apply: {crashed:?}"
            );

            // A DIFFERENT root's interrupted commit. The bundle refuses foreign
            // roots by design, so this one is left through the manager's own
            // durable commit path — what an earlier process death would leave.
            let manager = Arc::clone(authority.manager());
            let foreign_session = manager
                .open_session(&foreign_root_string, "notes", true)
                .await
                .expect("foreign session");
            test_hooks::set_crash_point(Some("after_file_apply"));
            let foreign_crashed = manager
                .commit_session_durable(
                    &foreign_session,
                    &[create_entry("foreign.txt", PAYLOAD)],
                    &foreign_root_string,
                )
                .await;
            test_hooks::set_crash_point(None);
            assert!(
                matches!(foreign_crashed, Err(SessionError::Internal(_))),
                "the armed crash point must interrupt the foreign root's apply"
            );
            assert_eq!(
                std::fs::read(foreign_root.join("notes/foreign.txt")).expect("foreign applied bytes"),
                PAYLOAD
            );
        }

        // ── Reopen: the selected root is recovered, the foreign root is not. ──
        let reopened = core.hosted_workspace_deps().await.expect("reopened bundle");
        let state = reopened
            .workspace_state_provider
            .clone()
            .expect("state provider")
            .workspace_state()
            .await
            .expect("selected root workspace state");
        assert_eq!(state["committed"], serde_json::json!(true));
        assert_eq!(
            state["session_id"],
            serde_json::json!(selected_session.to_string())
        );
        assert_eq!(state["workspace_root"], serde_json::json!(canonical_root));

        let foreign_state: String = sqlx::query_scalar(
            "SELECT state FROM workspace_commit_intents WHERE workspace_root = ? \
             ORDER BY rowid DESC LIMIT 1",
        )
        .bind(&foreign_root_string)
        .fetch_one(&core.inner.pool)
        .await
        .expect("foreign intent row");
        assert_eq!(
            foreign_state, "applying",
            "the foreign root's intent must stay unsettled"
        );
        assert_eq!(
            std::fs::read(foreign_root.join("notes/foreign.txt")).expect("foreign bytes"),
            PAYLOAD,
            "the foreign root's applied files must be untouched"
        );
    }
}
