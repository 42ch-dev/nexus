//! Core-owned hosted production composition (v1.195 P0-T1/P0-T2).
//!
//! The hosted production owner is composed ONCE, from the state the core was
//! opened against — never from a caller-supplied path. This module owns both
//! halves of that composition:
//!
//! - [`CoreService::hosted_workspace_deps`] (P0-T1) resolves the SELECTED
//!   creator's canonical creative workspace root from the core's own home
//!   metadata, constructs the one durable workspace commit/recovery authority
//!   over the core's pool, settles every interrupted commit BEFORE returning,
//!   and hands back the [`RunnerDeps`] workspace ports;
//! - [`CoreService::start_hosted_execution`] (P0-T2), the public factory of
//!   current-host contracts §3.1, adds the Host-plane half — prompt executor,
//!   provider-catalog port, run-event port and the shared maps — plus the
//!   hosted scheduler, and calls [`CoreService::start_execution`] once.
//!
//! Why the root is resolved here rather than accepted as an argument: a
//! caller-supplied root would be a second, unvalidated workspace truth beside
//! the one the core was admitted for. [`RunnerDeps::workspace_root`] is the
//! root the engine's `Filesystem` gates resolve against, so it MUST be the same
//! root the commit authority commits through; building both from one canonical
//! value in this function is what makes them identical.

use std::collections::HashMap;
use std::path::PathBuf;
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
use crate::service::CoreService;

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

    /// Assemble the selected-root workspace port bundle for hosted production.
    ///
    /// The bundle carries the complete workspace half of the hosted owner: a
    /// real `workspace.open`/`workspace.commit` executor, the
    /// `_context.workspace.*` state provider, the durable commit authority and
    /// the recovered manager all three share, plus the frozen root and nexus
    /// home the engine needs. Every port is bound to ONE canonical root
    /// resolved from this core's own creator/workspace metadata.
    ///
    /// Startup intent recovery runs HERE, before any port is returned: a commit
    /// interrupted by a crash must reach its durable conclusion before the
    /// hosted owner can admit work. Recovery is scoped to the SELECTED root —
    /// another root's unsettled intents are left untouched, never settled,
    /// rolled back, failed on or deleted, because this authority was not
    /// admitted for them.
    ///
    /// # Errors
    /// - [`CoreError::AuthRequired`] when the service is closing or the
    ///   on-disk creator/workspace selection moved since open;
    /// - [`CoreError::Uninitialized`] when the selected workspace has no
    ///   registered creative root, or that root no longer exists;
    /// - [`CoreError::Busy`] when this DB already has a workspace
    ///   commit/recovery authority (a second bundle must never become a
    ///   duplicate writer), or when the selected root's own recovery state is
    ///   an unresolvable conflict;
    /// - [`CoreError::Internal`] when the environment prevents establishing the
    ///   authority (the lease file cannot be created or opened) or a
    ///   metadata/IO fault occurs.
    pub(crate) async fn hosted_workspace_deps(&self) -> CoreResult<RunnerDeps> {
        // The selection comes from the core's own config snapshot, so a caller
        // cannot inject a different creator/workspace into the bundle.
        let principal = self.active_principal().await?;
        let selected_root = self
            .work_workspace_path(&principal)?
            .filter(|root| !root.trim().is_empty())
            .ok_or(CoreError::Uninitialized)?;
        let canonical_root = canonical_workspace_root(&selected_root).await?;

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
            .startup_recovery_for_root(&canonical_root)
            .await
            .map_err(map_recovery_error)?;

        let executor: Arc<dyn WorkspaceExecutor> = Arc::new(WorkspaceCommitExecutor::new(
            Arc::clone(&manager),
            canonical_root.clone(),
        ));
        let state_provider: Arc<dyn WorkspaceStateProvider> = Arc::new(
            CoreWorkspaceStateProvider::new(Arc::clone(&manager), canonical_root.clone()),
        );
        let commit_authority =
            WorkspaceCommitAuthority::new(Arc::clone(&manager), canonical_root.clone());

        Ok(RunnerDeps {
            workspace_executor: Some(executor),
            workspace_state_provider: Some(state_provider),
            workspace_commit: Some(commit_authority),
            workspace_root: Some(PathBuf::from(&canonical_root)),
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

/// Canonicalize the creative root the current selection names.
///
/// A root that is named but no longer exists is an uninitialized workspace, not
/// an internal fault; every other failure is reported verbatim. Canonicalizing
/// is what makes the frozen run root, the executor's scope root and the commit
/// authority's active root the same string, so a symlinked or non-normalized
/// selection cannot produce two roots that compare unequal.
async fn canonical_workspace_root(selected_root: &str) -> CoreResult<String> {
    match tokio::fs::canonicalize(selected_root).await {
        Ok(path) => Ok(path.to_string_lossy().into_owned()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(CoreError::Uninitialized),
        Err(err) => Err(CoreError::Internal {
            category: format!("workspace root {selected_root}: {err}"),
        }),
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
        let operational = nexus_home_layout::operational_workspace_dir(user_home, CREATOR, SLUG);
        std::fs::create_dir_all(&operational).expect("operational dir");
        std::fs::write(
            operational.join("meta.json"),
            serde_json::to_vec(&serde_json::json!({ "local_root": creative_root }))
                .expect("meta json"),
        )
        .expect("meta");

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
