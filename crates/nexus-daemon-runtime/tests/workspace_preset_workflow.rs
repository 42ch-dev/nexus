//! v1.188 P3 — production-wired preset workflow proof.
//!
//! Exercises the REAL daemon path end to end: the `workspace.open` /
//! `workspace.commit` capabilities are invoked through a capability registry
//! built from production runtime deps, so they route through
//! `DaemonWorkspaceExecutor` -> the daemon's shared `WorkspaceSessionManager`
//! -> the workspace state DB and the real workspace files. A real preset graph,
//! wired by the engine from a preset manifest, then branches on
//! `_context.workspace.committed` resolved live from
//! `DaemonWorkspaceStateProvider` over that same shared manager.
//!
//! Nothing here stubs the provider or calls a task directly.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use nexus_daemon_runtime::test_utils::create_test_workspace;
use nexus_daemon_runtime::workspace::executor::DaemonWorkspaceExecutor;
use nexus_daemon_runtime::workspace::state_provider::DaemonWorkspaceStateProvider;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::capability::{CapabilityRegistry, CapabilityRuntimeDeps};
use nexus_orchestration::engine::{GraphFlowEngine, OrchestrationEngine};
use nexus_orchestration::preset::load_preset_from_str;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::CapabilityRegistryHolder;
use serde_json::json;

/// A preset whose `check` state branches on live workspace state.
const PRESET_YAML: &str = r#"
preset:
  id: p3-workspace-proof
  version: 1
  kind: creator
  description: v1.188 P3 workspace state wiring proof
  requires_capabilities:
    - workspace.open
    - workspace.commit
  initial: open_scope
  terminal: done
states:
  - id: open_scope
    enter:
      - kind: capability
        name: workspace.open
        args:
          path: pkg
    exit_when: { kind: rule }
    next: check
  - id: check
    enter: []
    exit_when: { kind: rule }
    next:
      kind: conditional
      rules:
        - when: "_context.workspace.committed"
          target: committed_state
      default: pending_state
  - id: committed_state
    enter: []
    exit_when: { kind: rule }
    next: done
  - id: pending_state
    enter: []
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
"#;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Drive the preset until the branch target is reached, returning it.
async fn routed_state(
    engine: &GraphFlowEngine,
    registry: &Arc<CapabilityRegistry>,
    label: &str,
) -> String {
    let loaded = load_preset_from_str(PRESET_YAML, registry).expect("preset loads");
    let session = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("preset session starts");

    for _ in 0..8 {
        let current = engine
            .get_current_task_id(&session)
            .await
            .expect("cursor readable");
        if let Some(state) = current {
            if state == "committed_state" || state == "pending_state" {
                return state;
            }
            if state == "done" {
                panic!("{label}: reached terminal without a recorded branch target");
            }
        }
        engine.run_step(&session).await.expect("step succeeds");
    }
    panic!("{label}: preset never reached a branch target");
}

#[tokio::test]
#[serial_test::serial]
async fn production_preset_workflow_branches_on_live_workspace_state() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let ws_root = tmp.path().join("workspace");
    std::fs::create_dir_all(ws_root.join("pkg")).expect("workspace root");

    let state =
        WorkspaceState::new_for_testing(nexus_home, db_path, Some(ws_root.display().to_string()))
            .await;
    let mgr = state.session_manager().expect("shared workspace manager");
    let root = state.workspace_path().expect("workspace root");
    mgr.startup_recovery().await.expect("startup recovery");

    // Production wiring: the SAME shared manager backs the executor and the
    // state provider.
    let deps = CapabilityRuntimeDeps {
        pool: state.pool().cloned(),
        prompt_executor: None,
        session_cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: Some(Arc::new(DaemonWorkspaceExecutor::new(
            Arc::clone(&mgr),
            root.clone(),
        ))),
    };
    let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

    let mut engine = GraphFlowEngine::new_with_storage(
        Arc::new(SqliteSessionStorage::new(mgr.pool())),
        CapabilityRegistryHolder::with_registry(Arc::clone(&registry)),
    );
    engine.set_workspace_state_provider(Arc::new(DaemonWorkspaceStateProvider::new(
        Arc::clone(&mgr),
        root.clone(),
    )));

    // Phase 1: nothing committed for this workspace -> the branch must resolve
    // to `pending_state`, proving the state is not a constant.
    assert_eq!(
        routed_state(&engine, &registry, "pre-commit").await,
        "pending_state"
    );

    // Phase 2: commit through the PRODUCTION capability path (registry ->
    // DaemonWorkspaceExecutor -> shared manager -> DB + files).
    let opened = registry
        .get("workspace.open")
        .expect("workspace.open registered")
        .run(json!({ "path": "pkg" }))
        .await
        .expect("workspace.open runs");
    let session_id = opened["sessionId"]
        .as_str()
        .expect("sessionId present")
        .to_string();

    let committed = registry
        .get("workspace.commit")
        .expect("workspace.commit registered")
        .run(json!({
            "sessionId": session_id,
            "changes": [{
                "path": "wired.txt",
                "op": "create",
                "contentBase64": b64(b"wired-through-daemon"),
            }],
        }))
        .await
        .expect("workspace.commit runs");
    let revision = committed["revision"].as_str().expect("revision").to_string();
    assert!(revision.starts_with("rev_"), "got {revision}");

    // The bytes really landed in the workspace on disk.
    let written = ws_root.join("pkg/wired.txt");
    assert_eq!(
        std::fs::read(&written).expect("committed file on disk"),
        b"wired-through-daemon"
    );

    // The intent really landed in the workspace state DB.
    let canonical_root = std::fs::canonicalize(&ws_root)
        .expect("canonicalize")
        .display()
        .to_string();
    let intent = nexus_local_db::latest_committed_intent_for_root(&mgr.pool(), &canonical_root)
        .await
        .expect("intent lookup")
        .expect("committed intent row");
    assert_eq!(intent.revision, revision);
    assert_eq!(intent.state, nexus_local_db::IntentState::Committed);

    // Phase 3: the same preset now observes the committed state and branches
    // the other way — resolved live from the shared manager, not a fixture.
    assert_eq!(
        routed_state(&engine, &registry, "post-commit").await,
        "committed_state"
    );

    let _ = std::fs::remove_file(&written);
}
