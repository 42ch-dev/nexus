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
use std::os::fd::AsRawFd;
use tracing::info;

/// Advisory lock file inside a user preset bundle.
///
/// Provides cross-process serialization for Strategy patch operations. The lock
/// is acquired before any read/check/write sequence and released when the guard
/// drops. This prevents the TOCTOU race where two writers starting from the
/// same `base_revision` both commit `revision: N+1`.
const STRATEGY_LOCK_FILE: &str = ".strategy-lock";

/// Maximum YAML file size for a user preset (1 MiB).
const PRESET_MAX_YAML_SIZE: usize = 1024 * 1024;

/// Maximum YAML nesting depth for a user preset.
const PRESET_MAX_YAML_DEPTH: usize = 10;

// ─── Cross-process advisory lock ───────────────────────────────────────────

/// RAII guard holding an exclusive `flock` on the strategy bundle lock file.
#[derive(Debug)]
struct StrategyLockGuard {
    fd: std::fs::File,
}

impl Drop for StrategyLockGuard {
    fn drop(&mut self) {
        let raw_fd = self.fd.as_raw_fd();
        #[allow(deprecated)]
        {
            if let Err(e) = nix::fcntl::flock(raw_fd, nix::fcntl::FlockArg::Unlock) {
                tracing::error!(error = %e, "strategy patch: failed to release flock");
            }
        }
    }
}

/// Acquire an exclusive advisory lock for a strategy bundle.
///
/// Blocks until the lock is available. All mutating strategy operations must
/// hold this lock for the entire load → CAS → validate → write sequence.
fn acquire_strategy_lock(bundle_dir: &std::path::Path) -> Result<StrategyLockGuard, NexusApiError> {
    let lock_path = bundle_dir.join(STRATEGY_LOCK_FILE);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
            code: "DIR_CREATE_ERROR".to_string(),
            message: format!("cannot create bundle directory: {e}"),
        })?;
    }

    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| NexusApiError::Internal {
            code: "LOCK_OPEN_ERROR".to_string(),
            message: format!("cannot open strategy lock file: {e}"),
        })?;

    let raw_fd = fd.as_raw_fd();
    #[allow(deprecated)]
    nix::fcntl::flock(raw_fd, nix::fcntl::FlockArg::LockExclusive).map_err(|e| {
        NexusApiError::Internal {
            code: "LOCK_ACQUIRE_ERROR".to_string(),
            message: format!("cannot acquire strategy lock: {e}"),
        }
    })?;

    Ok(StrategyLockGuard { fd })
}

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
/// Uses a request-unique temp file + rename + fsync (file and parent directory)
/// for atomicity. The parent directory fsync ensures the rename entry is durable
/// on POSIX filesystems.
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
    let suffix = uuid::Uuid::new_v4().to_string();
    let tmp_path = bundle_root.join(format!("preset.yaml.tmp.{suffix}"));

    let yaml_str = serde_yaml::to_string(value).map_err(|e| NexusApiError::Internal {
        code: "YAML_SERIALIZE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    atomic_write_with_dir_fsync(&yaml_path, &tmp_path, yaml_str.as_bytes())?;

    Ok(())
}

/// Atomically write `content` to `target_path` using `tmp_path`, then rename.
///
/// Syncs the temp file, renames, and fsyncs the parent directory. Cleans up
/// `tmp_path` on error.
fn atomic_write_with_dir_fsync(
    target_path: &std::path::Path,
    tmp_path: &std::path::Path,
    content: &[u8],
) -> Result<(), NexusApiError> {
    std::fs::write(tmp_path, content).map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".to_string(),
        message: format!("cannot write temp file {}: {e}", tmp_path.display()),
    })?;

    let file = std::fs::File::open(tmp_path).map_err(|e| NexusApiError::Internal {
        code: "FILE_SYNC_ERROR".to_string(),
        message: format!("cannot open temp file for fsync: {e}"),
    })?;
    file.sync_all().map_err(|e| NexusApiError::Internal {
        code: "FILE_SYNC_ERROR".to_string(),
        message: format!("cannot fsync temp file: {e}"),
    })?;
    drop(file);

    std::fs::rename(tmp_path, target_path).map_err(|e| {
        let _ = std::fs::remove_file(tmp_path);
        NexusApiError::Internal {
            code: "FILE_RENAME_ERROR".to_string(),
            message: format!(
                "cannot rename {} to {}: {e}",
                tmp_path.display(),
                target_path.display()
            ),
        }
    })?;

    // fsync parent directory so the rename is durable.
    if let Some(parent) = target_path.parent() {
        let dir = std::fs::File::open(parent).map_err(|e| NexusApiError::Internal {
            code: "DIR_SYNC_ERROR".to_string(),
            message: format!("cannot open bundle directory for fsync: {e}"),
        })?;
        if let Err(e) = dir.sync_all() {
            tracing::warn!(
                parent = %parent.display(),
                error = %e,
                "strategy patch: directory fsync failed"
            );
        }
    }

    Ok(())
}

/// Back up the existing file at `path` (if any) and return the backup bytes.
fn backup_existing_file(path: &std::path::Path) -> Result<Option<Vec<u8>>, NexusApiError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| NexusApiError::Internal {
        code: "FILE_READ_ERROR".to_string(),
        message: format!("cannot back up {}: {e}", path.display()),
    })?;
    Ok(Some(bytes))
}

/// Restore `path` from `backup` and remove any leftover temp file.
///
/// The restore itself is atomic: backup bytes are written to a fresh temp file
/// in the same directory, fsync'd, and renamed over `path`, followed by a
/// directory fsync. This mirrors the V1.72 outline markdown persistence pattern
/// so a crash mid-rollback cannot leave the template truncated while the YAML
/// revision has already been restored.
fn rollback_template_write(
    path: &std::path::Path,
    backup: Option<Vec<u8>>,
    tmp_path: &std::path::Path,
) {
    let _ = std::fs::remove_file(tmp_path);
    match backup {
        Some(bytes) => {
            let rollback_tmp = if let Some(name) = path.file_name() {
                let mut tmp_name = name.to_os_string();
                tmp_name.push(format!(
                    ".rollback.{}.{}",
                    std::process::id(),
                    uuid::Uuid::new_v4()
                ));
                path.with_file_name(&tmp_name)
            } else {
                // The caller always passes a file path, but if that invariant
                // ever breaks, still attempt the rollback via a non-atomic write
                // rather than leaving the file in an inconsistent state.
                if let Err(e) = std::fs::write(path, &bytes) {
                    tracing::error!(
                        path = %path.display(),
                        error = %e,
                        "strategy patch: failed to roll back prompt template after validation failure"
                    );
                }
                return;
            };
            if let Err(e) = atomic_write_with_dir_fsync(path, &rollback_tmp, &bytes) {
                tracing::error!(
                    path = %path.display(),
                    error = %e,
                    "strategy patch: failed to roll back prompt template after validation failure"
                );
            }
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Validate that a transition condition string parses against the preset
/// condition grammar. Conditional `when:` values in user presets are expression
/// strings; an unparsable condition must be rejected before persistence.
fn validate_transition_condition(condition: &str) -> Result<(), NexusApiError> {
    nexus_orchestration::preset::expr::parse(condition)
        .map(|_| ())
        .map_err(|e| NexusApiError::BadRequest {
            code: "strategy_transition_condition_invalid".to_string(),
            message: format!("transition condition is not a valid expression: {e}"),
        })
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

    let nexus_home = state.nexus_home().clone();
    let strategy_id = strategy_id.clone();
    let state_id = state_id.clone();
    let req = req.clone();

    let response = tokio::task::spawn_blocking(move || {
        patch_state_inner(&nexus_home, &strategy_id, &state_id, &req)
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "PATCH_TASK_ERROR".to_string(),
        message: format!("patch_state task failed: {e}"),
    })?;

    response.map(Json)
}

fn patch_state_inner(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    state_id: &str,
    req: &StrategyPatchStateRequest,
) -> Result<StrategyPatchResponse, NexusApiError> {
    let bundle_dir = user_preset_bundle_dir(nexus_home, strategy_id);
    let _guard = acquire_strategy_lock(&bundle_dir)?;

    // Load the canonical YAML while holding the lock so the revision check is
    // not subject to TOCTOU.
    let (mut yaml_value, bundle_dir, current_revision) =
        load_user_preset_yaml(nexus_home, strategy_id)?;

    if req.base_revision != current_revision {
        return Err(strategy_conflict(
            current_revision,
            state_id,
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

    let idx = find_state_index(states, state_id).ok_or_else(|| {
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
            side_effects = rename_state_references(&mut yaml_value, state_id, &new_label)?;
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

    let new_revision = current_revision.saturating_add(1);
    write_preset_yaml(&bundle_dir, &mut yaml_value, new_revision)?;

    Ok(StrategyPatchResponse {
        new_revision: i64::try_from(new_revision).unwrap_or(i64::MAX),
        validation_summary: serde_json::json!({ "errors": [], "warnings": warnings }),
        side_effects: Some(side_effects),
    })
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

    let nexus_home = state.nexus_home().clone();
    let strategy_id = strategy_id.clone();
    let req = req.clone();

    let response = tokio::task::spawn_blocking(move || {
        patch_transition_inner(&nexus_home, &strategy_id, &req)
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "PATCH_TASK_ERROR".to_string(),
        message: format!("patch_transition task failed: {e}"),
    })?;

    response.map(Json)
}

fn patch_transition_inner(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    req: &StrategyPatchTransitionRequest,
) -> Result<StrategyPatchResponse, NexusApiError> {
    let bundle_dir = user_preset_bundle_dir(nexus_home, strategy_id);
    let _guard = acquire_strategy_lock(&bundle_dir)?;

    let (mut yaml_value, bundle_dir, current_revision) =
        load_user_preset_yaml(nexus_home, strategy_id)?;

    if req.base_revision != current_revision {
        return Err(strategy_conflict(
            current_revision,
            &req.source_state_id,
            "transitions",
            "refetch the Strategy and reapply your edit",
        ));
    }

    // Reject an unparsable condition before touching YAML so the file is never
    // left with a bad expression.
    if let Some(condition) = &req.condition {
        validate_transition_condition(condition)?;
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

    let (matched, side_effects) = apply_transition_patch(next, req);

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

    let new_revision = current_revision.saturating_add(1);
    write_preset_yaml(&bundle_dir, &mut yaml_value, new_revision)?;

    Ok(StrategyPatchResponse {
        new_revision: i64::try_from(new_revision).unwrap_or(i64::MAX),
        validation_summary: serde_json::json!({ "errors": [], "warnings": warnings }),
        side_effects: Some(side_effects),
    })
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

    let nexus_home = state.nexus_home().clone();
    let strategy_id = strategy_id.clone();
    let state_id = state_id.clone();
    let req = req.clone();

    let response = tokio::task::spawn_blocking(move || {
        patch_prompt_template_inner(&nexus_home, &strategy_id, &state_id, &req)
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "PATCH_TASK_ERROR".to_string(),
        message: format!("patch_prompt_template task failed: {e}"),
    })?;

    response.map(Json)
}

fn patch_prompt_template_inner(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    state_id: &str,
    req: &StrategyPatchPromptTemplateRequest,
) -> Result<StrategyPatchResponse, NexusApiError> {
    patch_prompt_template_inner_with_writer(
        nexus_home,
        strategy_id,
        state_id,
        req,
        write_preset_yaml,
    )
}

/// Injected YAML writer for tests so filesystem failures after the template
/// rename can be exercised deterministically.
type PresetYamlWriter =
    fn(&std::path::Path, &mut serde_yaml::Value, u64) -> Result<(), NexusApiError>;

fn patch_prompt_template_inner_with_writer(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    state_id: &str,
    req: &StrategyPatchPromptTemplateRequest,
    write_yaml: PresetYamlWriter,
) -> Result<StrategyPatchResponse, NexusApiError> {
    let bundle_dir = user_preset_bundle_dir(nexus_home, strategy_id);
    let _guard = acquire_strategy_lock(&bundle_dir)?;

    let (mut yaml_value, bundle_dir, current_revision) =
        load_user_preset_yaml(nexus_home, strategy_id)?;

    if req.base_revision != current_revision {
        return Err(strategy_conflict(
            current_revision,
            state_id,
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

    // Stage the new template with a request-unique temp file, then rename it
    // into place before validating the manifest. If validation fails we roll
    // back to the previous file contents and never bump YAML revision.
    let backup = backup_existing_file(&canonical_template)?;

    let file_name = canonical_template
        .file_name()
        .ok_or_else(|| NexusApiError::Internal {
            code: "TEMPLATE_PATH_INVALID".to_string(),
            message: "template path has no file name".to_string(),
        })?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(format!(".tmp.{}", uuid::Uuid::new_v4()));
    let tmp_path = canonical_template.with_file_name(&tmp_name);

    // Use the same atomic-write helper as the YAML path so the template file
    // and the parent directory are fsync'd before and after the rename. Without
    // this, a crash between the write and the OS page-cache flush could leave
    // the template empty/truncated while the YAML revision has already been
    // bumped. R-V171-GREPTILE-P1-2.
    atomic_write_with_dir_fsync(&canonical_template, &tmp_path, body.as_bytes())?;

    // Validate the manifest still loads with the new/updated template file.
    let (errors, warnings) = validate_preset_yaml(&bundle_dir, &yaml_value)?;
    if !errors.is_empty() {
        rollback_template_write(&canonical_template, backup, &tmp_path);
        return Err(NexusApiError::strategy_validation_failed(
            &errors, &warnings,
        ));
    }

    // Persist the YAML revision only after the template file has been
    // committed. If YAML persistence fails, roll the template back so the
    // on-disk prompt bytes and the YAML revision can never diverge.
    let new_revision = current_revision.saturating_add(1);
    if let Err(e) = write_yaml(&bundle_dir, &mut yaml_value, new_revision) {
        rollback_template_write(&canonical_template, backup, &tmp_path);
        return Err(e);
    }

    let mut side_effects: Vec<String> = Vec::new();
    side_effects.push(format!("wrote prompt template '{}'", req.template_ref));

    Ok(StrategyPatchResponse {
        new_revision: i64::try_from(new_revision).unwrap_or(i64::MAX),
        validation_summary: serde_json::json!({ "errors": [], "warnings": warnings }),
        side_effects: Some(side_effects),
    })
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

    #[tokio::test]
    async fn patch_transition_rejects_invalid_condition() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        seed_test_bundle(&nexus_home, "test-strategy");
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = StrategyPatchTransitionRequest {
            strategy_id: "test-strategy".to_string(),
            base_revision: 1,
            source_state_id: "start".to_string(),
            old_target: "end".to_string(),
            new_target: None,
            condition: Some("not a valid expression @#$".to_string()),
            transition_kind: None,
        };
        let err = patch_transition(State(state), Path("test-strategy".to_string()), Json(req))
            .await
            .expect_err("invalid condition should fail");

        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
        match err {
            NexusApiError::BadRequest { code, .. } => {
                assert_eq!(code, "strategy_transition_condition_invalid");
            }
            other => panic!("expected BadRequest with condition code, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn patch_prompt_template_rolls_back_on_validation_failure() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let bundle_dir = user_preset_bundle_dir(&nexus_home, "test-strategy");
        std::fs::create_dir_all(&bundle_dir).expect("create bundle dir");

        // The manifest references a missing asset so validation will fail after
        // the prompt template is staged.
        let yaml = r#"
revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Test strategy for rollback"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start state"
    context_update:
      op:
        kind: append
        body: ""
      template_file: prompts/missing.md
    next: end
  - id: end
    terminal: true
"#;
        std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write preset.yaml");

        std::fs::create_dir_all(bundle_dir.join("prompts")).expect("create prompts dir");
        let other_path = bundle_dir.join("prompts/other.md");
        std::fs::write(&other_path, "original content").expect("write original template");

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = StrategyPatchPromptTemplateRequest {
            strategy_id: "test-strategy".to_string(),
            state_id: "start".to_string(),
            base_revision: 1,
            template_ref: "prompts/other.md".to_string(),
            set: serde_json::json!({ "body": "new content" }),
        };
        let err = patch_prompt_template(
            State(state),
            Path(("test-strategy".to_string(), "start".to_string())),
            Json(req),
        )
        .await
        .expect_err("validation failure should roll back");

        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(err.error_code(), "strategy_validation_failed");

        // The template file must be restored to its original content.
        let body = std::fs::read_to_string(&other_path).unwrap();
        assert_eq!(body, "original content");
    }

    #[tokio::test]
    async fn concurrent_patch_state_serializes_on_lock() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        seed_test_bundle(&nexus_home, "test-strategy");
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

        let req_a = StrategyPatchStateRequest {
            strategy_id: "test-strategy".to_string(),
            state_id: "start".to_string(),
            base_revision: 1,
            set: serde_json::json!({ "description": "A" }),
        };
        let req_b = StrategyPatchStateRequest {
            strategy_id: "test-strategy".to_string(),
            state_id: "start".to_string(),
            base_revision: 1,
            set: serde_json::json!({ "description": "B" }),
        };

        let state_a = state.clone();
        let task_a = tokio::spawn(async move {
            patch_state(
                State(state_a),
                Path(("test-strategy".to_string(), "start".to_string())),
                Json(req_a),
            )
            .await
        });

        let task_b = tokio::spawn(async move {
            // Small delay so both requests are in flight and contend on the lock.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            patch_state(
                State(state),
                Path(("test-strategy".to_string(), "start".to_string())),
                Json(req_b),
            )
            .await
        });

        let (res_a, res_b) = tokio::join!(task_a, task_b);
        let outcomes = [res_a.unwrap(), res_b.unwrap()];
        let successes = outcomes.iter().filter(|r| r.is_ok()).count();
        let conflicts = outcomes
            .iter()
            .filter(|r| {
                r.as_ref()
                    .err()
                    .is_some_and(|e| e.error_code() == "strategy_conflict")
            })
            .count();

        assert_eq!(successes, 1, "exactly one concurrent patch should succeed");
        assert_eq!(
            conflicts, 1,
            "the other concurrent patch should get a conflict"
        );

        let yaml = std::fs::read_to_string(
            user_preset_bundle_dir(&nexus_home, "test-strategy").join("preset.yaml"),
        )
        .unwrap();
        assert!(yaml.contains("revision: 2"));
    }

    #[tokio::test]
    async fn patch_prompt_template_rolls_back_on_yaml_persistence_failure() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        fn failing_yaml_writer(
            _bundle_root: &std::path::Path,
            _value: &mut serde_yaml::Value,
            _revision: u64,
        ) -> Result<(), NexusApiError> {
            Err(NexusApiError::Internal {
                code: "INJECTED_YAML_WRITE_ERROR".to_string(),
                message: "injected yaml persistence failure".to_string(),
            })
        }

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let bundle_dir = user_preset_bundle_dir(&nexus_home, "test-strategy");
        std::fs::create_dir_all(&bundle_dir).expect("create bundle dir");

        let yaml = r#"
revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Test strategy for yaml rollback"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start state"
    context_update:
      op:
        kind: append
        body: ""
      template_file: prompts/original.md
    next: end
  - id: end
    terminal: true
"#;
        std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write preset.yaml");

        std::fs::create_dir_all(bundle_dir.join("prompts")).expect("create prompts dir");
        let template_path = bundle_dir.join("prompts/original.md");
        std::fs::write(&template_path, "original content").expect("write original template");

        // Pre-load the WorkspaceState so the registry is available for validation,
        // but exercise the synchronous inner function directly to inject a YAML
        // writer that fails after the template rename.
        let _state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

        let req = StrategyPatchPromptTemplateRequest {
            strategy_id: "test-strategy".to_string(),
            state_id: "start".to_string(),
            base_revision: 1,
            template_ref: "prompts/original.md".to_string(),
            set: serde_json::json!({ "body": "new content" }),
        };

        let err = patch_prompt_template_inner_with_writer(
            &nexus_home,
            "test-strategy",
            "start",
            &req,
            failing_yaml_writer,
        )
        .expect_err("yaml persistence failure should roll back");

        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(err.error_code(), "internal");

        // The prompt template must be rolled back to its original bytes.
        let body = std::fs::read_to_string(&template_path).unwrap();
        assert_eq!(body, "original content");

        // The YAML revision must not have advanced.
        let yaml = std::fs::read_to_string(bundle_dir.join("preset.yaml")).unwrap();
        assert!(yaml.contains("revision: 1"));
        assert!(!yaml.contains("revision: 2"));
    }

    #[test]
    fn rollback_template_write_restores_original_bytes_atomically() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("prompt.md");
        let original = b"original content";
        std::fs::write(&path, original).expect("write original template");

        let backup = backup_existing_file(&path).expect("backup original");
        assert!(backup.is_some());

        let tmp_path = dir
            .path()
            .join("prompt.md.tmp.00000000-0000-0000-0000-000000000000");
        atomic_write_with_dir_fsync(&path, &tmp_path, b"new content that should be rolled back")
            .expect("atomic write new content");

        rollback_template_write(&path, backup, &tmp_path);

        let restored = std::fs::read(&path).expect("read restored template");
        assert_eq!(restored, original);
        assert!(!tmp_path.exists(), "staged temp file must be removed");
    }

    #[test]
    fn rollback_template_write_removes_file_when_no_backup() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("new_prompt.md");
        let tmp_path = dir
            .path()
            .join("new_prompt.md.tmp.00000000-0000-0000-0000-000000000000");

        atomic_write_with_dir_fsync(&path, &tmp_path, b"new content that should be removed")
            .expect("atomic write new file");
        assert!(path.exists());

        rollback_template_write(&path, None, &tmp_path);

        assert!(!path.exists(), "new file must be removed on rollback");
        assert!(!tmp_path.exists(), "staged temp file must be removed");
    }
}
