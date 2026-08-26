//! V1.176 P1 (RN-2, AR-95 #3/#6) — live-daemon catalog-refresh journey.
//!
//! A daemon router built through the same boot wiring as `run_daemon`
//! (`api::create_router` over a `WorkspaceState` whose shared
//! `CapabilityRegistryHolder` is set) PLUS the REAL hot-reload watcher
//! (`boot::spawn_user_capability_watcher`, AR-91/AR-92) over a temp
//! capabilities dir. Proves the author-observable SLA on BOTH catalog
//! routes (single spine, AR-92 #8 — no second surface to curl):
//!
//! 1. Adding a complete `<name>/` trio appears on
//!    `GET /v1/daemon/tools` **and**
//!    `GET /v1/daemon/orchestration/capabilities` within the bounded
//!    interval (deadline > 2 × 1 s watch, AR-95 #7);
//! 2. Deleting the dir drops the row from both routes within the same
//!    bound, and the registry no longer resolves the name — the
//!    spine-refusal basis for a fresh dispatch (AR-94 #1);
//! 3. The watcher exits cleanly on the daemon shutdown notify.
//!
//! The removal/last-good merge rule itself is unit-tested in
//! `crates/nexus-orchestration` (`capability::watch`, AR-95 #2/#5); this
//! file pins the end-of-wire journey the author actually observes.
//!
//! Runtime: `multi_thread` (2 workers) — the watcher is a spawned tokio
//! task that must progress alongside the axum-test mock-transport request
//! chain; on a current-thread runtime the spawned task is starved and the
//! 1 s watch tick never lands (verified during implementation).

#![allow(clippy::unwrap_used, clippy::expect_used)]
// `axum_test::TestServer` request futures are `!Send` (the in-memory
// transport keeps request state across awaits). The multi_thread test
// drives every helper on a fixed runtime with no cross-thread use, so the
// lint would be noise here.
#![allow(clippy::future_not_send)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::boot;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::{CapabilityRegistry, CapabilityRuntimeDeps};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Write an admitted `<name>/capability.json` trio at
/// `<root>/capabilities/<name>/` (AR-35 layout): a hash-consistent
/// `manifest.json` + `<module-id>.wasm` pair so the AR-43 admission gates
/// pass inside the scan (same fixture as `capability_scan_boot.rs`).
fn write_capability_dir(root: &Path, name: &str) {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let dir = root.join("capabilities").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let wasm = b"fake module bytes";
    let sha: String = {
        let mut hex = String::with_capacity(64);
        for b in Sha256::digest(wasm) {
            let _ = write!(hex, "{b:02x}");
        }
        hex
    };
    let descriptor = format!(
        r#"{{
            "name": "{name}",
            "inputSchema": "{{\"type\":\"object\"}}",
            "outputSchema": "{{\"type\":\"object\"}}",
            "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
        }}"#
    );
    std::fs::write(dir.join("capability.json"), descriptor).unwrap();
    let manifest = format!(
        r#"{{
            "module_id": "basic-combat",
            "name": "Basic Combat",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "",
            "wasm_sha256": "{sha}"
        }}"#
    );
    std::fs::write(dir.join("manifest.json"), manifest).unwrap();
    std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
}

/// Build the daemon-shaped test rig: real router + REAL watcher over
/// `scan_dir`, exactly like the boot arms (AR-92 #2/#3; engine-less arm,
/// AR-44).
#[allow(clippy::type_complexity)]
async fn rig(
    scan_dir: &Path,
) -> (
    TestTempRoot,
    TestServer,
    WorkspaceState,
    Arc<tokio::sync::Notify>,
    tokio::task::JoinHandle<()>,
) {
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
    let holder = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry));
    state.set_capability_registry(holder.clone());

    // The same spawn seam boot.rs uses; boot_mirror = the boot outcome's
    // admitted set (AR-92 #4), so a hot tick carries last-good correctly;
    // boot_digest = the boot scan's structural digest (W-B, admission-time
    // ground truth since V1.176 PR wave 2), so the first poll compares
    // against the boot state instead of absorbing it.
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let boot_digest = nexus_orchestration::capability::watch::digest_from_admitted(&outcome);
    let watcher = boot::spawn_user_capability_watcher(
        holder,
        deps,
        None,
        None,
        scan_dir.to_path_buf(),
        Arc::clone(&shutdown),
        outcome.admitted,
        boot_digest,
    );

    let app = api::create_router(state.clone(), DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("failed to create test server");
    (tmp, server, state, shutdown, watcher)
}

/// `GET /v1/daemon/tools` ids present in the response.
async fn tools_ids(server: &TestServer) -> Vec<String> {
    let resp = server.get("/v1/daemon/tools").await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect()
}

/// `GET /v1/daemon/orchestration/capabilities` names present.
async fn capability_names(server: &TestServer) -> Vec<String> {
    let resp = server.get("/v1/daemon/orchestration/capabilities").await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .filter_map(|item| item["name"].as_str().map(str::to_owned))
        .collect()
}

/// Poll `GET /v1/daemon/tools` until `id` is (or is not) present. The
/// deadline pins the AR-93 daemon-leg budget: the documented bound for
/// `capability list` (the route behind this catalog) is **~2 s** (1 s
/// watch incl. rebuild + one HTTP round trip); 3 s = 1.5 × the documented
/// bound, so a regression past it fails this journey (qc3 S-2).
async fn wait_tools(server: &TestServer, id: &str, present: bool, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let ids = tools_ids(server).await;
        if ids.contains(&id.to_owned()) == present {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Poll `GET /v1/daemon/orchestration/capabilities` until `name` is (or is
/// not) present (same AR-93-pinned bounds as [`wait_tools`]).
async fn wait_orchestration(server: &TestServer, name: &str, present: bool, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let names = capability_names(server).await;
        if names.contains(&name.to_owned()) == present {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_routes_reflect_add_and_remove_within_bounded_interval() {
    // Empty scan dir at boot → no user caps; the watcher is seeded with
    // the boot `Tree` digest (W-B) and rescans only on change (AR-91 #6).
    let tmp_root = tempfile::TempDir::new().unwrap();
    let scan_dir = tmp_root.path().join("capabilities");
    std::fs::create_dir_all(&scan_dir).unwrap();
    let (_tmp, server, state, shutdown, watcher) = rig(&scan_dir).await;

    // Baseline: neither route knows the name yet.
    let baseline_tools = tools_ids(&server).await;
    assert!(
        !baseline_tools.contains(&"journey.add".to_owned()),
        "baseline tools route must not list the not-yet-existent name"
    );
    let baseline_caps = capability_names(&server).await;
    assert!(
        !baseline_caps.contains(&"journey.add".to_owned()),
        "baseline orchestration route must not list the not-yet-existent name"
    );

    // ── Add: a complete trio appears on BOTH routes within the bound ──
    write_capability_dir(tmp_root.path(), "journey.add");
    wait_tools(
        &server,
        "journey.add",
        true,
        "tools route to include journey.add after add",
    )
    .await;
    wait_orchestration(
        &server,
        "journey.add",
        true,
        "orchestration route to include journey.add after add",
    )
    .await;

    // ── Remove: deleting the dir drops the name on BOTH routes within
    // the same bound, and the registry no longer resolves it (a fresh
    // dispatch would be refused like any unknown id, AR-94 #1) ──
    std::fs::remove_dir_all(scan_dir.join("journey.add")).unwrap();
    wait_tools(
        &server,
        "journey.add",
        false,
        "tools route to drop journey.add after directory delete",
    )
    .await;
    wait_orchestration(
        &server,
        "journey.add",
        false,
        "orchestration route to drop journey.add after directory delete",
    )
    .await;
    let registry = state.capability_registry().expect("registry set");
    assert!(
        registry.get("journey.add").is_none(),
        "removed name is not resolvable in the registry (spine refusal basis)"
    );

    // The watcher exits cleanly on the daemon shutdown notify (AR-91 #6
    // lifecycle; the peer-lane precedent).
    shutdown.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(5), watcher)
        .await
        .expect("watcher exits after the shutdown notify");
    result.expect("watcher task completes without error");
}
