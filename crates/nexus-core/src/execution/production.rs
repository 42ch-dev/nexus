//! Core-owned hosted workspace production composition (v1.195 P0-T1).
//!
//! The hosted production owner is composed ONCE, from the state the core was
//! opened against — never from a caller-supplied path. This module owns the
//! workspace half of that composition: it resolves the SELECTED creator's
//! canonical creative workspace root from the core's own home metadata,
//! constructs the one durable workspace commit/recovery authority over the
//! core's pool, settles every interrupted commit BEFORE returning, and hands
//! back the [`RunnerDeps`] workspace ports consumed by
//! [`crate::CoreService::start_execution`].
//!
//! The public `start_hosted_execution` factory (prompt/catalog/starter
//! adapters, the shared maps and the owned scheduler) is composed by the
//! following task and is deliberately absent here: this module produces a
//! bundle, it never starts an engine and never accepts work.
//!
//! Why the root is resolved here rather than accepted as an argument: a
//! caller-supplied root would be a second, unvalidated workspace truth beside
//! the one the core was admitted for. [`RunnerDeps::workspace_root`] is the
//! root the engine's `Filesystem` gates resolve against, so it MUST be the same
//! root the commit authority commits through; building both from one canonical
//! value in this function is what makes them identical.

use std::path::PathBuf;
use std::sync::Arc;

use nexus_orchestration::capability::{WorkspaceExecutor, WorkspaceStateProvider};

use crate::error::{CoreError, CoreResult};
use crate::execution::executor::WorkspaceCommitExecutor;
use crate::execution::lifecycle::RunnerDeps;
use crate::execution::session::{SessionError, WorkspaceSessionManager};
use crate::execution::state_provider::CoreWorkspaceStateProvider;
use crate::execution::workspace::WorkspaceCommitAuthority;
use crate::service::CoreService;

impl CoreService {
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
    /// hosted owner can admit work against the same root.
    ///
    /// # Errors
    /// - [`CoreError::AuthRequired`] when the service is closing or the
    ///   on-disk creator/workspace selection moved since open;
    /// - [`CoreError::Uninitialized`] when the selected workspace has no
    ///   registered creative root, or that root no longer exists;
    /// - [`CoreError::Busy`] when this DB already has a workspace
    ///   commit/recovery authority (a second bundle must never become a
    ///   duplicate writer);
    /// - [`CoreError::Internal`] for a metadata/IO fault or an unresolvable
    ///   recovery state.
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
        manager.startup_recovery().await.map_err(map_recovery_error)?;

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
/// `WorkspaceSessionManager::new_recoverable`'s only fallible step is taking
/// the exclusive workspace-authority lease beside the DB, so its failure is a
/// WRITER conflict: this DB already has a workspace commit/recovery authority.
/// Refusing as [`CoreError::Busy`] keeps that refusal typed instead of turning
/// it into a storage fault; the lease's own diagnostic is logged.
fn map_authority_error(err: SessionError) -> CoreError {
    tracing::error!(error = %err, "workspace commit/recovery authority unavailable");
    CoreError::Busy
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
            let foreign = CoreWorkspaceCommitRequest {
                changes: vec![CoreWorkspaceCommitRequestChangesItem {
                    content_base64: Some(b64(b"escape")),
                    expected_hash: None,
                    op: CoreWorkspaceCommitRequestChangesItemOp::Create,
                    path: "escape.txt".parse().expect("non-empty path"),
                }],
                session_id: foreign_session
                    .to_string()
                    .parse()
                    .expect("non-empty session id"),
            };
            assert!(
                matches!(authority.commit(foreign).await, Err(CoreError::Busy)),
                "a foreign workspace root must be refused"
            );
            assert!(!fx.creative_root.join("escape.txt").exists());
            assert!(!other_root.join("escape.txt").exists());

            // ── An interrupted commit: the file apply lands, the intent does
            // not settle (the crash seam stands in for a process death). The
            // crash point is per applied entry, so this commit carries exactly
            // one change — recovery then completes the whole applied set. ──
            let session = manager
                .open_session(&canonical_root, "notes", true)
                .await
                .expect("recovery session");
            recovered_session = session.clone();
            test_hooks::set_crash_point(Some("after_file_apply"));
            let crashed = manager
                .commit_session_durable(
                    &session,
                    &[create_entry("recovered.txt", PAYLOAD)],
                    &canonical_root,
                )
                .await;
            test_hooks::set_crash_point(None);
            match crashed {
                Err(SessionError::Internal(message)) => {
                    assert!(
                        message.contains("test_crash"),
                        "the armed crash point must be what interrupted the apply, got: {message}"
                    );
                }
                other => panic!("the armed crash point must interrupt the apply: {other:?}"),
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
}
