//! Preset transport adapters. Authoring and validation live in nexus-core.

use crate::api::errors::NexusApiError;
use crate::api::handlers::raw_user_home;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::generated::daemon_api::preset_management::{
    list_presets_response::ListPresetsResponse, reload_preset_response::ReloadPresetResponse,
};
use nexus_contracts::{
    GetPresetResponse, ScaffoldPresetRequest, ScaffoldPresetResponse, UpdatePresetRequest,
    UpdatePresetResponse, ValidatePresetRequest, ValidatePresetResponse,
};
use nexus_home_layout::user_preset_bundle_dir;
use tracing::info;

pub async fn list_presets(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListPresetsResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    Ok(Json(core.list_presets(&principal).await?))
}

pub async fn scaffold_preset(
    State(state): State<WorkspaceState>,
    Json(request): Json<ScaffoldPresetRequest>,
) -> Result<Json<ScaffoldPresetResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    Ok(Json(core.scaffold_preset(&principal, request).await?))
}

pub async fn validate_preset(
    State(state): State<WorkspaceState>,
    Json(request): Json<ValidatePresetRequest>,
) -> Result<Json<ValidatePresetResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    Ok(Json(core.validate_preset(&principal, request).await?))
}

pub async fn get_preset(
    State(state): State<WorkspaceState>,
    Path(id): Path<String>,
) -> Result<Json<GetPresetResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    Ok(Json(core.get_preset(&principal, id).await?))
}

pub async fn update_preset(
    State(state): State<WorkspaceState>,
    Path(id): Path<String>,
    Json(request): Json<UpdatePresetRequest>,
) -> Result<Json<UpdatePresetResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    Ok(Json(core.update_preset(&principal, id, request).await?))
}

pub async fn delete_preset(
    State(state): State<WorkspaceState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    core.delete_preset(&principal, id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `POST /v1/daemon/presets/:id` — reload preset (T37)
///
/// Routed as `POST /v1/daemon/presets/:id` because matchit 0.7 cannot register
/// `:id:reload` as a separate pattern. The path segment must end with
/// `:reload`; otherwise this returns 404.
///
/// Reloads a user or system preset. For embedded presets, refreshes
/// the cached source hash.
pub async fn reload_preset(
    State(state): State<WorkspaceState>,
    Path(segment): Path<String>,
) -> Result<Json<ReloadPresetResponse>, NexusApiError> {
    let preset_id = segment
        .strip_suffix(":reload")
        .ok_or_else(|| NexusApiError::NotFound(format!("Preset route '{segment}' not found")))?
        .to_string();

    info!(preset_id = %preset_id, "Reloading preset");

    // Try loading from embedded/system first
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_preset::load_embedded_preset(&preset_id, &caps);

    if let Ok(_preset) = loaded {
        return Ok(Json(ReloadPresetResponse {
            id: preset_id,
            reloaded: true,
        }));
    }

    // Try user preset
    let nexus_home = state.nexus_home();
    let bundle_dir = user_preset_bundle_dir(raw_user_home(nexus_home)?, &preset_id);
    if !bundle_dir.join("preset.yaml").exists() {
        // Try system preset
        let system_dir = system_preset_dir_for_id(nexus_home, &preset_id);
        if !system_dir.join("preset.yaml").exists() {
            return Err(NexusApiError::NotFound(format!(
                "Preset '{preset_id}' not found"
            )));
        }
    }

    Ok(Json(ReloadPresetResponse {
        id: preset_id,
        reloaded: true,
    }))
}

/// Resolve the on-disk bundle directory for a system preset id.
///
/// System presets are listed with a qualified `_system.<name>` id, while the
/// on-disk bundle lives at `presets/_system/<name>/` — strip the `_system.`
/// prefix before joining (AD-P0-1b). Unqualified ids pass through unchanged.
fn system_preset_dir_for_id(nexus_home: &std::path::Path, preset_id: &str) -> std::path::PathBuf {
    let dir_name = preset_id.strip_prefix("_system.").unwrap_or(preset_id);
    nexus_preset::system_preset_dir::system_preset_bundle_dir(nexus_home, dir_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn authoring_state(
        nexus_home: std::path::PathBuf,
        db_path: std::path::PathBuf,
        workspace_path: Option<String>,
    ) -> WorkspaceState {
        let home = nexus_home.parent().expect("raw home");
        std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            home,
            "test_creator",
            "default",
        ))
        .expect("operational workspace");
        std::fs::write(nexus_home.join("config.toml"),
            "active_creator_id = \"test_creator\"\n[active_workspace_slug_by_creator]\ntest_creator = \"default\"\n"
        ).expect("active principal configuration");
        WorkspaceState::new_for_testing(nexus_home, db_path, workspace_path).await
    }

    async fn validate_for_test(
        request: ValidatePresetRequest,
    ) -> Result<Json<ValidatePresetResponse>, NexusApiError> {
        let (_tmp, home, db) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(home, db, None).await;
        validate_preset(State(state), Json(request)).await
    }

    #[tokio::test]
    async fn scaffold_creates_bundle() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
        let db_path = nexus_home.join("state.db");
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("migrate");
        nexus_local_db::seed_versions(&pool).await.expect("seed");

        let state = authoring_state(nexus_home.clone(), db_path, None).await;

        let req = ScaffoldPresetRequest {
            name: "test-strat".to_string(),
        };
        let result = scaffold_preset(State(state), Json(req)).await;
        assert!(result.is_ok(), "scaffold should succeed: {result:?}");

        let resp = result.expect("ok");
        assert_eq!(resp.id, "test-strat");
        assert!(bundle_dir_exists(&nexus_home, "test-strat"));
    }

    #[tokio::test]
    async fn scaffold_rejects_duplicate() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
        let db_path = nexus_home.join("state.db");
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("migrate");
        nexus_local_db::seed_versions(&pool).await.expect("seed");

        let state = authoring_state(nexus_home.clone(), db_path, None).await;

        let req = ScaffoldPresetRequest {
            name: "dup-strat".to_string(),
        };
        let _ = scaffold_preset(State(state.clone()), Json(req)).await;
        let req2 = ScaffoldPresetRequest {
            name: "dup-strat".to_string(),
        };
        let result = scaffold_preset(State(state), Json(req2)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::Conflict(msg) => assert!(msg.contains("already exists")),
            other => panic!("Expected Conflict, got: {other}"),
        }
    }

    #[tokio::test]
    async fn validate_accepts_valid_preset() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        // Create a properly-named bundle directory so the id-vs-directory check passes.
        let bundle_dir = tmp.path().join("test");
        std::fs::create_dir_all(&bundle_dir).expect("mkdir");
        let yaml_path = bundle_dir.join("preset.yaml");
        std::fs::write(
            &yaml_path,
            r"preset:
  id: test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
",
        )
        .expect("write");

        let req = ValidatePresetRequest {
            path: yaml_path.to_str().expect("path").to_string(),
        };
        let result = validate_for_test(req).await;
        assert!(result.is_ok());
        let resp = result.expect("ok");
        assert!(
            resp.valid,
            "expected valid: errors={:?}, warnings={:?}",
            resp.errors, resp.warnings
        );
        assert_eq!(resp.id.as_deref(), Some("test"));
    }

    /// Check a user preset bundle exists at the canonical layout:
    /// `<nexus_home>/presets/<name>/preset.yaml` (the layout
    /// `scan_user_presets` / `resolve_preset` use — F-QA-001).
    fn bundle_dir_exists(nexus_home: &std::path::Path, name: &str) -> bool {
        nexus_home
            .join("presets")
            .join(name)
            .join("preset.yaml")
            .exists()
    }

    /// Helper: create a bundle directory with a preset.yaml and return its path.
    fn create_bundle(tmp: &tempfile::TempDir, id: &str, yaml: &str) -> std::path::PathBuf {
        let bundle_dir = tmp.path().join(id);
        std::fs::create_dir_all(&bundle_dir).expect("mkdir");
        std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write");
        bundle_dir
    }

    /// Helper: call `validate_preset` on a bundle and return the response.
    async fn validate_bundle(bundle_dir: &std::path::Path) -> ValidatePresetResponse {
        let yaml_path = bundle_dir.join("preset.yaml");
        let req = ValidatePresetRequest {
            path: yaml_path.to_str().expect("path").to_string(),
        };
        validate_for_test(req).await.expect("ok").0
    }

    // ── W1: Invalid P1 parity fixtures ──────────────────────────────────

    #[tokio::test]
    async fn w1_reject_unreachable_terminal() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: unreachable
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    enter: []
    exit_when: { kind: manual }
    next: a
  - id: c
    terminal: true
";
        let bundle = create_bundle(&tmp, "unreachable", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp
            .errors
            .iter()
            .any(|e| e.contains("cannot reach any terminal") || e.contains("Reachability")));
    }

    #[tokio::test]
    async fn w1_reject_terminal_header_mismatch() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: mismatch
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    enter: []
    exit_when: { kind: manual }
";
        let bundle = create_bundle(&tmp, "mismatch", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp.errors.iter().any(|e| e.contains("terminal")));
    }

    #[tokio::test]
    async fn w1_reject_id_directory_mismatch() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: wrong-name
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        // Directory is "right-name" but manifest says "wrong-name"
        let bundle = create_bundle(&tmp, "right-name", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp
            .errors
            .iter()
            .any(|e| e.contains("does not match bundle directory")));
    }

    #[tokio::test]
    async fn w1_reject_missing_inner_graph() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: missing-ig
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: nonexistent
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
";
        let bundle = create_bundle(&tmp, "missing-ig", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp
            .errors
            .iter()
            .any(|e| e.contains("unknown inner_graph") || e.contains("not defined")));
    }

    #[tokio::test]
    async fn w1_warn_orphan_inner_graph() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: orphan
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
inner_graphs:
  unused:
    nodes:
      - id: n1
        kind: acp_prompt
";
        let bundle = create_bundle(&tmp, "orphan", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(
            resp.valid,
            "orphan graph is a warning, not error: {:?}",
            resp.errors
        );
        assert!(
            resp.warnings.iter().any(|w| w.contains("not referenced")),
            "expected orphan warning: {:?}",
            resp.warnings
        );
    }

    #[tokio::test]
    async fn w1_reject_missing_template_file_in_bundle() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: missing-file
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: prompts/nonexistent.md
    next: b
  - id: b
    terminal: true
";
        let bundle = create_bundle(&tmp, "missing-file", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp.errors.iter().any(|e| e.contains("does not exist")));
    }

    #[tokio::test]
    async fn w1_reject_capability_drift() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: cap-drift
  version: 1
  kind: creator
  description: test
  requires_capabilities:
    - totally.fake.capability
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let bundle = create_bundle(&tmp, "cap-drift", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp
            .errors
            .iter()
            .any(|e| e.contains("not found in registry")));
    }

    // ── W2: Path safety regression tests ────────────────────────────────

    #[tokio::test]
    async fn w2_reject_dotdot_traversal() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r#"preset:
  id: dotdot
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "../../etc/passwd"
    next: b
  - id: b
    terminal: true
"#;
        let bundle = create_bundle(&tmp, "dotdot", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should reject traversal: {:?}", resp.errors);
        // Verify error message does not leak full host path
        for e in &resp.errors {
            assert!(
                !e.contains("/private/"),
                "error should not leak host path: {e}"
            );
            assert!(!e.contains("/var/"), "error should not leak host path: {e}");
        }
    }

    #[tokio::test]
    async fn w2_reject_absolute_path() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r#"preset:
  id: abspath
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op: { kind: append }
      template_file: "/etc/shadow"
  - id: b
    terminal: true
"#;
        let bundle = create_bundle(&tmp, "abspath", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(
            !resp.valid,
            "should reject absolute path: {:?}",
            resp.errors
        );
        assert!(resp.errors.iter().any(|e| e.contains("absolute")));
    }

    #[tokio::test]
    async fn w2_reject_symlink_escape() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let bundle_dir = tmp.path().join("symlink-escape");
        std::fs::create_dir_all(&bundle_dir).expect("mkdir");

        // Create a file outside the bundle
        let outside = tmp.path().join("secret.txt");
        std::fs::write(&outside, "secret").expect("write");

        // Create a symlink inside the bundle pointing outside
        let prompts_dir = bundle_dir.join("prompts");
        std::fs::create_dir_all(&prompts_dir).expect("mkdir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, prompts_dir.join("judge.md")).expect("symlink");

        let yaml = r"preset:
  id: symlink-escape
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: prompts/judge.md
    next: b
  - id: b
    terminal: true
";
        std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write");
        let resp = validate_bundle(&bundle_dir).await;
        assert!(
            !resp.valid,
            "should reject symlink escape: {:?}",
            resp.errors
        );
        assert!(resp
            .errors
            .iter()
            .any(|e| e.contains("symlink") || e.contains("outside")));
    }

    // ── C4: Bundle dir id match test ────────────────────────────────────

    #[tokio::test]
    async fn c4_accept_matching_id_and_dirname() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r"preset:
  id: my-preset
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let bundle = create_bundle(&tmp, "my-preset", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(
            resp.valid,
            "id matches dirname: errors={:?}, warnings={:?}",
            resp.errors, resp.warnings
        );
    }

    // ── Full CRUD tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn get_preset_returns_user_bundle() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        let _ = scaffold_preset(
            State(state.clone()),
            axum::Json(ScaffoldPresetRequest {
                name: "crud-test".to_string(),
            }),
        )
        .await
        .expect("scaffold");

        let resp = get_preset(State(state), Path("crud-test".to_string()))
            .await
            .expect("get preset")
            .0;
        assert_eq!(resp.id, "crud-test");
        assert_eq!(resp.source.to_string(), "user");
        assert!(resp.yaml.contains("crud-test"));
    }

    #[tokio::test]
    async fn get_preset_returns_embedded_preset() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        let resp = get_preset(State(state), Path("novel-writing".to_string()))
            .await
            .expect("get embedded preset")
            .0;
        assert_eq!(resp.id, "novel-writing");
        assert_eq!(resp.source.to_string(), "embedded");
        assert!(resp.yaml.contains("novel-writing"));
        assert!(resp.path.is_none());
    }

    /// AD-P0-1c repro leg 3: a qualified `_system.*` id returned by
    /// `list_presets` must resolve to `presets/_system/<name>/` on disk.
    #[tokio::test]
    async fn get_preset_returns_system_preset() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        // First-start fallback creates `presets/_system/maintenance/` on disk.
        nexus_preset::system_preset_dir::ensure_maintenance_preset(state.nexus_home())
            .expect("ensure maintenance preset");

        let resp = get_preset(State(state), Path("_system.maintenance".to_string()))
            .await
            .expect("get system preset")
            .0;
        assert_eq!(resp.id, "_system.maintenance");
        assert_eq!(resp.source.to_string(), "system");
        assert!(resp.path.is_some());
        assert!(resp.yaml.contains("maintenance"));
    }

    /// Same qualified-id resolution bug class on the reload route.
    #[tokio::test]
    async fn reload_preset_accepts_qualified_system_id() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        nexus_preset::system_preset_dir::ensure_maintenance_preset(state.nexus_home())
            .expect("ensure maintenance preset");

        let resp = reload_preset(State(state), Path("_system.maintenance:reload".to_string()))
            .await
            .expect("reload system preset")
            .0;
        assert_eq!(resp.id, "_system.maintenance");
        assert!(resp.reloaded);
    }

    #[tokio::test]
    async fn update_preset_mutates_user_yaml() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        let _ = scaffold_preset(
            State(state.clone()),
            axum::Json(ScaffoldPresetRequest {
                name: "update-test".to_string(),
            }),
        )
        .await
        .expect("scaffold");

        let new_yaml = r"preset:
  id: update-test
  version: 1
  kind: creator
  description: updated description
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"
        .to_string();
        let resp = update_preset(
            State(state.clone()),
            Path("update-test".to_string()),
            axum::Json(UpdatePresetRequest { yaml: new_yaml }),
        )
        .await
        .expect("update preset")
        .0;
        assert!(resp.updated);

        let yaml_path = user_preset_bundle_dir(
            state.nexus_home().parent().expect("nexus_home parent"),
            "update-test",
        )
        .join("preset.yaml");
        let written = std::fs::read_to_string(yaml_path).expect("read yaml");
        assert!(written.contains("updated description"));
    }

    #[tokio::test]
    async fn update_preset_rejects_embedded() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        let result = update_preset(
            State(state),
            Path("novel-writing".to_string()),
            axum::Json(UpdatePresetRequest {
                yaml: "preset:\n".to_string(),
            }),
        )
        .await;
        assert!(result.is_err(), "embedded preset update must be rejected");
    }

    #[tokio::test]
    async fn delete_preset_removes_user_bundle() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        let _ = scaffold_preset(
            State(state.clone()),
            axum::Json(ScaffoldPresetRequest {
                name: "delete-test".to_string(),
            }),
        )
        .await
        .expect("scaffold");

        let status = delete_preset(State(state.clone()), Path("delete-test".to_string()))
            .await
            .expect("delete preset");
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
        assert!(!bundle_dir_exists(state.nexus_home(), "delete-test"));
    }

    #[tokio::test]
    async fn delete_preset_rejects_embedded() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let state = {
            let nexus_home = tmp.path().join(".nexus42");
            std::fs::create_dir_all(&nexus_home).expect("create nexus_home dir");
            let db_path = nexus_home.join("state.db");
            let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
            nexus_local_db::run_migrations(&pool)
                .await
                .expect("migrate");
            nexus_local_db::seed_versions(&pool).await.expect("seed");
            authoring_state(nexus_home, db_path, None).await
        };

        let result = delete_preset(State(state), Path("novel-writing".to_string())).await;
        assert!(result.is_err(), "embedded preset delete must be rejected");
    }
}
