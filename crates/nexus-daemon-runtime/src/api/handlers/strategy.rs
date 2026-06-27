//! Strategy canvas write-boundary handlers (V1.71 Track A).
//!
//! Three structured patch routes for the Strategy (preset) graph surface:
//! - `POST /v1/local/strategies/{strategy_id}/states/{state_id}/patch`
//! - `POST /v1/local/strategies/{strategy_id}/transitions/patch`
//! - `POST /v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch`
//!
//! All writes target **user** preset bundles only; embedded/system presets are
//! read-only. Every mutating request carries a `base_revision`; stale revisions
//! return a structured 409 conflict before any file is touched.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::{
    StrategyPatchPromptTemplateRequest, StrategyPatchResponse, StrategyPatchStateRequest,
    StrategyPatchTransitionRequest,
};
use nexus_home_layout::{user_preset_base_dir, user_preset_bundle_dir};
use serde_json::Value;
use tracing::info;

/// Maximum YAML file size for a user preset (1 MiB).
const PRESET_MAX_YAML_SIZE: usize = 1024 * 1024;

/// Maximum YAML nesting depth for a user preset.
const PRESET_MAX_YAML_DEPTH: usize = 10;

// ─── Request helpers ───────────────────────────────────────────────────────

/// Parsed `set` payload for `patch_state`.
#[derive(Debug, Default)]
struct StatePatchSet {
    label: Option<String>,
    description: Option<String>,
}

fn parse_state_set(value: &Value) -> Result<StatePatchSet, NexusApiError> {
    let label = value
        .get("label")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    if label.is_none() && description.is_none() {
        return Err(NexusApiError::InvalidInput {
            field: "set".to_string(),
            reason: "must include 'label' and/or 'description'".to_string(),
        });
    }
    Ok(StatePatchSet { label, description })
}

// ─── Preset loading / revision read ────────────────────────────────────────

/// Locate a user preset bundle and read its YAML + current revision.
///
/// Returns the parsed YAML value, bundle directory, and current `revision`
/// (missing `revision:` reads as `0`).
fn load_user_preset_yaml(
    nexus_home: &std::path::Path,
    strategy_id: &str,
) -> Result<(serde_yaml::Value, std::path::PathBuf, u64), NexusApiError> {
    validate_strategy_id(strategy_id)?;

    let bundle_dir = user_preset_bundle_dir(nexus_home, strategy_id);
    let yaml_path = bundle_dir.join("preset.yaml");
    if !yaml_path.is_file() {
        // Reject embedded/system presets explicitly so callers get a clear
        // forbidden message rather than a generic not-found.
        if nexus_orchestration::preset::list_embedded_presets().contains(&strategy_id.to_string()) {
            return Err(NexusApiError::BadRequest {
                code: "strategy_update_forbidden".to_string(),
                message: format!("embedded preset '{strategy_id}' is read-only"),
            });
        }
        let system_path = user_preset_base_dir(nexus_home)
            .join("_system")
            .join(strategy_id)
            .join("preset.yaml");
        if system_path.is_file() {
            return Err(NexusApiError::BadRequest {
                code: "strategy_update_forbidden".to_string(),
                message: format!("system preset '{strategy_id}' is read-only"),
            });
        }
        return Err(NexusApiError::NotFound(format!(
            "Strategy '{strategy_id}' not found"
        )));
    }

    let yaml = std::fs::read_to_string(&yaml_path).map_err(|e| NexusApiError::Internal {
        code: "FILE_READ_ERROR".to_string(),
        message: e.to_string(),
    })?;

    if yaml.len() > PRESET_MAX_YAML_SIZE {
        return Err(NexusApiError::BadRequest {
            code: "strategy_yaml_too_large".to_string(),
            message: format!(
                "preset YAML exceeds maximum size ({} bytes, limit is {} bytes)",
                yaml.len(),
                PRESET_MAX_YAML_SIZE
            ),
        });
    }

    let value: serde_yaml::Value =
        serde_yaml::from_str(&yaml).map_err(|e| NexusApiError::BadRequest {
            code: "strategy_yaml_invalid".to_string(),
            message: format!("preset.yaml is not valid YAML: {e}"),
        })?;

    let depth = nexus_orchestration::preset::yaml_value_depth(&value);
    if depth > PRESET_MAX_YAML_DEPTH {
        return Err(NexusApiError::BadRequest {
            code: "strategy_yaml_too_deep".to_string(),
            message: format!(
                "preset YAML nesting depth ({depth}) exceeds maximum ({PRESET_MAX_YAML_DEPTH})"
            ),
        });
    }

    let revision = value
        .get("revision")
        .and_then(serde_yaml::Value::as_u64)
        .unwrap_or(0);

    Ok((value, bundle_dir, revision))
}

/// Validate a strategy/preset identifier (same rules as a user preset name).
fn validate_strategy_id(id: &str) -> Result<(), NexusApiError> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id == "."
        || id == ".."
        || id == "_system"
    {
        return Err(NexusApiError::InvalidInput {
            field: "strategy_id".to_string(),
            reason:
                "must be a non-empty path segment without separators, not '.', '..', or '_system'"
                    .to_string(),
        });
    }
    Ok(())
}

/// Ensure the request's repeated identifier matches the URL path.
fn ensure_id_matches(path: &str, body: &str, field: &str) -> Result<(), NexusApiError> {
    if path != body {
        return Err(NexusApiError::BadRequest {
            code: "strategy_id_mismatch".to_string(),
            message: format!("{field} in body ('{body}') does not match URL path ('{path}')"),
        });
    }
    Ok(())
}

// ─── Conflict builder ──────────────────────────────────────────────────────

fn strategy_conflict(
    current_revision: u64,
    node_id: &str,
    conflicting_path: &str,
    recovery_hint: &str,
) -> NexusApiError {
    NexusApiError::strategy_conflict(current_revision, node_id, conflicting_path, recovery_hint)
}

// ─── Patch application ─────────────────────────────────────────────────────

/// Find the index of a state by id inside a YAML sequence.
fn find_state_index(states: &[serde_yaml::Value], id: &str) -> Option<usize> {
    states.iter().position(|s| {
        s.get("id")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == id)
    })
}

/// Collect the ids of all states in a YAML sequence.
fn state_ids(states: &[serde_yaml::Value]) -> Vec<String> {
    states
        .iter()
        .filter_map(|s| s.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect()
}

/// Rename a state id and rewrite all references held in YAML values.
fn rename_state_references(
    root: &mut serde_yaml::Value,
    old_id: &str,
    new_id: &str,
) -> Result<Vec<String>, NexusApiError> {
    if new_id.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "set.label".to_string(),
            reason: "state label must be non-empty".to_string(),
        });
    }

    let mut side_effects = Vec::new();

    // Update preset.initial / preset.terminal
    if let Some(preset) = root.get_mut("preset") {
        for key in ["initial", "terminal"] {
            if preset
                .get(key)
                .and_then(|v| v.as_str())
                .is_some_and(|v| v == old_id)
            {
                preset[key] = serde_yaml::Value::String(new_id.to_string());
                side_effects.push(format!("preset.{key} updated to '{new_id}'"));
            }
        }
    }

    // Update every state's next references.
    if let Some(states) = root.get_mut("states").and_then(|v| v.as_sequence_mut()) {
        for state in states {
            // Linear next scalar.
            if let Some(next) = state.get_mut("next") {
                if next.is_string() {
                    if next.as_str().is_some_and(|v| v == old_id) {
                        *next = serde_yaml::Value::String(new_id.to_string());
                    }
                } else if let Some(next_map) = next.as_mapping_mut() {
                    // Conditional rules / labeled branches
                    if let Some(rules) = next_map.get_mut("rules").and_then(|v| v.as_sequence_mut())
                    {
                        for rule in rules {
                            if let Some(to) = rule.get_mut("to") {
                                if to.as_str().is_some_and(|v| v == old_id) {
                                    *to = serde_yaml::Value::String(new_id.to_string());
                                }
                            }
                        }
                    }
                    // Default target
                    if let Some(default) = next_map.get_mut("default") {
                        if default.as_str().is_some_and(|v| v == old_id) {
                            *default = serde_yaml::Value::String(new_id.to_string());
                        }
                    }
                    // Go/nogo branches
                    for key in ["go", "nogo"] {
                        if let Some(branch) = next_map.get_mut(key) {
                            if branch.as_str().is_some_and(|v| v == old_id) {
                                *branch = serde_yaml::Value::String(new_id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Update the renamed state's own id.
    if let Some(states) = root.get_mut("states").and_then(|v| v.as_sequence_mut()) {
        if let Some(state) = states.iter_mut().find(|s| {
            s.get("id")
                .and_then(serde_yaml::Value::as_str)
                .is_some_and(|v| v == old_id)
        }) {
            state["id"] = serde_yaml::Value::String(new_id.to_string());
        }
    }

    side_effects.push(format!("renamed state '{old_id}' -> '{new_id}'"));
    Ok(side_effects)
}

// ─── Domain validation ─────────────────────────────────────────────────────

/// Run the same preset validation the loader uses and return (errors, warnings).
fn validate_preset_yaml(
    bundle_root: &std::path::Path,
    yaml_value: &serde_yaml::Value,
) -> Result<(Vec<String>, Vec<String>), NexusApiError> {
    let manifest: nexus_contracts::local::orchestration::preset::PresetManifest =
        serde_yaml::from_value(yaml_value.clone()).map_err(|e| NexusApiError::BadRequest {
            code: "strategy_validation_failed".to_string(),
            message: format!("structural validation failed: {e}"),
        })?;

    let caps = nexus_orchestration::capability::CapabilityRegistry::with_builtins();

    let mut errors: Vec<String> =
        nexus_orchestration::preset::loader_validate_manifest_compat(&manifest, &caps)
            .iter()
            .map(|p| format!("{}: {}", p.path, p.error))
            .collect();

    let sem = nexus_orchestration::preset::validate_preset_semantic(&manifest, &caps);
    for d in sem.errors() {
        errors.push(format!("{}: {}", d.path, d.message));
    }

    let path_result = nexus_orchestration::preset::validate_path_safety(&manifest);
    let asset_result =
        nexus_orchestration::preset::validate_assets_in_bundle(&manifest, bundle_root);
    for d in path_result.errors().chain(asset_result.errors()) {
        errors.push(format!("{}: {}", d.path, d.message));
    }

    let warnings: Vec<String> = sem
        .warnings()
        .chain(path_result.warnings())
        .chain(asset_result.warnings())
        .map(|d| format!("{}: {}", d.path, d.message))
        .collect();

    Ok((errors, warnings))
}

// ─── Atomic persistence ────────────────────────────────────────────────────

/// Write the updated YAML back to `preset.yaml`, bumping the revision header.
///
/// Uses a temp file + rename + fsync for atomicity.
fn write_preset_yaml(
    bundle_root: &std::path::Path,
    value: &mut serde_yaml::Value,
    new_revision: u64,
) -> Result<(), NexusApiError> {
    if let Some(root) = value.as_mapping_mut() {
        root.insert(
            serde_yaml::Value::String("revision".to_string()),
            serde_yaml::Value::Number(new_revision.into()),
        );
    }

    let yaml_path = bundle_root.join("preset.yaml");
    let tmp_path = bundle_root.join("preset.yaml.tmp");

    let yaml_str = serde_yaml::to_string(value).map_err(|e| NexusApiError::Internal {
        code: "YAML_SERIALIZE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    std::fs::write(&tmp_path, yaml_str).map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    // fsync the temp file so the subsequent rename is durable.
    let file = std::fs::File::open(&tmp_path).map_err(|e| NexusApiError::Internal {
        code: "FILE_SYNC_ERROR".to_string(),
        message: e.to_string(),
    })?;
    file.sync_all().map_err(|e| NexusApiError::Internal {
        code: "FILE_SYNC_ERROR".to_string(),
        message: e.to_string(),
    })?;
    drop(file);

    std::fs::rename(&tmp_path, &yaml_path).map_err(|e| NexusApiError::Internal {
        code: "FILE_RENAME_ERROR".to_string(),
        message: e.to_string(),
    })?;

    Ok(())
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `POST /v1/local/strategies/{strategy_id}/states/{state_id}/patch` — patch a state.
///
/// # Errors
///
/// Returns `strategy_id_mismatch` (400), `state_id_mismatch` (400),
/// `strategy_conflict` (409), `strategy_invalid` (400), `not_found` (404),
/// `strategy_state_duplicate` (400), or `strategy_validation_failed` (422).
pub async fn patch_state(
    State(state): State<WorkspaceState>,
    Path((strategy_id, state_id)): Path<(String, String)>,
    Json(req): Json<StrategyPatchStateRequest>,
) -> Result<Json<StrategyPatchResponse>, NexusApiError> {
    info!(strategy_id = %strategy_id, state_id = %state_id, "Patching Strategy state");
    ensure_id_matches(&strategy_id, &req.strategy_id, "strategy_id")?;
    ensure_id_matches(&state_id, &req.state_id, "state_id")?;

    let nexus_home = state.nexus_home();
    let (mut yaml_value, bundle_dir, current_revision) =
        load_user_preset_yaml(nexus_home, &strategy_id)?;

    if req.base_revision != current_revision {
        return Err(strategy_conflict(
            current_revision,
            &state_id,
            "states",
            "refetch the Strategy and reapply your edit",
        ));
    }

    let set = parse_state_set(&req.set)?;

    let states = yaml_value
        .get_mut("states")
        .and_then(|v| v.as_sequence_mut())
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "strategy_invalid".to_string(),
            message: "preset.yaml is missing the 'states' array".to_string(),
        })?;

    let idx = find_state_index(states, &state_id).ok_or_else(|| {
        NexusApiError::NotFound(format!(
            "state '{state_id}' not found in Strategy '{strategy_id}'"
        ))
    })?;

    let mut side_effects: Vec<String> = Vec::new();

    if let Some(new_label) = set.label {
        let new_label = new_label.trim().to_string();
        if new_label != state_id {
            let ids = state_ids(states);
            if ids.contains(&new_label) {
                return Err(NexusApiError::BadRequest {
                    code: "strategy_state_duplicate".to_string(),
                    message: format!("state id '{new_label}' already exists"),
                });
            }
            side_effects = rename_state_references(&mut yaml_value, &state_id, &new_label)?;
        }
    }

    if let Some(description) = set.description {
        let states2 = yaml_value
            .get_mut("states")
            .and_then(|v| v.as_sequence_mut())
            .ok_or_else(|| NexusApiError::Internal {
                code: "STRATEGY_STATES_MISSING".to_string(),
                message: "states array disappeared during description update".to_string(),
            })?;
        if let Some(state_node) = states2.get_mut(idx) {
            state_node["description"] = serde_yaml::Value::String(description);
        }
    }

    let (errors, warnings) = validate_preset_yaml(&bundle_dir, &yaml_value)?;
    if !errors.is_empty() {
        return Err(NexusApiError::strategy_validation_failed(
            &errors, &warnings,
        ));
    }

    let new_revision = i64::try_from(current_revision.saturating_add(1)).unwrap_or(i64::MAX);
    write_preset_yaml(
        &bundle_dir,
        &mut yaml_value,
        current_revision.saturating_add(1),
    )?;

    Ok(Json(StrategyPatchResponse {
        new_revision,
        validation_summary: serde_json::json!({ "errors": [], "warnings": warnings }),
        side_effects: Some(side_effects),
    }))
}

/// Apply a transition patch to a `next` YAML value.
///
/// Returns `(matched, side_effects)`. If no branch matches, `matched` is false
/// so the caller can emit a `strategy_transition_not_found` error.
fn apply_transition_patch(
    next: &mut serde_yaml::Value,
    req: &StrategyPatchTransitionRequest,
) -> (bool, Vec<String>) {
    let mut matched = false;
    let mut side_effects: Vec<String> = Vec::new();

    if next.is_string() {
        if next.as_str().is_some_and(|v| v == req.old_target) {
            let new_target = req.new_target.as_deref().unwrap_or(&req.old_target);
            *next = serde_yaml::Value::String(new_target.to_string());
            matched = true;
            side_effects.push(format!(
                "transition {} -> {} set to {}",
                req.source_state_id, req.old_target, new_target
            ));
        }
    } else if let Some(next_map) = next.as_mapping_mut() {
        matched = apply_conditional_rules(next_map, req, &mut side_effects);

        if !matched {
            matched = apply_default_transition(next_map, req, &mut side_effects);
        }

        if !matched {
            matched = apply_go_nogo_branches(next_map, req, &mut side_effects);
        }
    }

    (matched, side_effects)
}

/// Match and update a conditional/labeled `rules` branch inside a transition.
fn apply_conditional_rules(
    next_map: &mut serde_yaml::Mapping,
    req: &StrategyPatchTransitionRequest,
    side_effects: &mut Vec<String>,
) -> bool {
    let mut matched = false;
    if let Some(rules) = next_map.get_mut("rules").and_then(|v| v.as_sequence_mut()) {
        for rule in rules {
            let to_match = rule
                .get("to")
                .and_then(serde_yaml::Value::as_str)
                .is_some_and(|v| v == req.old_target);
            let cond_match = req.condition.as_ref().is_none_or(|cond| {
                rule.get("when")
                    .and_then(serde_yaml::Value::as_str)
                    .is_some_and(|v| v == cond)
            });
            if to_match && cond_match {
                if let Some(new_target) = &req.new_target {
                    rule["to"] = serde_yaml::Value::String(new_target.clone());
                }
                if let Some(condition) = &req.condition {
                    rule["when"] = serde_yaml::Value::String(condition.clone());
                }
                matched = true;
                side_effects.push(format!(
                    "branch {} -> {} updated",
                    req.source_state_id, req.old_target
                ));
            }
        }
    }
    matched
}

/// Match and update the `default` target of a conditional transition.
fn apply_default_transition(
    next_map: &mut serde_yaml::Mapping,
    req: &StrategyPatchTransitionRequest,
    side_effects: &mut Vec<String>,
) -> bool {
    if let Some(default) = next_map.get_mut("default") {
        if default.as_str().is_some_and(|v| v == req.old_target) {
            if let Some(new_target) = &req.new_target {
                *default = serde_yaml::Value::String(new_target.clone());
            }
            side_effects.push(format!(
                "default transition {} -> {} updated",
                req.source_state_id, req.old_target
            ));
            return true;
        }
    }
    false
}

/// Match and update `go` / `nogo` branches of a transition.
fn apply_go_nogo_branches(
    next_map: &mut serde_yaml::Mapping,
    req: &StrategyPatchTransitionRequest,
    side_effects: &mut Vec<String>,
) -> bool {
    let mut matched = false;
    for key in ["go", "nogo"] {
        if let Some(branch) = next_map.get_mut(key) {
            if branch.as_str().is_some_and(|v| v == req.old_target) {
                if let Some(new_target) = &req.new_target {
                    *branch = serde_yaml::Value::String(new_target.clone());
                }
                matched = true;
                side_effects.push(format!("{key} branch from {} updated", req.source_state_id));
            }
        }
    }
    matched
}

/// `POST /v1/local/strategies/{strategy_id}/transitions/patch` — rewire a transition.
///
/// # Errors
///
/// Returns `strategy_id_mismatch` (400), `strategy_conflict` (409),
/// `strategy_invalid` (400), `not_found` (404), `strategy_transition_missing` (400),
/// `strategy_transition_not_found` (400), or `strategy_validation_failed` (422).
pub async fn patch_transition(
    State(state): State<WorkspaceState>,
    Path(strategy_id): Path<String>,
    Json(req): Json<StrategyPatchTransitionRequest>,
) -> Result<Json<StrategyPatchResponse>, NexusApiError> {
    info!(strategy_id = %strategy_id, source = %req.source_state_id, "Patching Strategy transition");
    ensure_id_matches(&strategy_id, &req.strategy_id, "strategy_id")?;

    let nexus_home = state.nexus_home();
    let (mut yaml_value, bundle_dir, current_revision) =
        load_user_preset_yaml(nexus_home, &strategy_id)?;

    if req.base_revision != current_revision {
        return Err(strategy_conflict(
            current_revision,
            &req.source_state_id,
            "transitions",
            "refetch the Strategy and reapply your edit",
        ));
    }

    let states = yaml_value
        .get_mut("states")
        .and_then(|v| v.as_sequence_mut())
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "strategy_invalid".to_string(),
            message: "preset.yaml is missing the 'states' array".to_string(),
        })?;

    let state_idx = find_state_index(states, &req.source_state_id).ok_or_else(|| {
        NexusApiError::NotFound(format!(
            "source state '{}' not found in Strategy '{strategy_id}'",
            req.source_state_id
        ))
    })?;

    let state_node = states
        .get_mut(state_idx)
        .ok_or_else(|| NexusApiError::Internal {
            code: "STRATEGY_STATE_INDEX".to_string(),
            message: "state index disappeared during transition patch".to_string(),
        })?;

    let next = state_node
        .get_mut("next")
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "strategy_transition_missing".to_string(),
            message: format!("state '{}' has no outgoing transition", req.source_state_id),
        })?;

    let (matched, side_effects) = apply_transition_patch(next, &req);

    if !matched {
        return Err(NexusApiError::BadRequest {
            code: "strategy_transition_not_found".to_string(),
            message: format!(
                "no transition from '{}' to '{}' matches the request",
                req.source_state_id, req.old_target
            ),
        });
    }

    let (errors, warnings) = validate_preset_yaml(&bundle_dir, &yaml_value)?;
    if !errors.is_empty() {
        return Err(NexusApiError::strategy_validation_failed(
            &errors, &warnings,
        ));
    }

    let new_revision = i64::try_from(current_revision.saturating_add(1)).unwrap_or(i64::MAX);
    write_preset_yaml(
        &bundle_dir,
        &mut yaml_value,
        current_revision.saturating_add(1),
    )?;

    Ok(Json(StrategyPatchResponse {
        new_revision,
        validation_summary: serde_json::json!({ "errors": [], "warnings": warnings }),
        side_effects: Some(side_effects),
    }))
}

/// `POST /v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch` — patch prompt template.
///
/// # Errors
///
/// Returns `strategy_id_mismatch` (400), `state_id_mismatch` (400),
/// `strategy_conflict` (409), `strategy_template_path_unsafe` (400),
/// `forbidden` (403), `invalid_input` (400), or `strategy_validation_failed` (422).
pub async fn patch_prompt_template(
    State(state): State<WorkspaceState>,
    Path((strategy_id, state_id)): Path<(String, String)>,
    Json(req): Json<StrategyPatchPromptTemplateRequest>,
) -> Result<Json<StrategyPatchResponse>, NexusApiError> {
    info!(strategy_id = %strategy_id, state_id = %state_id, template_ref = %req.template_ref, "Patching Strategy prompt template");
    ensure_id_matches(&strategy_id, &req.strategy_id, "strategy_id")?;
    ensure_id_matches(&state_id, &req.state_id, "state_id")?;

    let nexus_home = state.nexus_home();
    let (mut yaml_value, bundle_dir, current_revision) =
        load_user_preset_yaml(nexus_home, &strategy_id)?;

    if req.base_revision != current_revision {
        return Err(strategy_conflict(
            current_revision,
            &state_id,
            &format!("prompt:{}", req.template_ref),
            "refetch the Strategy and reapply your edit",
        ));
    }

    // Validate the template path is safe before touching the filesystem.
    nexus_orchestration::preset::loader::assert_template_file_safe(&req.template_ref).map_err(
        |reason| NexusApiError::BadRequest {
            code: "strategy_template_path_unsafe".to_string(),
            message: reason,
        },
    )?;

    // Resolve the bundle root canonically so the containment check works even
    // when the parent path contains symlinks (e.g., macOS /var → /private/var).
    let canonical_root =
        std::fs::canonicalize(&bundle_dir).map_err(|e| NexusApiError::Internal {
            code: "PATH_CANONICALIZE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    let template_path = canonical_root.join(&req.template_ref);

    // For existing files, canonicalize the concrete path; for new files the
    // join above already keeps us inside the bundle root (defence in depth).
    let canonical_template = if template_path.exists() {
        std::fs::canonicalize(&template_path).map_err(|e| NexusApiError::Internal {
            code: "PATH_CANONICALIZE_ERROR".to_string(),
            message: e.to_string(),
        })?
    } else {
        template_path
    };

    if !canonical_template.starts_with(&canonical_root) {
        return Err(NexusApiError::Forbidden {
            resource: "prompt_template".to_string(),
            reason: "template path resolves outside the bundle root".to_string(),
        });
    }

    // Ensure parent directory exists inside the bundle.
    if let Some(parent) = canonical_template.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
            code: "DIR_CREATE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    }

    let body = req
        .set
        .get("body")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "set.body".to_string(),
            reason: "prompt body is required".to_string(),
        })?;

    let mut side_effects: Vec<String> = Vec::new();
    std::fs::write(&canonical_template, body).map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".to_string(),
        message: e.to_string(),
    })?;
    side_effects.push(format!("wrote prompt template '{}'", req.template_ref));

    // Validate the manifest still loads with the new/updated template file.
    let (errors, warnings) = validate_preset_yaml(&bundle_dir, &yaml_value)?;
    if !errors.is_empty() {
        return Err(NexusApiError::strategy_validation_failed(
            &errors, &warnings,
        ));
    }

    let new_revision = i64::try_from(current_revision.saturating_add(1)).unwrap_or(i64::MAX);
    write_preset_yaml(
        &bundle_dir,
        &mut yaml_value,
        current_revision.saturating_add(1),
    )?;

    Ok(Json(StrategyPatchResponse {
        new_revision,
        validation_summary: serde_json::json!({ "errors": [], "warnings": warnings }),
        side_effects: Some(side_effects),
    }))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_strategy_id_rejects_empty() {
        assert!(validate_strategy_id("").is_err());
    }

    #[test]
    fn validate_strategy_id_rejects_slash() {
        assert!(validate_strategy_id("a/b").is_err());
    }

    #[test]
    fn validate_strategy_id_accepts_valid() {
        assert!(validate_strategy_id("my-strategy").is_ok());
    }

    #[test]
    fn find_state_index_finds_target() {
        let states = vec![
            serde_yaml::from_str::<serde_yaml::Value>("id: a").unwrap(),
            serde_yaml::from_str::<serde_yaml::Value>("id: b").unwrap(),
        ];
        assert_eq!(find_state_index(&states, "b"), Some(1));
        assert_eq!(find_state_index(&states, "c"), None);
    }

    #[test]
    fn rename_state_references_updates_next_and_initial() {
        let mut root = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
preset:
  initial: a
  terminal: b
states:
  - id: a
    next: b
  - id: b
    terminal: true
"#,
        )
        .unwrap();
        let effects = rename_state_references(&mut root, "a", "start").unwrap();
        assert!(effects.iter().any(|e| e.contains("preset.initial")));
        let states = root["states"].as_sequence().unwrap();
        assert_eq!(states[0]["id"].as_str(), Some("start"));
        assert_eq!(states[0]["next"].as_str(), Some("b"));
        assert_eq!(root["preset"]["initial"].as_str(), Some("start"));
    }

    /// Build a minimal valid user preset bundle for handler integration tests.
    fn seed_test_bundle(nexus_home: &std::path::Path, strategy_id: &str) -> std::path::PathBuf {
        let bundle_dir = nexus_home_layout::user_preset_bundle_dir(nexus_home, strategy_id);
        std::fs::create_dir_all(&bundle_dir).expect("create bundle dir");
        let yaml = r#"
revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Test strategy for patch handlers"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start state"
    next: end
  - id: end
    terminal: true
"#;
        std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write preset.yaml");
        bundle_dir
    }

    #[tokio::test]
    async fn patch_state_renames_state_and_bumps_revision() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        seed_test_bundle(&nexus_home, "test-strategy");
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

        let req = StrategyPatchStateRequest {
            strategy_id: "test-strategy".to_string(),
            state_id: "start".to_string(),
            base_revision: 1,
            set: serde_json::json!({ "label": "begin", "description": "Begin here." }),
        };
        let res = patch_state(
            State(state),
            Path(("test-strategy".to_string(), "start".to_string())),
            Json(req),
        )
        .await
        .expect("patch_state should succeed");

        assert_eq!(res.new_revision, 2);
        assert!(res
            .side_effects
            .as_ref()
            .unwrap()
            .iter()
            .any(|s| s.contains("begin")));

        let yaml = std::fs::read_to_string(
            nexus_home_layout::user_preset_bundle_dir(&nexus_home, "test-strategy")
                .join("preset.yaml"),
        )
        .unwrap();
        assert!(yaml.contains("id: begin"));
        assert!(yaml.contains("description: Begin here."));
        assert!(yaml.contains("revision: 2"));
    }

    #[tokio::test]
    async fn patch_state_rejects_stale_revision_with_conflict() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        seed_test_bundle(&nexus_home, "test-strategy");
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = StrategyPatchStateRequest {
            strategy_id: "test-strategy".to_string(),
            state_id: "start".to_string(),
            base_revision: 0, // stale
            set: serde_json::json!({ "description": "Stale edit." }),
        };
        let err = patch_state(
            State(state),
            Path(("test-strategy".to_string(), "start".to_string())),
            Json(req),
        )
        .await
        .expect_err("stale revision should fail");

        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
        assert_eq!(err.error_code(), "strategy_conflict");
    }

    #[tokio::test]
    async fn patch_transition_rewires_target() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        seed_test_bundle(&nexus_home, "test-strategy");
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

        let req = StrategyPatchTransitionRequest {
            strategy_id: "test-strategy".to_string(),
            base_revision: 1,
            source_state_id: "start".to_string(),
            old_target: "end".to_string(),
            new_target: Some("end".to_string()),
            condition: None,
            transition_kind: Some("next".to_string()),
        };
        // Sanity: rewriting to the same target is a no-op but should succeed.
        let res = patch_transition(State(state), Path("test-strategy".to_string()), Json(req))
            .await
            .expect("patch_transition should succeed");
        assert_eq!(res.new_revision, 2);
    }

    #[tokio::test]
    async fn patch_prompt_template_writes_file_and_bumps_revision() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let bundle_dir = seed_test_bundle(&nexus_home, "test-strategy");
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

        let req = StrategyPatchPromptTemplateRequest {
            strategy_id: "test-strategy".to_string(),
            state_id: "start".to_string(),
            base_revision: 1,
            template_ref: "prompts/start.md".to_string(),
            set: serde_json::json!({ "body": "# Hello\n" }),
        };
        let res = patch_prompt_template(
            State(state),
            Path(("test-strategy".to_string(), "start".to_string())),
            Json(req),
        )
        .await
        .expect("patch_prompt_template should succeed");

        assert_eq!(res.new_revision, 2);
        let body = std::fs::read_to_string(bundle_dir.join("prompts/start.md")).unwrap();
        assert_eq!(body, "# Hello\n");
    }
}
