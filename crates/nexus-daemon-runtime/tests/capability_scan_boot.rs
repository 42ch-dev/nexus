//! V1.172 P0 T3 — daemon boot registration + discovery of user capabilities
//! (AR-35/36/44).
//!
//! Proves the boot-path contract end to end through the same registry
//! constructors the daemon boot arms call (`with_runtime_deps_and_user_caps` /
//! `with_runtime_deps_and_wasm_and_user_caps`):
//!
//! 1. A user capability installed at `~/.nexus42/capabilities/<name>/`
//!    appears on `GET /v1/daemon/orchestration/capabilities` with its
//!    declared name + schemas (AC-V172-1 discover half).
//! 2. Invoking it via `CapabilityRegistry::get(name).run()` yields the named
//!    `CapabilityError::WorkerUnavailable` stub (AR-44 — no new variant).
//! 3. A bad/missing capabilities dir never fails registration (skip-and-log,
//!    AC-V172-2): the registry still builds and serves builtins.
//!
//! The boot site's raw-home scan-dir resolution is covered by the unit test
//! `user_capabilities_scan_dir_uses_raw_home` in `boot.rs` (the smallest
//! honest seam — the T2 constructor tests already prove scan+append).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::{CapabilityError, CapabilityRegistry, CapabilityRuntimeDeps};
use serde_json::Value;
use std::path::Path;

const VALID_SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Write a valid `<name>/capability.json` trio at
/// `<root>/capabilities/<name>/` (AR-35 layout). `manifest.json` +
/// `<module-id>.wasm` are not scanned at P0 (AR-35) but are written to mirror
/// the real install layout.
fn write_capability_dir(root: &Path, name: &str) {
    let dir = root.join("capabilities").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let descriptor = format!(
        r#"{{
            "name": "{name}",
            "inputSchema": "{{\"type\":\"object\"}}",
            "outputSchema": "{{\"type\":\"object\"}}",
            "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }}
        }}"#
    );
    std::fs::write(dir.join("capability.json"), descriptor).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        r#"{ "module_id": "basic-combat" }"#,
    )
    .unwrap();
    std::fs::write(dir.join("basic-combat.wasm"), b"\0asm").unwrap();
}

/// Build a daemon router whose capability registry is constructed with the
/// engine-less boot arm `with_runtime_deps_and_user_caps` scanning `scan_dir`
/// (AR-36 engine-less arm + AR-44 fallback registration). Mirrors boot.rs:
/// raw user home → `nexus_home_layout::user_capabilities_dir` is the boot
/// site's job (unit-tested separately); here the temp dir stands in for that
/// resolved path.
async fn server_with_scan(scan_dir: &Path) -> (TestTempRoot, TestServer, WorkspaceState) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: None,
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, scan_dir);
    assert!(
        outcome.skipped.is_empty(),
        "no skips expected: {:?}",
        outcome.skipped
    );
    state.set_capability_registry(std::sync::Arc::new(registry));

    let app = api::create_router(state.clone(), DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("failed to create test server");
    (tmp, server, state)
}

#[tokio::test]
async fn user_capability_appears_on_list_and_stub_runs_worker_unavailable() {
    let tmp_root = tempfile::TempDir::new().unwrap();
    write_capability_dir(tmp_root.path(), "demo.pull");
    // The scan dir mirrors the boot site's resolved
    // `~/.nexus42/capabilities/` (AR-35 layout).
    let scan_dir = tmp_root.path().join("capabilities");

    let (_tmp, server, state) = server_with_scan(&scan_dir).await;

    // Discovery: the declared name + schemas appear on the existing list
    // endpoint (AC-V172-1 discover half / AR-36 append-after-builtins).
    let resp = server.get("/v1/daemon/orchestration/capabilities").await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().expect("items array");
    let user = items
        .iter()
        .find(|cap| cap["name"] == "demo.pull")
        .unwrap_or_else(|| panic!("demo.pull not in capabilities list: {body}"));
    assert_eq!(user["inputSchema"], "{\"type\":\"object\"}");
    assert_eq!(user["outputSchema"], "{\"type\":\"object\"}");

    // Invocation stub: `run()` yields the named `WorkerUnavailable` unit
    // variant (AR-44; no empty success, no hang).
    let registry = state.capability_registry().expect("registry set");
    let cap = registry.get("demo.pull").expect("user capability indexed");
    let err = cap.run(serde_json::json!({})).await.unwrap_err();
    assert!(
        matches!(err, CapabilityError::WorkerUnavailable),
        "stub run: expected WorkerUnavailable, got {err:?}"
    );
}

#[tokio::test]
async fn boot_never_fails_on_bad_or_missing_capabilities_dir() {
    // Missing dir: the scan returns an empty outcome (AR-35 missing-dir
    // contract); the registry still builds and the endpoint serves builtins.
    let tmp_root = tempfile::TempDir::new().unwrap();
    let missing = tmp_root.path().join("does-not-exist");
    let (_tmp, server, state) = server_with_scan(&missing).await;

    let resp = server.get("/v1/daemon/orchestration/capabilities").await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    assert!(
        !body["items"].as_array().expect("items array").is_empty(),
        "builtins must still be served: {body}"
    );
    let registry = state.capability_registry().expect("registry set");
    assert!(registry.get("narrative.compute").is_some());

    // Bad descriptor dir: per-entry skip, never a registration failure
    // (AC-V172-2 skip-and-log).
    let bad_root = tempfile::TempDir::new().unwrap();
    let bad_dir = bad_root.path().join("capabilities").join("broken.cap");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("capability.json"), "{ not json").unwrap();

    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: None,
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(
        &deps,
        &bad_root.path().join("capabilities"),
    );
    assert_eq!(outcome.skipped.len(), 1, "one skip with named reason");
    assert_eq!(outcome.skipped[0].name, "broken.cap");
    assert!(
        outcome.skipped[0]
            .reason
            .contains("invalid capability.json"),
        "named reason: {:?}",
        outcome.skipped[0].reason
    );
    assert!(
        registry.get("narrative.compute").is_some(),
        "builtins intact"
    );
    assert!(registry.get("broken.cap").is_none(), "skipped cap absent");
}
