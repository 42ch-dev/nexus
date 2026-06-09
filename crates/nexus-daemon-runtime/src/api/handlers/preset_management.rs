//! Preset management handlers (V1.20 Batch 5, T34–T37).
//!
//! Endpoints:
//! - `GET /v1/local/presets` — list presets grouped by source
//! - `POST /v1/local/presets` — scaffold user preset
//! - `POST /v1/local/presets:validate` — validate preset YAML/bundle
//! - `POST /v1/local/presets/{id}:reload` — reload preset

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_home_layout::{user_preset_base_dir, user_preset_bundle_dir};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Default maximum YAML file size for validation (1 MiB).
const VALIDATE_MAX_YAML_SIZE: usize = 1024 * 1024;

/// Default maximum YAML nesting depth for validation.
const VALIDATE_MAX_YAML_DEPTH: usize = 10;

// ─── Request / Response types ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PresetSummary {
    pub id: String,
    pub source: String, // "embedded" | "system" | "user"
    /// Declared run intents (V1.33 §5). Empty if the preset doesn't declare any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_intents: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ListPresetsGroupedResponse {
    pub embedded: Vec<PresetSummary>,
    pub system: Vec<PresetSummary>,
    pub user: Vec<PresetSummary>,
}

#[derive(Debug, Deserialize)]
pub struct ScaffoldPresetRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ScaffoldPresetResponse {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct ValidatePresetRequest {
    /// Path to the preset.yaml file to validate.
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ValidatePresetResponse {
    pub valid: bool,
    pub id: Option<String>,
    pub version: Option<u32>,
    pub state_count: Option<usize>,
    pub errors: Vec<String>,
    /// Non-fatal warnings from semantic validation (V1.32 P1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ReloadPresetResponse {
    pub id: String,
    pub reloaded: bool,
}

// ─── Template ──────────────────────────────────────────────────────────────

/// The template YAML for a new user preset.
const PRESET_INIT_TEMPLATE: &str = r#"preset:
  id: {{name}}
  version: 1
  kind: creator
  description: "Custom orchestration strategy"
  requires_capabilities: []
  run_intents: [work_init]
  initial: start
  terminal: done
states:
  - id: start
    description: "Begin the workflow"
    enter:
      - kind: capability
        name: workspace.open
        args:
          prompt_file: prompts/start.md
          vars:
            input: "{{preset.input}}"
    exit_when:
      kind: manual
    next: done
  - id: done
    terminal: true
"#;

/// Template for the default prompt file scaffolding.
const PROMPT_INIT_CONTENT: &str = r"# Start Prompt

{{input}}
";

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Validate a preset name: non-empty, single path segment, not reserved.
fn validate_preset_name(name: &str) -> Result<(), NexusApiError> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name == "_system"
    {
        return Err(NexusApiError::InvalidInput {
            field: "name".to_string(),
            reason:
                "must be a non-empty path segment without separators, not '.', '..', or '_system'"
                    .to_string(),
        });
    }
    Ok(())
}

/// Compile-time generated embedded preset IDs.
/// In the daemon-runtime, we list embedded presets via the orchestration crate.
fn list_embedded_ids() -> Vec<String> {
    nexus_orchestration::preset::list_embedded_presets()
}

/// List user preset IDs from filesystem.
fn list_user_ids(nexus_home: &std::path::Path) -> Vec<String> {
    nexus_home_layout::list_user_preset_ids(nexus_home)
}

/// List system preset IDs from filesystem.
fn list_system_ids(nexus_home: &std::path::Path) -> Vec<String> {
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let scan_result =
        nexus_orchestration::system_preset_dir::scan_system_presets(nexus_home, &caps);
    nexus_orchestration::system_preset_dir::list_system_preset_ids(&scan_result)
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /v1/local/presets` — list presets grouped by source (T34)
///
/// Replaces `GET /v1/local/orchestration/presets` with richer grouping.
pub async fn list_presets(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListPresetsGroupedResponse>, NexusApiError> {
    let nexus_home = state.nexus_home();
    let caps = state.capability_registry();

    // Build a map of preset id -> run_intents from the capability registry
    let intent_map: std::collections::HashMap<String, Vec<String>> =
        caps.map_or_else(std::collections::HashMap::new, |registry| {
            nexus_orchestration::preset::list_embedded_presets()
                .into_iter()
                .filter_map(|id| {
                    let loaded =
                        nexus_orchestration::preset::load_embedded_preset(&id, &registry).ok()?;
                    let intents = loaded
                        .manifest
                        .preset
                        .run_intents
                        .iter()
                        .map(|ri| {
                            serde_json::to_value(ri)
                                .ok()
                                .and_then(|v| v.as_str().map(String::from))
                                .unwrap_or_default()
                        })
                        .collect();
                    Some((id, intents))
                })
                .collect()
        });

    let embedded = list_embedded_ids()
        .into_iter()
        .map(|id| {
            let run_intents = intent_map.get(&id).cloned().unwrap_or_default();
            PresetSummary {
                id,
                source: "embedded".to_string(),
                run_intents,
            }
        })
        .collect();

    let system = list_system_ids(nexus_home)
        .into_iter()
        .map(|id| PresetSummary {
            id,
            source: "system".to_string(),
            run_intents: vec![],
        })
        .collect();

    let user = list_user_ids(nexus_home)
        .into_iter()
        .map(|id| PresetSummary {
            id,
            source: "user".to_string(),
            run_intents: vec![],
        })
        .collect();

    Ok(Json(ListPresetsGroupedResponse {
        embedded,
        system,
        user,
    }))
}

/// `POST /v1/local/presets` — scaffold user preset (T35)
///
/// Creates a new user preset bundle directory with template files.
pub async fn scaffold_preset(
    State(state): State<WorkspaceState>,
    Json(req): Json<ScaffoldPresetRequest>,
) -> Result<Json<ScaffoldPresetResponse>, NexusApiError> {
    info!(name = %req.name, "Scaffolding user preset");
    validate_preset_name(&req.name)?;

    let nexus_home = state.nexus_home();
    let bundle_dir = user_preset_bundle_dir(nexus_home, &req.name);

    if bundle_dir.exists() {
        return Err(NexusApiError::Conflict(format!(
            "Preset '{}' already exists at {}",
            req.name,
            bundle_dir.display()
        )));
    }

    // Create directory structure
    let prompts_dir = bundle_dir.join("prompts");
    std::fs::create_dir_all(&prompts_dir).map_err(|e| NexusApiError::Internal {
        code: "DIR_CREATE_ERROR".into(),
        message: e.to_string(),
    })?;

    // Write preset.yaml
    let preset_yaml = PRESET_INIT_TEMPLATE.replace("{{name}}", &req.name);
    std::fs::write(bundle_dir.join("preset.yaml"), preset_yaml).map_err(|e| {
        NexusApiError::Internal {
            code: "FILE_WRITE_ERROR".into(),
            message: e.to_string(),
        }
    })?;

    // Write default start prompt
    std::fs::write(prompts_dir.join("start.md"), PROMPT_INIT_CONTENT).map_err(|e| {
        NexusApiError::Internal {
            code: "FILE_WRITE_ERROR".into(),
            message: e.to_string(),
        }
    })?;

    Ok(Json(ScaffoldPresetResponse {
        id: req.name,
        path: bundle_dir.display().to_string(),
    }))
}

/// `POST /v1/local/presets:validate` — validate preset YAML/bundle (T36, V1.32 P1)
///
/// Validates a preset YAML file for structural correctness with
/// field-level detail in the error response. Routes through the shared
/// semantic validation facade (`nexus_orchestration::preset::validate_preset_semantic`)
/// so the daemon endpoint and the runtime loader reject the same defects.
///
/// R-V139P5-N3 (waived): non-CLI callers (future API, programmatic consumers)
/// must route preset.input through this endpoint or the schedule enqueue path
/// (which runs `evaluate_gates`). Both paths validate; no unvalidated preset
/// input reaches execution.
///
/// Asset-path checks (template file existence) run when the path points to a
/// directory (bundle mode); otherwise only in-memory semantic checks run.
pub async fn validate_preset(
    Json(req): Json<ValidatePresetRequest>,
) -> Result<Json<ValidatePresetResponse>, NexusApiError> {
    info!(path = %req.path, "Validating preset");
    let file_path = std::path::Path::new(&req.path);

    if !file_path.exists() {
        return Err(NexusApiError::NotFound(format!(
            "File not found: {}",
            file_path.display()
        )));
    }

    // Check file size via metadata BEFORE reading
    let metadata = std::fs::metadata(file_path).map_err(|e| NexusApiError::Internal {
        code: "METADATA_ERROR".into(),
        message: e.to_string(),
    })?;
    if metadata.len() > VALIDATE_MAX_YAML_SIZE as u64 {
        return Ok(ValidatePresetResponse::invalid(&[format!(
            "Preset YAML exceeds maximum size ({} bytes, limit is {} bytes)",
            metadata.len(),
            VALIDATE_MAX_YAML_SIZE
        )]));
    }

    let yaml = std::fs::read_to_string(file_path).map_err(|e| NexusApiError::Internal {
        code: "FILE_READ_ERROR".into(),
        message: e.to_string(),
    })?;

    // Defense-in-depth size check
    if yaml.len() > VALIDATE_MAX_YAML_SIZE {
        return Ok(ValidatePresetResponse::invalid(&[format!(
            "Preset YAML exceeds maximum size ({} bytes, limit is {} bytes)",
            yaml.len(),
            VALIDATE_MAX_YAML_SIZE
        )]));
    }

    // Parse + depth check
    let manifest = parse_and_check_manifest(&yaml)?;

    // C2: Run loader-equivalent structural validation so the daemon endpoint
    //     rejects the same defects the runtime loader would reject.
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let structural_problems =
        nexus_orchestration::preset::loader_validate_manifest_compat(&manifest, &caps);
    if !structural_problems.is_empty() {
        let errors: Vec<String> = structural_problems
            .iter()
            .map(|p| format!("{}: {}", p.path, p.error))
            .collect();
        return Ok(Json(ValidatePresetResponse {
            valid: false,
            id: Some(manifest.preset.id.clone()),
            version: Some(manifest.preset.version),
            state_count: Some(manifest.states.len()),
            errors,
            warnings: Vec::new(),
        }));
    }

    // C3: Shared path-safety check (same `assert_template_file_safe` the loader uses).
    let path_result = nexus_orchestration::preset::validate_path_safety(&manifest);

    // A5: Run shared semantic validation (the same surface used by the loader).
    let sem_result = nexus_orchestration::preset::validate_preset_semantic(&manifest, &caps);

    // A3: If the path points into a bundle directory, also run asset checks.
    let asset_result = infer_bundle_root(file_path).map_or_else(
        nexus_orchestration::preset::ValidationResult::default,
        |bundle_root| {
            nexus_orchestration::preset::validate_assets_in_bundle(&manifest, &bundle_root)
        },
    );

    // Combine diagnostics from path safety + semantic + asset checks
    let mut errors: Vec<String> = Vec::new();
    for d in path_result
        .diagnostics
        .iter()
        .chain(sem_result.diagnostics.iter())
        .chain(asset_result.diagnostics.iter())
        .filter(|d| d.severity == nexus_orchestration::preset::DiagnosticSeverity::Error)
    {
        // Sanitize: use only the relative path, not full host FS path
        errors.push(format!("{}: {}", d.path, d.message));
    }

    // Warnings are reported separately in the response (informational).
    let warnings: Vec<String> = path_result
        .diagnostics
        .iter()
        .chain(sem_result.diagnostics.iter())
        .chain(asset_result.diagnostics.iter())
        .filter(|d| d.severity == nexus_orchestration::preset::DiagnosticSeverity::Warning)
        .map(|d| format!("{}: {}", d.path, d.message))
        .collect();

    let valid = errors.is_empty();
    Ok(Json(ValidatePresetResponse {
        valid,
        id: Some(manifest.preset.id.clone()),
        version: Some(manifest.preset.version),
        state_count: Some(manifest.states.len()),
        errors,
        warnings,
    }))
}

/// Infer the bundle root directory from a preset.yaml path.
///
/// If `file_path` ends with `preset.yaml`, return its parent directory.
/// Otherwise return `None` (standalone YAML file, no bundle).
fn infer_bundle_root(file_path: &std::path::Path) -> Option<std::path::PathBuf> {
    if file_path.file_name().is_some_and(|f| f == "preset.yaml") {
        file_path.parent().map(std::path::Path::to_path_buf)
    } else {
        None
    }
}

/// Parse YAML text into a `PresetManifest`, checking depth and structure.
fn parse_and_check_manifest(
    yaml: &str,
) -> Result<nexus_contracts::local::orchestration::preset::PresetManifest, NexusApiError> {
    let yaml_value: serde_yaml::Value = match serde_yaml::from_str(yaml) {
        Ok(v) => v,
        Err(e) => {
            return Err(NexusApiError::Internal {
                code: "YAML_PARSE_ERROR".into(),
                message: format!("YAML parse error: {e}"),
            });
        }
    };

    let depth = nexus_orchestration::preset::yaml_value_depth(&yaml_value);
    if depth > VALIDATE_MAX_YAML_DEPTH {
        return Err(NexusApiError::Internal {
            code: "DEPTH_EXCEEDED".into(),
            message: format!("Nesting depth ({depth}) exceeds maximum ({VALIDATE_MAX_YAML_DEPTH})"),
        });
    }

    serde_yaml::from_value(yaml_value).map_err(|e| NexusApiError::Internal {
        code: "STRUCTURAL_ERROR".into(),
        message: format!("Structural validation error: {e}"),
    })
}

impl ValidatePresetResponse {
    /// Build an invalid response with the given errors and no manifest data.
    fn invalid(errors: &[String]) -> Json<Self> {
        Json(Self {
            valid: false,
            id: None,
            version: None,
            state_count: None,
            errors: errors.to_vec(),
            warnings: Vec::new(),
        })
    }
}

/// `POST /v1/local/presets/{id}:reload` — reload preset (T37)
///
/// Reloads a user or system preset. For embedded presets, refreshes
/// the cached source hash.
pub async fn reload_preset(
    State(state): State<WorkspaceState>,
    Path(preset_id): Path<String>,
) -> Result<Json<ReloadPresetResponse>, NexusApiError> {
    info!(preset_id = %preset_id, "Reloading preset");

    // Try loading from embedded/system first
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset(&preset_id, &caps);

    if let Ok(_preset) = loaded {
        return Ok(Json(ReloadPresetResponse {
            id: preset_id,
            reloaded: true,
        }));
    }

    // Try user preset
    let nexus_home = state.nexus_home();
    let bundle_dir = user_preset_bundle_dir(nexus_home, &preset_id);
    if !bundle_dir.join("preset.yaml").exists() {
        // Try system preset
        let system_dir = user_preset_base_dir(nexus_home)
            .join("_system")
            .join(&preset_id);
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

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_preset_name_rejects_empty() {
        assert!(validate_preset_name("").is_err());
    }

    #[test]
    fn validate_preset_name_rejects_slash() {
        assert!(validate_preset_name("foo/bar").is_err());
    }

    #[test]
    fn validate_preset_name_rejects_system() {
        assert!(validate_preset_name("_system").is_err());
    }

    #[test]
    fn validate_preset_name_accepts_valid() {
        assert!(validate_preset_name("my-strategy").is_ok());
    }

    #[tokio::test]
    async fn scaffold_creates_bundle() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let nexus_home = tmp.path().to_path_buf();
        let db_path = nexus_home.join("state.db");
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("migrate");
        nexus_local_db::seed_versions(&pool).await.expect("seed");

        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None)
                .await;

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
        let nexus_home = tmp.path().to_path_buf();
        let db_path = nexus_home.join("state.db");
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("migrate");
        nexus_local_db::seed_versions(&pool).await.expect("seed");

        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None)
                .await;

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
            r#"preset:
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
"#,
        )
        .expect("write");

        let req = ValidatePresetRequest {
            path: yaml_path.to_str().expect("path").to_string(),
        };
        let result = validate_preset(Json(req)).await;
        assert!(result.is_ok());
        let resp = result.expect("ok");
        assert!(
            resp.valid,
            "expected valid: errors={:?}, warnings={:?}",
            resp.errors, resp.warnings
        );
        assert_eq!(resp.id.as_deref(), Some("test"));
    }

    fn bundle_dir_exists(nexus_home: &std::path::Path, name: &str) -> bool {
        user_preset_bundle_dir(nexus_home, name)
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

    /// Helper: call validate_preset on a bundle and return the response.
    async fn validate_bundle(bundle_dir: &std::path::Path) -> ValidatePresetResponse {
        let yaml_path = bundle_dir.join("preset.yaml");
        let req = ValidatePresetRequest {
            path: yaml_path.to_str().expect("path").to_string(),
        };
        validate_preset(Json(req)).await.expect("ok").0
    }

    // ── W1: Invalid P1 parity fixtures ──────────────────────────────────

    #[tokio::test]
    async fn w1_reject_unreachable_terminal() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r#"preset:
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
"#;
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
        let yaml = r#"preset:
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
"#;
        let bundle = create_bundle(&tmp, "mismatch", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp.errors.iter().any(|e| e.contains("terminal")));
    }

    #[tokio::test]
    async fn w1_reject_id_directory_mismatch() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r#"preset:
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
"#;
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
        let yaml = r#"preset:
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
"#;
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
        let yaml = r#"preset:
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
"#;
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
        let yaml = r#"preset:
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
"#;
        let bundle = create_bundle(&tmp, "missing-file", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(!resp.valid, "should be invalid: {:?}", resp.errors);
        assert!(resp.errors.iter().any(|e| e.contains("does not exist")));
    }

    #[tokio::test]
    async fn w1_reject_capability_drift() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let yaml = r#"preset:
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
"#;
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

        let yaml = r#"preset:
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
"#;
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
        let yaml = r#"preset:
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
"#;
        let bundle = create_bundle(&tmp, "my-preset", yaml);
        let resp = validate_bundle(&bundle).await;
        assert!(
            resp.valid,
            "id matches dirname: errors={:?}, warnings={:?}",
            resp.errors, resp.warnings
        );
    }
}
